//! Integration tests for the HTTP layer (spec §6: fake-sysroot harness, no
//! hardware). Covers the highest-risk cross-cutting concerns:
//!   - first-boot admin creation gate (CRA/SB-327, no default password)
//!   - CSRF double-submit enforcement on mutations
//!   - role-based access (admin vs viewer)
//!   - the networking commit/confirm rollback timer (spec §4.2 flagship)
//!
//! Lives in-crate (gated `#[cfg(test)]`) because `webconfigd` is a binary crate
//! and binary crates don't expose a lib target to `tests/`. The fake sysroot
//! means no host side effects and parallel-safe execution.

#![cfg(test)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use webconfig_engine::alert::AlertEngine;
use webconfig_engine::audit::AuditLog;
use webconfig_engine::manifest::{BoardManifest, IoKind};
use webconfig_engine::schema::Schema;
use webconfig_engine::{ConfigStore, Sysroot};

use crate::engine_bridge::{EngineBridge, PendingConfirm};
use crate::state::AppState;

/// Build the full app router against a tempdir fake sysroot. Returns the router
/// and the tempdir (caller must keep the tempdir alive for the test duration).
async fn build_app() -> (axum::Router, tempfile::TempDir) {
    build_app_with(Schema::default_permissive()).await
}

/// Like [`build_app`] but with a caller-supplied schema (e.g. one loaded from a
/// tempdir to exercise schema-driven paths like app `apply_hook`s).
async fn build_app_with(schema: Schema) -> (axum::Router, tempfile::TempDir) {
    build_app_with_systemd(schema, None).await
}

/// Like [`build_app_with`] but lets the caller substitute the systemd client
/// (e.g. a client that fails every verb) to exercise error-path handling that
/// the fake sysroot's always-succeeding mock can't reach.
async fn build_app_with_systemd(
    schema: Schema,
    systemd: Option<Arc<dyn crate::systemd::SystemdClient>>,
) -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let sysroot = Sysroot::at(dir.path());
    let webconfig_dir = sysroot.webconfig_dir();
    std::fs::create_dir_all(&webconfig_dir).ok();

    let store = ConfigStore::new(sysroot.clone(), schema.clone());
    let engine = EngineBridge::new(store, sysroot.clone());

    // Minimal manifest with a service allowlist + one peripheral so the I/O and
    // services pages have data.
    let manifest = BoardManifest {
        platform: "arches".into(),
        peripherals: vec![
            webconfig_engine::manifest::Peripheral {
                id: "can0".into(),
                device: "can0".into(),
                human_name: "CAN 0".into(),
                kind: IoKind::Can,
                ..Default::default()
            },
            webconfig_engine::manifest::Peripheral {
                id: "ttyTHS0".into(),
                device: "/dev/ttyTHS0".into(),
                human_name: "High-speed UART 0".into(),
                kind: IoKind::Serial,
                ..Default::default()
            },
        ],
        service_allowlist: vec!["sshd.service".into()],
        default_alert_rules: vec![
            webconfig_engine::store::alerts::AlertRule {
                id: "disk-full".into(),
                enabled: true,
                metric: "disk.pct".into(),
                comparison: ">".into(),
                threshold: 90.0,
                duration_secs: 60,
                actions: vec!["ui".into()],
            },
            webconfig_engine::store::alerts::AlertRule {
                id: "watchdog-reset".into(),
                enabled: true,
                metric: "boot.watchdog".into(),
                comparison: ">".into(),
                threshold: 0.0,
                duration_secs: 0,
                actions: vec!["ui".into()],
            },
        ],
        ..Default::default()
    };

    let state = AppState {
        engine: engine.clone(),
        schema: Arc::new(schema),
        manifest: Arc::new(manifest),
        audit: Arc::new(AuditLog::new(webconfig_dir.join("audit.jsonl"), 64 * 1024)),
        alert_engine: Arc::new(tokio::sync::Mutex::new(AlertEngine::new())),
        update_backend: crate::update_backend::make_backend(&sysroot),
        systemd: match systemd {
            Some(s) => s,
            None => crate::systemd::make_client(&sysroot).await,
        },
        users_path: webconfig_dir.join("users.json"),
        login_throttle: Arc::new(crate::auth::LoginThrottle::new()),
    };

    // Static dir: a tempdir is fine; API routes don't depend on it.
    let static_dir = tempfile::tempdir().unwrap().keep();
    let router = crate::routes::build_router(state, static_dir);
    (router, dir)
}

