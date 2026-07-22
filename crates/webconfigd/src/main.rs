//! webconfigd — on-device WebConfig daemon (spec §2: exactly one daemon, tokio
//! `current_thread` runtime, non-root, TLS, rustls, no OpenSSL).
//!
//! Layers (spec §3): HTTP (here) → config engine (webconfig-engine) → platform
//! (board manifest). The async HTTP layer talks to the synchronous engine over
//! the [`engine_bridge`] and runs appliers on blocking tasks.

#![forbid(unsafe_code)]

mod api;
mod auth;
mod csrf;
mod engine_bridge;
mod error;
mod routes;
mod sse;
mod state;
mod systemd;
mod tls;
mod update_backend;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt as _;
use tracing_subscriber::EnvFilter;
use webconfig_engine::alert::{AlertEngine, AlertHistory};
use webconfig_engine::audit::AuditLog;
use webconfig_engine::manifest::BoardManifest;
use webconfig_engine::schema::Schema;
use webconfig_engine::{ConfigStore, Sysroot};

use crate::state::AppState;

/// One daemon, current-thread runtime (spec §2).
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .compact()
        .init();

    let sysroot = resolve_sysroot();
    let share = sysroot.share_dir();
    let webconfig_dir = sysroot.webconfig_dir();
    std::fs::create_dir_all(&webconfig_dir).ok();

    // Schema engine: load from /usr/share/webconfig/schemas, else permissive.
    let schema = Arc::new(
        Schema::load_dir(&share.join("schemas")).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "schema load failed — permissive");
            Schema::default_permissive()
        }),
    );

    // Board manifest: load from share dir, else seed from the embedded example
    // for the configured platform so dev/CI has data (spec §3 platform layer).
    let manifest = Arc::new(load_or_seed_manifest(&share));

    let store = ConfigStore::new(sysroot.clone(), (*schema).clone());
    let engine = engine_bridge::EngineBridge::new(store, sysroot.clone());

    let audit = Arc::new(AuditLog::new(webconfig_dir.join("audit.jsonl"), 256 * 1024));
    let alert_engine = Arc::new(tokio::sync::Mutex::new(AlertEngine::new()));
    let update_backend = update_backend::make_backend(&sysroot);
    let systemd = systemd::make_client(&sysroot).await;
    let users_path = webconfig_dir.join("users.json");
    let login_throttle = Arc::new(auth::LoginThrottle::new());

    let state = AppState {
        engine: engine.clone(),
        schema: schema.clone(),
        manifest: manifest.clone(),
        audit: audit.clone(),
        alert_engine: alert_engine.clone(),
        update_backend,
        systemd,
        users_path,
        login_throttle,
    };

    let static_dir = resolve_static_dir();
    let app = routes::build_router(state, static_dir);

    // Commit/confirm rollback timer (spec §4.2 flagship feature).
    let bridge = Arc::new(engine.clone());
    tokio::spawn(bridge.run_confirm_timer());

    // Alert evaluation timer (spec §4.7): rule evaluation on the same data the
    // dashboard reads, on a timer inside the daemon — no extra process.
    tokio::spawn(alert_timer(
        engine.clone(),
        manifest.clone(),
        alert_engine.clone(),
    ));

    // TLS: ensure a per-device self-signed cert on first boot (spec §4.8).
    let cert = webconfig_dir.join("cert.pem");
    let key = webconfig_dir.join("key.pem");
    let hostname = std::env::var("WEBCONFIG_HOSTNAME").ok();
    tls::ensure_self_signed(&cert, &key, hostname.as_deref())
        .map_err(|e| anyhow::anyhow!("tls init: {e}"))?;
    let acceptor = tls::acceptor(&cert, &key).map_err(|e| anyhow::anyhow!("tls acceptor: {e}"))?;
    let acceptor = TlsAcceptor::from(acceptor);

    let port: u16 = std::env::var("WEBCONFIG_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8443);
    let addr = format!("0.0.0.0:{port}");
    tracing::info!(
        "webconfigd listening on https://{addr} (platform={})",
        manifest.platform
    );

    let listener = TcpListener::bind(&addr).await?;
    loop {
        let (stream, _peer) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let app = app.clone();
        tokio::spawn(async move {
            let Ok(tls) = acceptor.accept(stream).await else {
                return;
            };
            // `axum::serve` only accepts a bare `TcpListener`; since the TLS
            // handshake already happened above, we drive this one connection
            // through hyper directly (same glue axum's own serve() uses).
            let io = TokioIo::new(tls);
            let tower_service =
                app.clone()
                    .map_request(|req: http::Request<hyper::body::Incoming>| {
                        req.map(axum::body::Body::new)
                    });
            let hyper_service = TowerToHyperService::new(tower_service);
            let _ = ConnBuilder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(io, hyper_service)
                .await;
        });
    }
}

