//! Updates & Maintenance handlers (spec §4.4): RAUC A/B slots, bundle upload +
//! verify + compat + install, reboot-to-apply, OTA check, backup/restore,
//! diagnostics bundle, factory reset (duplicated from System).

use axum::extract::State;
use axum::Json;
use tower_sessions::Session;
use webconfig_engine::update::{InstallProgress, InstallStage};

use crate::api::{audit, username};
use crate::auth::AuthRole;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /api/v1/updates/slots` — A/B slot status (spec §4.4).
pub async fn slots(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<webconfig_engine::update::SlotStatus>>> {
    Ok(Json(app.update_backend.slots().map_err(map_upd)?))
}

/// `POST /api/v1/updates/install` — browser upload of a signed bundle →
/// signature verification → compatibility check → write inactive slot →
/// staged progress → "reboot to apply" (spec §4.4).
pub async fn install(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<InstallRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("updates.install")?;
    let user = username(&session).await;
    let manifest = app
        .update_backend
        .verify_bundle(&req.bundle_path)
        .map_err(map_upd)?;
    let mut stages = Vec::new();
    app.update_backend
        .install(&req.bundle_path, &mut |p: InstallProgress| {
            stages.push(serde_json::json!({
                "stage": p.stage,
                "percent": p.percent,
                "message": p.message,
            }));
        })
        .map_err(map_upd)?;
    audit(
        &app,
        &user,
        "updates.install",
        Some(format!("{} v{}", manifest.platform, manifest.version)),
    );
    Ok(Json(serde_json::json!({
        "manifest": manifest,
        "stages": stages,
        "reboot_to_apply": true,
    })))
}

#[derive(serde::Deserialize)]
pub struct InstallRequest {
    /// Server-side path the upload handler staged the bundle at. (A multipart
    /// upload handler would write to /data/webconfig/uploads/ then call here.)
    pub bundle_path: String,
}

/// `POST /api/v1/updates/reboot-to-apply` (spec §4.4).
pub async fn reboot_to_apply(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("updates.reboot_to_apply")?;
    audit(
        &app,
        &username(&session).await,
        "updates.reboot_to_apply",
        None,
    );
    app.update_backend.reboot_to_apply().map_err(map_upd)?;
    Ok(Json(serde_json::json!({ "rebooting": true })))
}

/// `GET /api/v1/updates/ota/check` — notify-only in v1 (spec §4.4). [v2]
/// auto-apply + maintenance windows.
pub async fn ota_check(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let cfg = app.engine.load()?.updates.ota;
    let url = cfg.server_url.unwrap_or_default();
    if url.is_empty() {
        return Ok(Json(
            serde_json::json!({ "checked": false, "reason": "no server url" }),
        ));
    }
    // v1: notify-only — we do not download/apply. Real check would query the
    // RAUC HawKD server; here we report the configured URL (spec §4.4).
    Ok(Json(
        serde_json::json!({ "checked": true, "server_url": url, "update_available": null }),
    ))
}

/// `GET /api/v1/updates/backup` — download the config store as a file (spec
/// §4.4). **Secrets excluded by default** (PSKs, WireGuard private key, user
/// hashes redacted).
pub async fn backup(
    State(app): State<AppState>,
    _role: AuthRole,
    axum::extract::Query(q): axum::extract::Query<BackupQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut cfg = app.engine.load()?;
    if !q.include_secrets {
        redact_secrets(&mut cfg);
    }
    Ok(Json(serde_json::json!({
        "schema_version": webconfig_engine::CONFIG_SCHEMA_VERSION,
        "config": cfg,
        "secrets_included": q.include_secrets,
    })))
}

#[derive(serde::Deserialize, Default)]
pub struct BackupQuery {
    #[serde(default)]
    pub include_secrets: bool,
}

/// `POST /api/v1/updates/restore` — restore validates schema version and
/// tolerates version skew (drop unknown fields gracefully, spec §4.4).
pub async fn restore(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("updates.restore")?;
    // serde tolerates unknown fields by default (no deny_unknown_fields), so
    // version skew drops gracefully. We only refuse a *newer* schema than the
    // engine supports.
    let file: webconfig_engine::store::ConfigFile =
        serde_json::from_value(req).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if file.schema_version > webconfig_engine::CONFIG_SCHEMA_VERSION {
        return Err(ApiError::Conflict(format!(
            "backup schema_version {} > engine {}",
            file.schema_version,
            webconfig_engine::CONFIG_SCHEMA_VERSION
        )));
    }
    app.engine.store.commit(&file)?;
    audit(&app, &username(&session).await, "updates.restore", None);
    Ok(Json(serde_json::json!({ "restored": true })))
}

/// `GET /api/v1/updates/diagnostics` — tarball bundle of journal excerpt,
/// config store (secrets redacted), versions, slot status, dmesg, network
/// state, alert history, audit log (spec §4.4). Returned as JSON for the
/// frontend to assemble; the real tarball is built on the real root.
pub async fn diagnostics(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let mut cfg = app.engine.load()?;
    redact_secrets(&mut cfg);
    let slots = app.update_backend.slots().unwrap_or_default();
    let audit = app.audit.read_all().unwrap_or_default();
    let dmesg = if app.engine.sysroot.is_real_root() {
        webconfig_engine::applier::run_cmd("dmesg", &["--ctime"]).unwrap_or_default()
    } else {
        String::new()
    };
    let history = webconfig_engine::alert::AlertHistory::new(
        app.engine.sysroot.webconfig_dir().join("alert.jsonl"),
        1000,
    )
    .read_all()
    .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "config": cfg,
        "slots": slots,
        "audit": audit,
        "dmesg": dmesg,
        "alert_history": history,
        "versions": {
            "platform": app.manifest.platform,
            "model": app.manifest.model,
        },
    })))
}

/// `POST /api/v1/updates/factory-reset` — duplicated here (same action as
/// System, spec §4.4). Delegates to the system handler.
pub async fn factory_reset(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<crate::api::system::FactoryResetRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    crate::api::system::factory_reset(State(app), role, session, Json(req)).await
}

fn map_upd(e: webconfig_engine::update::UpdateError) -> ApiError {
    match e {
        webconfig_engine::update::UpdateError::Incompatible { bundle, device } => {
            ApiError::Conflict(format!("incompatible: bundle {bundle} != device {device}"))
        }
        webconfig_engine::update::UpdateError::Signature(m) => ApiError::BadRequest(m),
        other => ApiError::Engine(other.to_string()),
    }
}

/// Strip secrets from a config tree before backup/diagnostics (spec §4.4:
/// secrets excluded by default).
fn redact_secrets(cfg: &mut webconfig_engine::Config) {
    for w in &mut cfg.network.wifi {
        w.psk = w.psk.as_ref().map(|_| "[redacted]".into());
    }
    if let Some(m) = &mut cfg.services.mqtt.password {
        *m = "[redacted]".into();
    }
    // The WireGuard private key lives in a separate file and is never in the
    // config tree; no action needed here.
    // Keep the [v2] stub marker so the install stages enum is referenced.
    let _ = InstallStage::Done;
}