/// Read the full response body as a UTF-8 string.
async fn body_text(body: Body) -> String {
    String::from_utf8_lossy(&body.collect().await.unwrap().to_bytes()).to_string()
}

/// Extract the `id=<value>` session cookie from a Set-Cookie header value.
fn session_cookie(set_cookie: &str) -> String {
    let id = set_cookie.split(';').next().unwrap_or("").trim();
    id.to_string()
}

/// Send a request and return (status, set-cookie header if any, body text).
async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    csrf: Option<&str>,
    body: Option<&str>,
) -> (StatusCode, Option<String>, String) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    if let Some(t) = csrf {
        b = b.header("X-CSRF-Token", t);
    }
    if body.is_some() {
        b = b.header("content-type", "application/json");
    }
    let req = b.body(Body::from(body.unwrap_or("").to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(session_cookie);
    let text = body_text(resp.into_body()).await;
    (status, set_cookie, text)
}

#[tokio::test]
async fn first_boot_gate_blocks_protected_routes_until_admin_created() {
    let (app, _dir) = build_app().await;

    // whoami before any admin exists → first_boot true, no username.
    let (st, _ck, text) = send(&app, "GET", "/api/v1/auth/whoami", None, None, None).await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["first_boot"], true);
    assert!(v["username"].is_null());

    // A protected route is refused before first boot.
    let (st, _, _) = send(&app, "GET", "/api/v1/system", None, None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // Login is refused before first boot (no admin yet).
    let (st, _, _) = send(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        None,
        Some(r#"{"username":"admin","password":"whatever1"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE); // NotReady
}

#[tokio::test]
async fn first_boot_creates_admin_and_authenticates() {
    let (app, _dir) = build_app().await;

    let (st, ck, _) = send(
        &app,
        "POST",
        "/api/v1/auth/first-boot",
        None,
        None,
        Some(r#"{"username":"admin","password":"password1"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let cookie = ck.expect("first-boot sets a session cookie");

    // Now whoami reports an authenticated admin.
    let (st, _, text) = send(
        &app,
        "GET",
        "/api/v1/auth/whoami",
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["username"], "admin");
    assert_eq!(v["role"], "admin");
    let csrf = v["csrf"].as_str().expect("csrf token present").to_string();

    // A protected GET now succeeds.
    let (st, _, _) = send(&app, "GET", "/api/v1/system", Some(&cookie), None, None).await;
    assert_eq!(st, StatusCode::OK);

    // A mutation WITH the csrf token succeeds.
    let (st, _, _) = send(
        &app, "PUT", "/api/v1/system", Some(&cookie), Some(&csrf),
        Some(r#"{"hostname":"arches-01","asset_tag":null,"time":{"timezone":null,"ntp_enabled":false,"manual_datetime":null},"ssh":{"sshd_enabled":false,"serial_getty_enabled":false,"authorized_keys":""},"ntp_servers":[]}"#),
    ).await;
    assert_eq!(st, StatusCode::OK);
}

#[tokio::test]
async fn csrf_guard_rejects_mutations_without_token() {
    let (app, _dir) = build_app().await;
    let (_, ck, _) = send(
        &app,
        "POST",
        "/api/v1/auth/first-boot",
        None,
        None,
        Some(r#"{"username":"admin","password":"password1"}"#),
    )
    .await;
    let cookie = ck.unwrap();

    // PUT /system with a session but NO csrf token → rejected by csrf_guard
    // (returns 400 BadRequest, per csrf.rs).
    let (st, _, _) = send(
        &app, "PUT", "/api/v1/system", Some(&cookie), None,
        Some(r#"{"hostname":"x","asset_tag":null,"time":{"timezone":null,"ntp_enabled":false,"manual_datetime":null},"ssh":{"sshd_enabled":false,"serial_getty_enabled":false,"authorized_keys":""},"ntp_servers":[]}"#),
    ).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn viewer_role_cannot_mutate_admin_only_routes() {
    let (app, _dir) = build_app().await;

    // Bootstrap admin.
    let (_, admin_ck, _) = send(
        &app,
        "POST",
        "/api/v1/auth/first-boot",
        None,
        None,
        Some(r#"{"username":"admin","password":"password1"}"#),
    )
    .await;
    let admin_cookie = admin_ck.unwrap();
    let (_, _, text) = send(
        &app,
        "GET",
        "/api/v1/auth/whoami",
        Some(&admin_cookie),
        None,
        None,
    )
    .await;
    let admin_csrf = serde_json::from_str::<serde_json::Value>(&text).unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_string();

    // Admin creates a viewer.
    let (st, _, _) = send(
        &app,
        "POST",
        "/api/v1/system/users",
        Some(&admin_cookie),
        Some(&admin_csrf),
        Some(r#"{"username":"watcher","password":"password1","role":"viewer"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // Viewer logs in on its own session.
    let (st, viewer_ck, _) = send(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        None,
        Some(r#"{"username":"watcher","password":"password1"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let viewer_cookie = viewer_ck.unwrap();
    let (_, _, text) = send(
        &app,
        "GET",
        "/api/v1/auth/whoami",
        Some(&viewer_cookie),
        None,
        None,
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["role"], "viewer");
    let viewer_csrf = v["csrf"].as_str().unwrap().to_string();

    // Viewer can read system (any authenticated role).
    let (st, _, _) = send(
        &app,
        "GET",
        "/api/v1/system",
        Some(&viewer_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // Viewer cannot PUT /system (admin only) even with a valid csrf token.
    let (st, _, _) = send(
        &app, "PUT", "/api/v1/system", Some(&viewer_cookie), Some(&viewer_csrf),
        Some(r#"{"hostname":"x","asset_tag":null,"time":{"timezone":null,"ntp_enabled":false,"manual_datetime":null},"ssh":{"sshd_enabled":false,"serial_getty_enabled":false,"authorized_keys":""},"ntp_servers":[]}"#),
    ).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn confirm_timer_rolls_back_when_window_expires() {
    // The flagship commit/confirm rollback (spec §4.2): a provisional apply
    // not confirmed within its window is auto-rolled-back by tick_confirm.
    let dir = tempfile::tempdir().unwrap();
    let sysroot = Sysroot::at(dir.path());
    let store = ConfigStore::new(sysroot.clone(), Schema::default_permissive());
    let bridge = EngineBridge::new(store, sysroot.clone());

    // Plant a provisional apply with a zero-second window.
    bridge
        .confirm
        .set(PendingConfirm {
            applier_id: "networkd".into(),
            window_secs: 0,
            started_at: std::time::Instant::now(),
        })
        .await;

    // Let an instant elapse so elapsed(0) >= window(0).
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;

    // tick_confirm should consume the pending entry and roll back → true.
    let rolled = bridge.tick_confirm().await;
    assert!(rolled, "expired provisional apply must roll back");

    // After rollback, no pending entry remains; a further tick is a no-op.
    let again = bridge.tick_confirm().await;
    assert!(!again, "no pending apply → nothing to roll back");
}

#[tokio::test]
async fn confirm_pending_clears_on_explicit_confirm() {
    let dir = tempfile::tempdir().unwrap();
    let sysroot = Sysroot::at(dir.path());
    let store = ConfigStore::new(sysroot.clone(), Schema::default_permissive());
    let bridge = EngineBridge::new(store, sysroot);

    bridge
        .confirm
        .set(PendingConfirm {
            applier_id: "networkd".into(),
            window_secs: 600,
            started_at: std::time::Instant::now(),
        })
        .await;

    // Explicit confirm before the window expires → stands, no rollback.
    let confirmed = bridge.confirm_pending().await.unwrap();
    assert!(
        confirmed,
        "confirm_pending reports a consumed pending apply"
    );
    let rolled = bridge.tick_confirm().await;
    assert!(!rolled, "confirmed apply must not be rolled back");
}

#[tokio::test]
async fn app_settings_put_fires_schema_apply_hook() {
    // The schema-driven app settings engine must run the declared `apply_hook`
    // after a settings save (spec §3/§4.3): a `restart` hook reaches the
    // systemd client and is recorded in the audit trail.
    let schema_dir = tempfile::tempdir().unwrap();
    let schema = r#"{
        "id": "app:demo",
        "title": "Demo app",
        "groups": [
            {
                "id": "main",
                "title": "Main",
                "fields": [
                    {
                        "id": "site_id",
                        "title": "Site ID",
                        "field_type": "string",
                        "apply_hook": { "kind": "restart", "target": "demo.service" }
                    }
                ]
            }
        ]
    }"#;
    std::fs::write(schema_dir.path().join("app-demo.schema.json"), schema).unwrap();
    let loaded = Schema::load_dir(schema_dir.path()).unwrap();
    let (app, _dir) = build_app_with(loaded).await;

    // Bootstrap admin + session + csrf.
    let (st, ck, _) = send(
        &app,
        "POST",
        "/api/v1/auth/first-boot",
        None,
        None,
        Some(r#"{"username":"admin","password":"password1"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let cookie = ck.unwrap();
    let (_, _, text) = send(
        &app,
        "GET",
        "/api/v1/auth/whoami",
        Some(&cookie),
        None,
        None,
    )
    .await;
    let csrf = serde_json::from_str::<serde_json::Value>(&text).unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_string();

    // PUT app settings → the restart hook fires (mock systemd accepts it) and
    // is recorded in the audit entry for this save.
    let (st, _, body) = send(
        &app,
        "PUT",
        "/api/v1/apps/app-demo/settings",
        Some(&cookie),
        Some(&csrf),
        Some(r#"{"site_id":"site-1"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body, r#"{"site_id":"site-1"}"#);

    // The audit log entry carries the side effect that ran.
    let (st, _, audit) = send(&app, "GET", "/api/v1/logs/audit", Some(&cookie), None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        audit.contains("restart demo.service"),
        "audit should record the apply_hook: {audit}"
    );
}

/// Bootstrap the first admin and return (cookie, csrf) for an authenticated
/// admin session — the common preamble for the mutation tests below.
async fn boot_admin(app: &axum::Router) -> (String, String) {
    let (_, ck, _) = send(
        app,
        "POST",
        "/api/v1/auth/first-boot",
        None,
        None,
        Some(r#"{"username":"admin","password":"password1"}"#),
    )
    .await;
    let cookie = ck.unwrap();
    let (_, _, text) = send(app, "GET", "/api/v1/auth/whoami", Some(&cookie), None, None).await;
    let csrf = serde_json::from_str::<serde_json::Value>(&text).unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_string();
    (cookie, csrf)
}

#[tokio::test]
async fn updates_confirm_marks_good_and_audits() {
    // Spec §4.4: a freshly booted image is manually confirmed (mark-good) so
    // the bootloader does not roll it back. POST /updates/confirm is admin-only
    // and records an audit entry.
    let (app, _dir) = build_app().await;
    let (cookie, csrf) = boot_admin(&app).await;

    let (st, _, body) = send(
        &app,
        "POST",
        "/api/v1/updates/confirm",
        Some(&cookie),
        Some(&csrf),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body, r#"{"confirmed":true}"#);

    let (st, _, audit) = send(&app, "GET", "/api/v1/logs/audit", Some(&cookie), None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        audit.contains("updates.confirm"),
        "audit should record the mark-good: {audit}"
    );
}

#[tokio::test]
async fn updates_diagnostics_bundle_carries_journal_and_network_state() {
    // Spec §4.4 diagnostics bundle must include a journal excerpt and the live
    // network state (alongside config/slots/dmesg/alert history/audit).
    let (app, _dir) = build_app().await;
    let (cookie, _csrf) = boot_admin(&app).await;

    let (st, _, body) = send(
        &app,
        "GET",
        "/api/v1/updates/diagnostics",
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v.get("journal").is_some(), "diagnostics must carry journal");
    assert!(
        v.get("network").is_some(),
        "diagnostics must carry network state"
    );
    assert!(v.get("slots").is_some());
    assert!(v.get("dmesg").is_some());
    assert!(v.get("alert_history").is_some());
    assert!(v.get("audit").is_some());
}

#[tokio::test]
async fn alerts_expose_default_packs_and_persist_push_targets() {
    // Spec §4.7: GET /alerts surfaces the per-platform default rule packs
    // (read-only) and the user-configured rules + push targets; PUT /alerts
    // persists a user rule, webhook targets, and the MQTT event topic.
    let (app, _dir) = build_app().await;
    let (cookie, csrf) = boot_admin(&app).await;

    // Defaults shipped in the manifest are visible.
    let (st, _, body) = send(&app, "GET", "/api/v1/alerts", Some(&cookie), None, None).await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let defaults = v["default_rules"]
        .as_array()
        .expect("default_rules present");
    assert!(
        defaults.iter().any(|r| r["id"] == "disk-full"),
        "default pack must include disk-full: {defaults:?}"
    );
    assert!(
        defaults.iter().any(|r| r["id"] == "watchdog-reset"),
        "default pack must include watchdog-reset: {defaults:?}"
    );

    // Persist a user rule with webhook + mqtt actions and the push targets.
    let payload = serde_json::json!({
        "enabled": true,
        "rules": [{
            "id": "hot", "enabled": true, "metric": "temp",
            "comparison": ">", "threshold": 85.0, "duration_secs": 60,
            "actions": ["ui", "webhook", "mqtt"]
        }],
        "webhooks": ["https://hooks.example.com/alerts"],
        "mqtt_event_topic": "soccentric/events"
    })
    .to_string();
    let (st, _, _) = send(
        &app,
        "PUT",
        "/api/v1/alerts",
        Some(&cookie),
        Some(&csrf),
        Some(&payload),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // Reload — user rule, webhooks, and mqtt topic are persisted.
    let (st, _, body) = send(&app, "GET", "/api/v1/alerts", Some(&cookie), None, None).await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["enabled"], true);
    assert_eq!(v["mqtt_event_topic"], "soccentric/events");
    assert_eq!(v["webhooks"][0], "https://hooks.example.com/alerts");
    let hot = v["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "hot")
        .expect("user rule persisted");
    assert_eq!(hot["metric"], "temp");
    assert_eq!(hot["threshold"], 85.0);
    let actions = hot["actions"].as_array().unwrap();
    assert!(actions.iter().any(|a| a == "webhook"));
    assert!(actions.iter().any(|a| a == "mqtt"));
}

#[tokio::test]
async fn factory_reset_wipes_certs_and_alert_history() {
    // spec §4.1: factory reset "wipes the data partition". The device TLS
    // identity (cert.pem/key.pem), the uploaded cert store (certs/), and the
    // SQLite alert_history table must all be cleared so a reset device
    // re-provisions with a fresh identity — not just config/audit/users.
    let (app, dir) = build_app().await;
    let webconfig_dir = dir.path().join("data/webconfig");
    std::fs::create_dir_all(&webconfig_dir).unwrap();
    std::fs::write(webconfig_dir.join("cert.pem"), "FAKE CERT PEM").unwrap();
    std::fs::write(webconfig_dir.join("key.pem"), "FAKE KEY PEM").unwrap();
    std::fs::create_dir_all(webconfig_dir.join("certs")).unwrap();
    std::fs::write(webconfig_dir.join("certs/ca.pem"), "FAKE CA PEM").unwrap();
    // Seed an alert_history row in config.db so we can assert it is cleared.
    {
        let conn = rusqlite::Connection::open(webconfig_dir.join("config.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS alert_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL, rule_id TEXT NOT NULL, state TEXT NOT NULL, value REAL NOT NULL
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO alert_history (ts, rule_id, state, value) VALUES ('t=1','hot','fired',90.0)",
            [],
        )
        .unwrap();
    }

    // Bootstrap an admin and grab session + csrf.
    let (_, ck, _) = send(
        &app,
        "POST",
        "/api/v1/auth/first-boot",
        None,
        None,
        Some(r#"{"username":"admin","password":"password1"}"#),
    )
    .await;
    let cookie = ck.unwrap();
    let (_, _, text) = send(
        &app,
        "GET",
        "/api/v1/auth/whoami",
        Some(&cookie),
        None,
        None,
    )
    .await;
    let csrf = serde_json::from_str::<serde_json::Value>(&text).unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_string();

    // Factory reset (settings-only mode still wipes certs + alert history).
    let (st, _, _) = send(
        &app,
        "POST",
        "/api/v1/system/factory-reset",
        Some(&cookie),
        Some(&csrf),
        Some(r#"{"confirm":"RESET","mode":"settings"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // TLS identity + cert store are gone (next boot regenerates the identity).
    assert!(!webconfig_dir.join("cert.pem").exists());
    assert!(!webconfig_dir.join("key.pem").exists());
    assert!(!webconfig_dir.join("certs").exists());
    // The alert history table is empty after the reset.
    let conn = rusqlite::Connection::open(webconfig_dir.join("config.db")).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM alert_history", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn cert_store_upload_list_delete_round_trip() {
    // spec §4.8: device cert store accepts uploaded CA/client certs, lists
    // their metadata, and removes them. PEM is validated server-side; the PEM
    // file lands in <webconfig_dir>/certs/<id>.pem and the metadata in
    // config.security.certificates.
    let (app, dir) = build_app().await;
    let webconfig_dir = dir.path().join("data/webconfig");

    // Bootstrap admin + csrf.
    let (_, ck, _) = send(
        &app,
        "POST",
        "/api/v1/auth/first-boot",
        None,
        None,
        Some(r#"{"username":"admin","password":"password1"}"#),
    )
    .await;
    let cookie = ck.unwrap();
    let (_, _, text) = send(
        &app,
        "GET",
        "/api/v1/auth/whoami",
        Some(&cookie),
        None,
        None,
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let csrf = v["csrf"].as_str().unwrap().to_string();

    // A real self-signed cert PEM generated with rcgen.
    let mut params = rcgen::CertificateParams::new(vec!["ca.example".to_string()]).unwrap();
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "ca.example");
    let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let pem = params.self_signed(&key_pair).unwrap().pem();

    let body = serde_json::json!({
        "id": "ca-root",
        "kind": "ca",
        "subject": "CN=ca.example",
        "not_after": "2026-08-23",
        "pem": pem,
    })
    .to_string();
    let (st, _, text) = send(
        &app,
        "POST",
        "/api/v1/security/certs",
        Some(&cookie),
        Some(&csrf),
        Some(&body),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let resp: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(resp["id"], "ca-root");
    assert_eq!(resp["kind"], "ca");

    // The PEM file is on the data partition.
    assert!(webconfig_dir.join("certs/ca-root.pem").exists());

    // GET /security/certs lists it.
    let (st, _, text) = send(
        &app,
        "GET",
        "/api/v1/security/certs",
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let list: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(list.iter().any(|c| c["id"] == "ca-root"));

    // A garbage PEM is rejected at save.
    let bad = serde_json::json!({
        "id": "bad", "kind": "ca", "subject": "x", "not_after": "2026-08-23", "pem": "not a cert"
    })
    .to_string();
    let (st, _, _) = send(
        &app,
        "POST",
        "/api/v1/security/certs",
        Some(&cookie),
        Some(&csrf),
        Some(&bad),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // DELETE removes file + metadata.
    let (st, _, _) = send(
        &app,
        "DELETE",
        "/api/v1/security/certs/ca-root",
        Some(&cookie),
        Some(&csrf),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(!webconfig_dir.join("certs/ca-root.pem").exists());

    // Deleting a missing id → NotFound.
    let (st, _, _) = send(
        &app,
        "DELETE",
        "/api/v1/security/certs/nope",
        Some(&cookie),
        Some(&csrf),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tls_identity_upload_replaces_cert_and_rejects_mismatch() {
    // Spec §4.8: "upload of customer cert/key" replaces the device TLS
    // identity. The pair is validated (a mismatched cert/key is rejected) and
    // only then written to cert.pem/key.pem — a bad upload can never brick the
    // listener on the next boot.
    let (app, dir) = build_app().await;
    let webconfig_dir = dir.path().join("data/webconfig");
    let (cookie, csrf) = boot_admin(&app).await;

    // A real self-signed identity generated client-side = the "customer" pair.
    let mut params = rcgen::CertificateParams::new(vec!["customer.example".to_string()]).unwrap();
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "customer.example");
    let kp = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let cert_pem = params.self_signed(&kp).unwrap().pem();
    let key_pem = kp.serialize_pem();

    let body = serde_json::json!({ "cert": cert_pem, "key": key_pem }).to_string();
    let (st, _, resp) = send(
        &app,
        "POST",
        "/api/v1/security/tls",
        Some(&cookie),
        Some(&csrf),
        Some(&body),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["restart_required"], true);
    // The customer identity replaced cert.pem/key.pem on the data partition.
    let on_disk = std::fs::read_to_string(webconfig_dir.join("cert.pem")).unwrap();
    assert_eq!(on_disk, cert_pem, "cert.pem must be the uploaded cert");
    assert_eq!(
        std::fs::read_to_string(webconfig_dir.join("key.pem")).unwrap(),
        key_pem
    );
    // The private key is mode 0600 — not world-readable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(webconfig_dir.join("key.pem"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "key.pem must not be group/world readable");
    }

    // A cert uploaded against a DIFFERENT key is rejected before disk changes.
    let other_kp = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let bad = serde_json::json!({ "cert": cert_pem, "key": other_kp.serialize_pem() }).to_string();
    let (st, _, _) = send(
        &app,
        "POST",
        "/api/v1/security/tls",
        Some(&cookie),
        Some(&csrf),
        Some(&bad),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // cert.pem is still the previously-accepted one (not overwritten on reject).
    assert_eq!(
        std::fs::read_to_string(webconfig_dir.join("cert.pem")).unwrap(),
        cert_pem
    );

    // Empty body → 400.
    let (st, _, _) = send(
        &app,
        "POST",
        "/api/v1/security/tls",
        Some(&cookie),
        Some(&csrf),
        Some(r#"{"cert":"","key":""}"#),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn serial_apply_writes_baud_unit_and_rejects_conflicts() {
    // Spec §4.9: serial ownership + baud is applied (not just persisted), and
    // duplicate-device conflicts are prevented at PUT time.
    let (app, dir) = build_app().await;
    let (cookie, csrf) = boot_admin(&app).await;

    // A valid serial port on a manifest-declared UART.
    let body = serde_json::json!({
        "serial": [{ "device": "/dev/ttyTHS0", "enabled": true, "baud": 115200, "mode": "rs232", "owner": "customer" }],
        "serial_enabled": true,
        "can": [], "can_enabled": true, "gpio": [], "gpio_enabled": true,
        "cameras": [], "cameras_enabled": true, "mcu": null, "board_leds": {},
    })
    .to_string();
    let (st, _, _) = send(
        &app,
        "PUT",
        "/api/v1/io",
        Some(&cookie),
        Some(&csrf),
        Some(&body),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // Apply serial -> the baud unit lands under the sysroot's /etc, not the
    // data partition (matches SerialApplier's `sysroot.join("/etc/...")`).
    let (st, _, _) = send(
        &app,
        "POST",
        "/api/v1/io/serial/apply",
        Some(&cookie),
        Some(&csrf),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(dir
        .path()
        .join("etc/systemd/system/webconfig-serial-ttyTHS0.service")
        .exists());

    // Duplicate device (same UART, two entries) is rejected at PUT (409), even
    // though the two owners differ — ownership is per-device (spec §4.9).
    let dup = serde_json::json!({
        "serial": [
            { "device": "/dev/ttyTHS0", "enabled": true, "baud": 115200, "mode": "rs232", "owner": "console" },
            { "device": "/dev/ttyTHS0", "enabled": true, "baud": 9600, "mode": "rs232", "owner": "customer" }
        ],
        "serial_enabled": true,
        "can": [], "can_enabled": true, "gpio": [], "gpio_enabled": true,
        "cameras": [], "cameras_enabled": true, "mcu": null, "board_leds": {},
    })
    .to_string();
    let (st, _, _) = send(
        &app,
        "PUT",
        "/api/v1/io",
        Some(&cookie),
        Some(&csrf),
        Some(&dup),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // A port not in the manifest is rejected at PUT (400).
    let bad = serde_json::json!({
        "serial": [{ "device": "/dev/ttyS9", "enabled": true, "baud": 9600, "mode": "rs232", "owner": "unassigned" }],
        "serial_enabled": true,
        "can": [], "can_enabled": true, "gpio": [], "gpio_enabled": true,
        "cameras": [], "cameras_enabled": true, "mcu": null, "board_leds": {},
    })
    .to_string();
    let (st, _, _) = send(
        &app,
        "PUT",
        "/api/v1/io",
        Some(&cookie),
        Some(&csrf),
        Some(&bad),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

/// A systemd client that fails every verb — used to prove failures propagate
/// as real HTTP errors and land in the audit trail as failures, not "ok".
#[derive(Debug, Default)]
struct FailingSystemd;

#[async_trait::async_trait]
impl crate::systemd::SystemdClient for FailingSystemd {
    async fn status(
        &self,
        unit: &str,
    ) -> Result<crate::systemd::UnitState, crate::systemd::SystemdError> {
        Err(crate::systemd::SystemdError::NotFound(unit.to_string()))
    }
    async fn start(&self, _unit: &str) -> Result<(), crate::systemd::SystemdError> {
        Err(crate::systemd::SystemdError::Dbus(
            "simulated failure".into(),
        ))
    }
    async fn stop(&self, _unit: &str) -> Result<(), crate::systemd::SystemdError> {
        Err(crate::systemd::SystemdError::Dbus(
            "simulated failure".into(),
        ))
    }
    async fn restart(&self, _unit: &str) -> Result<(), crate::systemd::SystemdError> {
        Err(crate::systemd::SystemdError::Dbus(
            "simulated failure".into(),
        ))
    }
    async fn enable(&self, _unit: &str) -> Result<(), crate::systemd::SystemdError> {
        Err(crate::systemd::SystemdError::Dbus(
            "simulated failure".into(),
        ))
    }
    async fn disable(&self, _unit: &str) -> Result<(), crate::systemd::SystemdError> {
        Err(crate::systemd::SystemdError::Dbus(
            "simulated failure".into(),
        ))
    }
    async fn journal_tail(
        &self,
        _unit: &str,
        _n: usize,
    ) -> Result<String, crate::systemd::SystemdError> {
        Ok(String::new())
    }
}

#[tokio::test]
async fn services_verbs_report_real_failure_not_fake_ok() {
    // Spec §4.1/§4.3 (P0 0.2): a systemd failure must surface as a non-200
    // error and the audit entry must show "failed", never "ok":true.
    let (app, _dir) =
        build_app_with_systemd(Schema::default_permissive(), Some(Arc::new(FailingSystemd))).await;
    let (cookie, csrf) = boot_admin(&app).await;

    for (method, path) in [
        ("POST", "/api/v1/services/sshd.service/start"),
        ("POST", "/api/v1/services/sshd.service/stop"),
        ("POST", "/api/v1/services/sshd.service/restart"),
    ] {
        let (st, _, _) = send(&app, method, path, Some(&cookie), Some(&csrf), None).await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{method} {path}");
    }

    let (st, _, _) = send(
        &app,
        "PUT",
        "/api/v1/services/sshd.service/enabled",
        Some(&cookie),
        Some(&csrf),
        Some(r#"{"enabled": true}"#),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    // The audit log reflects failure for every verb, never a fabricated "ok".
    let (st, _, text) = send(
        &app,
        "GET",
        "/api/v1/logs/audit",
        Some(&cookie),
        Some(&csrf),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let entries: serde_json::Value = serde_json::from_str(&text).unwrap();
    let entries = entries.as_array().expect("audit log is a JSON array");
    let failed_count = entries
        .iter()
        .filter(|e| {
            e["action"]
                .as_str()
                .map(|a| a.starts_with("services."))
                .unwrap_or(false)
        })
        .filter(|e| {
            e["detail"]
                .as_str()
                .map(|d| d.contains("failed"))
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        failed_count, 4,
        "expected all 4 services.* audit entries to record failure: {text}"
    );
}