/// Resolve the sysroot from `WEBCONFIG_SYSROOT` (dev/CI fake sysroot) or `/`.
fn resolve_sysroot() -> Sysroot {
    match std::env::var("WEBCONFIG_SYSROOT") {
        Ok(dir) => Sysroot::at(dir),
        Err(_) => Sysroot::new(),
    }
}

/// Resolve the frontend static dir from `WEBCONFIG_STATIC_DIR`, else the
/// install default `/usr/share/webconfig/static`, else `frontend/` (dev).
fn resolve_static_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("WEBCONFIG_STATIC_DIR") {
        return std::path::PathBuf::from(dir);
    }
    let prod = std::path::PathBuf::from("/usr/share/webconfig/static");
    if prod.exists() {
        return prod;
    }
    std::path::PathBuf::from("frontend")
}

/// Load the board manifest from the share dir, or seed it from the embedded
/// example for the configured platform so dev/CI has data (spec §3).
fn load_or_seed_manifest(share: &std::path::Path) -> BoardManifest {
    if let Ok(m) = BoardManifest::load(share) {
        return m;
    }
    let platform = std::env::var("WEBCONFIG_PLATFORM").unwrap_or_else(|_| "arches".into());
    let raw = match platform.as_str() {
        "acadia" => include_str!("../../../board-manifests/acadia.json"),
        "zion" => include_str!("../../../board-manifests/zion.json"),
        "pinnacle" => include_str!("../../../board-manifests/pinnacle.json"),
        "joshua" => include_str!("../../../board-manifests/joshua.json"),
        "sequoia" => include_str!("../../../board-manifests/sequoia.json"),
        _ => include_str!("../../../board-manifests/arches.json"),
    };
    // Seed the file so the platform layer is consistent on subsequent boots.
    if let Ok(parent) = share.canonicalize() {
        let _ = std::fs::create_dir_all(&parent);
        let _ = std::fs::write(parent.join("board-manifest.json"), raw);
    }
    serde_json::from_str(raw).unwrap_or_default()
}

/// Periodic alert evaluation (spec §4.7). Runs every 5 s, evaluates rules
/// against the current vitals, records fired/cleared events to the ring
/// buffer, and (real root) would POST webhooks / publish MQTT events.
async fn alert_timer(
    engine: engine_bridge::EngineBridge,
    manifest: Arc<BoardManifest>,
    alert_engine: Arc<tokio::sync::Mutex<AlertEngine>>,
) {
    let history = AlertHistory::new(engine.sysroot.webconfig_dir().join("alert.jsonl"), 1000);
    let mut tick: u64 = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        tick += 5;
        let cfg = match engine.load() {
            Ok(c) => c,
            Err(_) => continue,
        };
        let vitals = webconfig_engine::monitoring::read_vitals(&engine.sysroot);
        let events = alert_engine
            .lock()
            .await
            .evaluate(&cfg.alerts.rules, &vitals, tick);
        for ev in events {
            tracing::info!(rule = %ev.rule_id, state = ?ev.state, "alert");
            if let Err(e) = history.record(&ev) {
                tracing::warn!(error = %e, "alert history write failed");
            }
            // v1 actions: UI red-state (the firing state drives it) + event log
            // (above). Webhook/MQTT emit happens here on the real root.
        }
        let _ = manifest; // platform default rule packs would be merged here
    }
}
