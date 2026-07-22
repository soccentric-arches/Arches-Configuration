//! Security handlers (spec §4.8): TLS/cert store, WireGuard, exposure summary,
//! trust status (read-only), session/audit config.

use axum::extract::State;
use axum::Json;
use tower_sessions::Session;
use webconfig_engine::store::security::SecurityConfig;

use crate::api::{apply_and_audit, audit, username};
use crate::auth::AuthRole;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /api/v1/security` — security section (spec §4.8).
pub async fn get(State(app): State<AppState>, _role: AuthRole) -> ApiResult<Json<SecurityConfig>> {
    Ok(Json(app.engine.load()?.security))
}

/// `PUT /api/v1/security` — idle timeout / plain-HTTP opt-in / WireGuard
/// config. Admin only. WireGuard apply happens on the next `POST /security/apply`.
pub async fn put(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<SecurityConfig>,
) -> ApiResult<Json<SecurityConfig>> {
    role.require_admin("security.put")?;
    let user = username(&session).await;
    let cfg = app.engine.update(move |c| {
        c.security = req.clone();
        Ok(())
    })?;
    audit(&app, &user, "security.put", None);
    Ok(Json(cfg.security))
}

/// `POST /api/v1/security/apply` — apply the WireGuard applier (spec §4.8).
pub async fn apply(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("security.apply")?;
    let user = username(&session).await;
    app.engine.validate("wireguard").await?;
    let _ = apply_and_audit(&app, &user, "wireguard").await?;
    Ok(Json(serde_json::json!({ "applied": ["wireguard"] })))
}

/// `GET /api/v1/security/wireguard/pubkey` — public key only; the private key
/// never leaves the device (spec §4.8).
pub async fn wg_pubkey(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let key_path = app.engine.sysroot.webconfig_dir().join("wireguard.key");
    let has = key_path.exists();
    // Derive the public key from the on-disk private key with `wg pubkey`.
    // If no key exists yet, return empty so the UI can prompt to enable WG.
    let pubkey = if has && app.engine.sysroot.is_real_root() {
        let privkey = std::fs::read_to_string(&key_path).unwrap_or_default();
        use std::io::Write;
        let mut child = std::process::Command::new("wg")
            .arg("pubkey")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|_| ApiError::Internal)?;
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(privkey.as_bytes());
        }
        let out = child.wait_with_output().map_err(|_| ApiError::Internal)?;
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        String::new()
    };
    Ok(Json(
        serde_json::json!({ "public_key": pubkey, "has_private_key": has }),
    ))
}

/// `POST /api/v1/security/csr` — generate a CSR (spec §4.8). Returns the PEM
/// CSR + the private key so the admin can store it in the cert store.
pub async fn generate_csr(
    State(app): State<AppState>,
    role: AuthRole,
    Json(req): Json<CsrRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("security.csr")?;
    let (csr, key) =
        crate::tls::generate_csr(&req.cn).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    audit(&app, &"", "security.csr", Some(req.cn.clone()));
    Ok(Json(serde_json::json!({ "csr": csr, "private_key": key })))
}

#[derive(serde::Deserialize)]
pub struct CsrRequest {
    pub cn: String,
}

/// `GET /api/v1/security/exposure` — one glanceable statement of currently
/// reachable services per interface (spec §4.8), derived from firewall +
/// service state.
pub async fn exposure(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let cfg = app.engine.load()?;
    let mut per_iface = Vec::new();
    for iface in &cfg.network.interfaces {
        if !iface.enabled {
            continue;
        }
        per_iface.push(serde_json::json!({
            "interface": iface.name,
            "open_services": iface.open_services,
        }));
    }
    Ok(Json(serde_json::json!({
        "ui_always_reachable": true,
        "plain_http": cfg.security.allow_plain_http,
        "per_interface": per_iface,
    })))
}

/// `GET /api/v1/security/trust` — read-only trust status from the BSP
/// platform-status file / manifest (spec §4.8). Never configurable from the UI.
pub async fn trust(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let m = &app.manifest;
    Ok(Json(serde_json::json!({
        "secure_boot": m.secure_boot,
        "tpm_present": m.has_tpm,
        // Signing key id + verity/encryption status would be read from the
        // platform-status file; surfaced as unknown when absent (spec §4.8).
        "update_signing_key_id": null,
        "verity_status": "unknown",
        "encryption_status": "unknown",
    })))
}

/// `GET /api/v1/security/certs` — device cert store metadata (spec §4.8).
pub async fn certs(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<webconfig_engine::store::security::CertMeta>>> {
    Ok(Json(app.engine.load()?.security.certificates))
}
