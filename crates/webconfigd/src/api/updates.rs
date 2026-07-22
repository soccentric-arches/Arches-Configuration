//! Updates & Maintenance handlers (spec §4.4): RAUC A/B slots, bundle upload +
//! verify + compat + install, reboot-to-apply, OTA check, backup/restore,
//! diagnostics bundle, factory reset (duplicated from System).

use axum::extract::State;
use axum::Json;
use tower_sessions::Session;
use webconfig_engine::update::InstallProgress;

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

/// `GET /api/v1/updates/ota` — read OTA server configuration.
pub async fn get_ota(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<webconfig_engine::store::updates::OtaConfig>> {
    Ok(Json(app.engine.load()?.updates.ota))
}

/// `PUT /api/v1/updates/ota` — save OTA server configuration (admin only).
pub async fn put_ota(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<webconfig_engine::store::updates::OtaConfig>,
) -> ApiResult<Json<webconfig_engine::store::updates::OtaConfig>> {
    role.require_admin("updates.ota.put")?;
    let user = username(&session).await;
    let cfg = app.engine.update(move |c| {
        c.updates.ota = req.clone();
        Ok(())
    })?;
    audit(
        &app,
        &user,
        "updates.ota.put",
        cfg.updates.ota.server_url.clone(),
    );
    Ok(Json(cfg.updates.ota))
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
    if !app.engine.sysroot.is_real_root() {
        return Ok(Json(serde_json::json!({
            "checked": true,
            "server_url": url,
            "update_available": null,
            "reason": "remote checks are disabled in the fake sysroot",
        })));
    }

    let output = webconfig_engine::applier::run_cmd(
        "curl",
        &[
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "10",
            &url,
        ],
    )
    .map_err(ApiError::from)?;
    let remote: serde_json::Value = serde_json::from_str(&output)
        .map_err(|e| ApiError::BadRequest(format!("OTA server returned invalid JSON: {e}")))?;
    let update_available = remote
        .get("update_available")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Ok(Json(serde_json::json!({
        "checked": true,
        "server_url": url,
        "update_available": update_available,
        "version": remote.get("version"),
        "platform": remote.get("platform"),
    })))
}

/// `GET /api/v1/updates/backup/config` — read remote backup configuration.
pub async fn get_backup_config(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<webconfig_engine::store::updates::BackupConfig>> {
    Ok(Json(app.engine.load()?.updates.backup))
}

/// `PUT /api/v1/updates/backup/config` — save remote backup configuration.
pub async fn put_backup_config(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<webconfig_engine::store::updates::BackupConfig>,
) -> ApiResult<Json<webconfig_engine::store::updates::BackupConfig>> {
    role.require_admin("updates.backup.put")?;
    let user = username(&session).await;
    let cfg = app.engine.update(move |c| {
        c.updates.backup = req.clone();
        Ok(())
    })?;
    audit(
        &app,
        &user,
        "updates.backup.put",
        cfg.updates.backup.server_url.clone(),
    );
    Ok(Json(cfg.updates.backup))
}

/// `GET /api/v1/updates/backup/check` — test the configured remote server.
pub async fn backup_check(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let cfg = app.engine.load()?.updates.backup;
    let url = cfg.server_url.clone().unwrap_or_default();
    if !cfg.enabled {
        return Ok(Json(serde_json::json!({
            "checked": false,
            "connected": false,
            "reason": "remote backup connection is disabled",
        })));
    }
    if url.is_empty() {
        return Ok(Json(serde_json::json!({
            "checked": false,
            "connected": false,
            "reason": "no backup server url",
        })));
    }
    if !app.engine.sysroot.is_real_root() {
        return Ok(Json(serde_json::json!({
            "checked": true,
            "connected": false,
            "server_url": url,
            "reason": "remote checks are disabled in the fake sysroot",
        })));
    }

    let output = webconfig_engine::applier::run_cmd(
        "curl",
        &[
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "10",
            &url,
        ],
    )
    .map_err(ApiError::from)?;
    let remote = serde_json::from_str::<serde_json::Value>(&output).ok();
    Ok(Json(serde_json::json!({
        "checked": true,
        "connected": true,
        "server_url": url,
        "remote_backup_enabled": remote.as_ref().and_then(|v| v.get("backup_enabled")),
        "remote_restore_enabled": remote.as_ref().and_then(|v| v.get("restore_enabled")),
    })))
}

/// `GET /api/v1/updates/backup` — download the config store as a file (spec
/// §4.4). **Secrets are always excluded** on this route: every secret field is
/// redacted. To obtain a backup that includes secrets, use
/// `POST /updates/backup` with a passphrase, which returns a
/// passphrase-encrypted envelope (secrets only ever leave the device encrypted,
/// spec §4.4).
pub async fn backup(
    State(app): State<AppState>,
    _role: AuthRole,
    axum::extract::Query(q): axum::extract::Query<BackupQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut cfg = app.engine.load()?;
    redact_secrets(&mut cfg);
    // `include_secrets` is accepted for backward compatibility with older
    // clients but is ignored on GET — secrets never travel in plaintext. The
    // response always reports `secrets_included: false`.
    let _ = q.include_secrets;
    Ok(Json(serde_json::json!({
        "schema_version": webconfig_engine::CONFIG_SCHEMA_VERSION,
        "config": cfg,
        "secrets_included": false,
    })))
}

#[derive(serde::Deserialize, Default)]
pub struct BackupQuery {
    #[serde(default)]
    pub include_secrets: bool,
}

/// `POST /api/v1/updates/backup` — download a **passphrase-encrypted** backup
/// that includes secrets (spec §4.4). The passphrase never leaves the request
/// body (not a query string, so it is not logged in access logs/journal). The
/// response is an encrypted envelope the frontend downloads verbatim; secrets
/// are only ever present inside the AEAD ciphertext.
pub async fn backup_encrypted(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<BackupSecretRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("updates.backup.secrets")?;
    if req.passphrase.len() < 8 {
        return Err(ApiError::BadRequest(
            "passphrase must be at least 8 characters".into(),
        ));
    }
    let cfg = app.engine.load()?;
    let payload = serde_json::json!({
        "schema_version": webconfig_engine::CONFIG_SCHEMA_VERSION,
        "config": cfg,
    });
    let payload_bytes =
        serde_json::to_vec(&payload).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let env = webconfig_engine::backup::encrypt(&payload_bytes, &req.passphrase)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    audit(
        &app,
        &username(&session).await,
        "updates.backup.secrets",
        None,
    );
    Ok(Json(serde_json::json!({
        "schema_version": webconfig_engine::CONFIG_SCHEMA_VERSION,
        "secrets_included": true,
        "encrypted": env,
    })))
}

#[derive(serde::Deserialize)]
pub struct BackupSecretRequest {
    pub passphrase: String,
}

/// `POST /api/v1/updates/restore` — restore validates schema version and
/// tolerates version skew (drop unknown fields gracefully, spec §4.4). Accepts
/// either a plaintext (redacted) `ConfigFile` or an encrypted backup envelope
/// produced by `POST /updates/backup`; in the encrypted case a `passphrase`
/// field is required to decrypt.
pub async fn restore(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("updates.restore")?;

    let file = if let Some(enc) = req.get("encrypted") {
        // Encrypted backup: decrypt with the supplied passphrase, then parse
        // the inner ConfigFile.
        let passphrase = req
            .get("passphrase")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ApiError::BadRequest("passphrase required to restore an encrypted backup".into())
            })?;
        let env: webconfig_engine::backup::EncryptedBackup =
            serde_json::from_value(enc.clone()).map_err(|e| ApiError::BadRequest(e.to_string()))?;
        let plaintext = webconfig_engine::backup::decrypt(&env, passphrase)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        serde_json::from_slice::<webconfig_engine::store::ConfigFile>(&plaintext)
            .map_err(|e| ApiError::BadRequest(format!("decrypted backup is not valid: {e}")))?
    } else {
        // Plaintext (redacted) backup: serde tolerates unknown fields by
        // default (no deny_unknown_fields), so version skew drops gracefully.
        serde_json::from_value::<webconfig_engine::store::ConfigFile>(req)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?
    };

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
        app.engine.sysroot.webconfig_dir().join("config.db"),
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

/// Strip every secret from a config tree before a plaintext backup/diagnostics
/// export (spec §4.4: secrets excluded by default — a secret-bearing backup
/// must go through the passphrase-encrypted path instead). The WireGuard
/// private key lives in a separate file (never in the config tree) and user
/// password hashes live in the separate users store (not exported here), so
/// neither needs redaction in this tree.
fn redact_secrets(cfg: &mut webconfig_engine::Config) {
    const REDACTED: &str = "[redacted]";

    // Wi-Fi PSKs.
    for w in &mut cfg.network.wifi {
        if w.psk.is_some() {
            w.psk = Some(REDACTED.into());
        }
    }
    // AP provisioning passphrase.
    if !cfg.network.ap_mode.passphrase.is_empty() {
        cfg.network.ap_mode.passphrase = REDACTED.into();
    }
    // 5G modem credentials.
    if cfg.network.cellular_5g.pin.is_some() {
        cfg.network.cellular_5g.pin = Some(REDACTED.into());
    }
    if cfg.network.cellular_5g.password.is_some() {
        cfg.network.cellular_5g.password = Some(REDACTED.into());
    }
    // MQTT broker password.
    if let Some(m) = &mut cfg.services.mqtt.password {
        *m = REDACTED.into();
    }
    // Telemetry publish password.
    if !cfg.monitoring.telemetry.password.is_empty() {
        cfg.monitoring.telemetry.password = REDACTED.into();
    }
}
