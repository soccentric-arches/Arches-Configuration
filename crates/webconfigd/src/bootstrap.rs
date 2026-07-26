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
    let manifest = Arc::new(load_or_seed_manifest(&sysroot, &share));
    let store = ConfigStore::with_defaults(
        sysroot.clone(),
        (*schema).clone(),
        manifest.config_defaults.clone().unwrap_or_default(),
    );
    let engine = EngineBridge::new(store, sysroot.clone());
    let database = webconfig_dir.join("config.db");
    let audit = Arc::new(AuditLog::new(database.clone(), 256 * 1024));
    let alert_engine = Arc::new(tokio::sync::Mutex::new(AlertEngine::new()));

    let state = AppState {
        engine: engine.clone(),
        schema,
        manifest: manifest.clone(),
        audit,
        alert_engine: alert_engine.clone(),
        update_backend: crate::update_backend::make_backend(&sysroot),
        systemd: crate::systemd::make_client(&sysroot).await,
        users_path: database,
        login_throttle: Arc::new(LoginThrottle::new()),
    };

    Application {
        state,
        engine,
        manifest,
        alert_engine,
    }
}

fn load_or_seed_manifest(sysroot: &Sysroot, share: &Path) -> BoardManifest {
    let local_yaml = sysroot.join("/etc/webconfig/board.yaml");
    if let Ok(manifest) = BoardManifest::load_yaml_path(&local_yaml) {
        return manifest;
    }
    if let Ok(manifest) = BoardManifest::load_yaml(share) {
        return manifest;
    }
    if let Ok(manifest) = BoardManifest::load(share) {
        return manifest;
    }

    let platform = std::env::var("WEBCONFIG_PLATFORM").unwrap_or_else(|_| "arches".into());
    let raw = match platform.as_str() {
        "generic" | "generic-linux" => include_str!("../../../board-manifests/generic.yaml"),
        _ => include_str!("../../../board-manifests/arches.json"),
    };

    if platform == "generic" || platform == "generic-linux" {
        serde_yaml::from_str(raw).unwrap_or_default()
    } else {
        serde_json::from_str(raw).unwrap_or_default()
    }
}

pub(crate) fn alert_history(engine: &EngineBridge) -> AlertHistory {
    AlertHistory::new(engine.sysroot.webconfig_dir().join("config.db"), 1000)
}
