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
    let dir = tempfile::tempdir().unwrap();
    let sysroot = Sysroot::at(dir.path());
    let webconfig_dir = sysroot.webconfig_dir();
    std::fs::create_dir_all(&webconfig_dir).ok();

    let schema = Schema::default_permissive();
    let store = ConfigStore::new(sysroot.clone(), schema);
    let engine = EngineBridge::new(store, sysroot.clone());

    // Minimal manifest with a service allowlist + one peripheral so the I/O and
    // services pages have data.
    let manifest = BoardManifest {
        platform: "arches".into(),
        peripherals: vec![webconfig_engine::manifest::Peripheral {
            id: "can0".into(),
            device: "can0".into(),
            human_name: "CAN 0".into(),
            kind: IoKind::Can,
            ..Default::default()
        }],
        service_allowlist: vec!["sshd.service".into()],
        ..Default::default()
    };

    let state = AppState {
        engine: engine.clone(),
        schema: Arc::new(Schema::default_permissive()),
        manifest: Arc::new(manifest),
        audit: Arc::new(AuditLog::new(webconfig_dir.join("audit.jsonl"), 64 * 1024)),
        alert_engine: Arc::new(tokio::sync::Mutex::new(AlertEngine::new())),
        update_backend: crate::update_backend::make_backend(&sysroot),
        systemd: crate::systemd::make_client(&sysroot).await,
        users_path: webconfig_dir.join("users.json"),
        login_throttle: Arc::new(crate::auth::LoginThrottle::new()),
    };

    // Static dir: a tempdir is fine; API routes don't depend on it.
    let static_dir = tempfile::tempdir().unwrap().into_path();
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
        .map(|s| session_cookie(s));
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
