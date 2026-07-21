//! System section handlers (spec §4.1): hostname + asset tag, timezone/NTP/
//! manual time, web-UI user management, sshd/serial-getty toggles + authorized
//! keys, factory reset, reboot/shutdown, read-only device info, root password.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use webconfig_engine::store::system::SystemConfig;
use webconfig_engine::users::Role;

use crate::api::{apply_and_audit, audit, username};
use crate::auth::AuthRole;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /api/v1/system` — the system section (spec §4.1).
pub async fn get(State(app): State<AppState>) -> ApiResult<Json<SystemConfig>> {
    Ok(Json(app.engine.load()?.system))
}

/// `PUT /api/v1/system` — update hostname/time/ssh/ntp. Admin only (spec §4.1).
pub async fn put(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<SystemConfig>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("system.put")?;
    let user = username(&session).await;
    let cfg = app
        .engine
        .update(|c| {
            c.system = req.clone();
            Ok(())
        })?;
    audit(&app, &user, "system.put", None);
    Ok(Json(serde_json::to_value(&cfg.system).unwrap_or_default()))
}

/// `POST /api/v1/system/apply` — apply hostname + time appliers (spec §4.1).
pub async fn apply(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("system.apply")?;
    let user = username(&session).await;
    app.engine.validate("hostname").await?;
    app.engine.validate("time").await?;
    let _ = apply_and_audit(&app, &user, "hostname").await?;
    let _ = apply_and_audit(&app, &user, "time").await?;
    Ok(Json(serde_json::json!({ "applied": ["hostname","time"] })))
}

/// `GET /api/v1/system/info` — read-only device info (spec §4.1).
pub async fn info(
    State(app): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let m = &app.manifest;
    let os_release = std::fs::read_to_string(app.engine.sysroot.join("/etc/os-release"))
        .unwrap_or_default();
    let mut kv = std::collections::BTreeMap::new();
    for line in os_release.lines() {
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k.to_string(), v.trim_matches('"').to_string());
        }
    }
    Ok(Json(serde_json::json!({
        "platform": m.platform,
        "model": m.model,
        "board_name": m.board_name,
        "has_rtc": m.has_rtc,
        "os_release": kv,
        "uptime_secs": webconfig_engine::monitoring::read_vitals(&app.engine.sysroot).uptime_secs,
    })))
}

/// `POST /api/v1/system/reboot` / `/shutdown` (spec §4.1). The frontend shows a
/// "rebooting, reconnecting…" page that polls until the device returns.
pub async fn reboot(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("system.reboot")?;
    let user = username(&session).await;
    audit(&app, &user, "system.reboot", None);
    if app.engine.sysroot.is_real_root() {
        let _ = webconfig_engine::applier::run_cmd("systemctl", &["reboot"]);
    }
    Ok(Json(serde_json::json!({ "rebooting": true })))
}

pub async fn shutdown(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("system.shutdown")?;
    let user = username(&session).await;
    audit(&app, &user, "system.shutdown", None);
    if app.engine.sysroot.is_real_root() {
        let _ = webconfig_engine::applier::run_cmd("systemctl", &["poweroff"]);
    }
    Ok(Json(serde_json::json!({ "shutting_down": true })))
}

/// `POST /api/v1/system/factory-reset` — wipe data partition and reboot (spec
/// §4.1). Requires typed confirmation ("RESET"); "settings only" vs
/// "settings + user data" via the `mode` field.
#[derive(Deserialize)]
pub struct FactoryResetRequest {
    pub confirm: String,
    pub mode: String, // "settings" | "settings+users"
}

pub async fn factory_reset(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<FactoryResetRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("system.factory-reset")?;
    if req.confirm != "RESET" {
        return Err(ApiError::BadRequest("type RESET to confirm".into()));
    }
    if req.mode != "settings" && req.mode != "settings+users" {
        return Err(ApiError::BadRequest("mode must be settings|settings+users".into()));
    }
    let user = username(&session).await;
    audit(&app, &user, "system.factory-reset", Some(req.mode.clone()));
    let dir = app.engine.sysroot.webconfig_dir();
    if dir.exists() {
        // Settings only: remove config + audit + alerts but keep users.
        let _ = std::fs::remove_file(dir.join("config.json"));
        let _ = std::fs::remove_file(dir.join("config.json.bak"));
        let _ = std::fs::remove_file(dir.join("audit.jsonl"));
        let _ = std::fs::remove_file(dir.join("alert.jsonl"));
        if req.mode == "settings+users" {
            let _ = std::fs::remove_file(app.users_path.clone());
        }
    }
    if app.engine.sysroot.is_real_root() {
        let _ = webconfig_engine::applier::run_cmd("systemctl", &["reboot"]);
    }
    Ok(Json(serde_json::json!({ "factory_reset": true, "mode": req.mode })))
}

// --- User management (spec §4.1) ------------------------------------------

/// `GET /api/v1/system/users` — list users (no hashes).
pub async fn list_users(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<UserPublic>>> {
    let store = app.load_users().map_err(|_| ApiError::Internal)?;
    Ok(Json(
        store
            .users
            .iter()
            .map(|u| UserPublic {
                username: u.username.clone(),
                role: u.role.as_str().to_string(),
            })
            .collect(),
    ))
}

#[derive(Serialize)]
pub struct UserPublic {
    pub username: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

/// `POST /api/v1/system/users` — create a user (admin only).
pub async fn create_user(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<CreateUserRequest>,
) -> ApiResult<Json<UserPublic>> {
    role.require_admin("users.create")?;
    let parsed = Role::parse(&req.role).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let mut store = app.load_users().map_err(|_| ApiError::Internal)?;
    store
        .add(&req.username, &req.password, parsed)
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    app.save_users(&store).map_err(|_| ApiError::Internal)?;
    audit(&app, &username(&session).await, "users.create", Some(req.username.clone()));
    Ok(Json(UserPublic {
        username: req.username,
        role: parsed.as_str().to_string(),
    }))
}

/// `DELETE /api/v1/system/users/:name` (admin only; last-admin guarded).
pub async fn delete_user(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("users.delete")?;
    let mut store = app.load_users().map_err(|_| ApiError::Internal)?;
    store
        .remove(&name)
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    app.save_users(&store).map_err(|_| ApiError::Internal)?;
    audit(&app, &username(&session).await, "users.delete", Some(name.clone()));
    Ok(Json(serde_json::json!({ "deleted": name })))
}

#[derive(Deserialize)]
pub struct SetRoleRequest {
    pub role: String,
}

/// `PUT /api/v1/system/users/:name/role` (admin only).
pub async fn set_role(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(name): Path<String>,
    Json(req): Json<SetRoleRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("users.set_role")?;
    let parsed = Role::parse(&req.role).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let mut store = app.load_users().map_err(|_| ApiError::Internal)?;
    store
        .set_role(&name, parsed)
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    app.save_users(&store).map_err(|_| ApiError::Internal)?;
    audit(&app, &username(&session).await, "users.set_role", Some(name.clone()));
    Ok(Json(serde_json::json!({ "user": name, "role": req.role })))
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub password: String,
}

/// `PUT /api/v1/system/users/:name/password` (admin only).
pub async fn change_password(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(name): Path<String>,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("users.change_password")?;
    if req.password.len() < 8 {
        return Err(ApiError::BadRequest("password >= 8 chars".into()));
    }
    let mut store = app.load_users().map_err(|_| ApiError::Internal)?;
    store.set_password(&name, &req.password)?;
    app.save_users(&store).map_err(|_| ApiError::Internal)?;
    audit(&app, &username(&session).await, "users.change_password", Some(name.clone()));
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /api/v1/system/root-password` — change Linux root password via
/// `chpasswd` (spec §4.1). Separate single action, admin only.
#[derive(Deserialize)]
pub struct RootPasswordRequest {
    pub password: String,
}

pub async fn root_password(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<RootPasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("system.root_password")?;
    if req.password.len() < 8 {
        return Err(ApiError::BadRequest("password >= 8 chars".into()));
    }
    audit(&app, &username(&session).await, "system.root_password", None);
    if app.engine.sysroot.is_real_root() {
        use std::io::Write;
        let mut child = std::process::Command::new("chpasswd")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|_| ApiError::Internal)?;
        if let Some(stdin) = child.stdin.as_mut() {
            writeln!(stdin, "root:{}", req.password).ok();
        }
        let _ = child.wait();
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}