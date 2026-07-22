//! Application dependency assembly.
//!
//! Keeping construction policy here leaves `main` responsible for process
//! lifecycle and listener setup, while feature modules receive a fully wired
//! `AppState`.

use std::path::Path;
use std::sync::Arc;

use webconfig_engine::alert::{AlertEngine, AlertHistory};
use webconfig_engine::audit::AuditLog;
use webconfig_engine::manifest::BoardManifest;
use webconfig_engine::schema::Schema;
use webconfig_engine::{ConfigStore, Sysroot};

use crate::auth::LoginThrottle;
use crate::engine_bridge::EngineBridge;
use crate::state::AppState;

/// Dependencies needed by the daemon after startup.
pub(crate) struct Application {
    pub state: AppState,
    pub engine: EngineBridge,
    pub manifest: Arc<BoardManifest>,
    pub alert_engine: Arc<tokio::sync::Mutex<AlertEngine>>,
}

/// Build the application state and its long-running feature dependencies.
pub(crate) async fn build(sysroot: Sysroot) -> Application {
    let share = sysroot.share_dir();
    let webconfig_dir = sysroot.webconfig_dir();
    std::fs::create_dir_all(&webconfig_dir).ok();

    let schema = Arc::new(
        Schema::load_dir(&share.join("schemas")).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "schema load failed — permissive");
            Schema::default_permissive()
        }),
    );
    let manifest = Arc::new(load_or_seed_manifest(&share));
    let store = ConfigStore::new(sysroot.clone(), (*schema).clone());
    let engine = EngineBridge::new(store, sysroot.clone());
    let audit = Arc::new(AuditLog::new(webconfig_dir.join("audit.jsonl"), 256 * 1024));
    let alert_engine = Arc::new(tokio::sync::Mutex::new(AlertEngine::new()));

    let state = AppState {
        engine: engine.clone(),
        schema,
        manifest: manifest.clone(),
        audit,
        alert_engine: alert_engine.clone(),
        update_backend: crate::update_backend::make_backend(&sysroot),
        systemd: crate::systemd::make_client(&sysroot).await,
        users_path: webconfig_dir.join("users.json"),
        login_throttle: Arc::new(LoginThrottle::new()),
    };

    Application {
        state,
        engine,
        manifest,
        alert_engine,
    }
}

fn load_or_seed_manifest(share: &Path) -> BoardManifest {
    if let Ok(manifest) = BoardManifest::load(share) {
        return manifest;
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

    if let Ok(parent) = share.canonicalize() {
        let _ = std::fs::create_dir_all(&parent);
        let _ = std::fs::write(parent.join("board-manifest.json"), raw);
    }
    serde_json::from_str(raw).unwrap_or_default()
}

pub(crate) fn alert_history(engine: &EngineBridge) -> AlertHistory {
    AlertHistory::new(engine.sysroot.webconfig_dir().join("alert.jsonl"), 1000)
}
