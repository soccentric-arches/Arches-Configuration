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
    let cfg = app.engine.update(move |c| {
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
pub async fn info(State(app): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let m = &app.manifest;
    let configured = app.engine.load()?.system;
    let sysroot = &app.engine.sysroot;
    let os_release = std::fs::read_to_string(sysroot.join("/etc/os-release")).unwrap_or_default();
    let mut kv = std::collections::BTreeMap::new();
    for line in os_release.lines() {
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k.to_string(), v.trim_matches('"').to_string());
        }
    }
    let unknown = || "unknown".to_string();
    let read_first = |paths: &[&str]| {
        paths
            .iter()
            .find_map(|path| {
                std::fs::read_to_string(sysroot.join(path))
                    .ok()
                    .map(|value| value.trim_matches('\0').trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_else(unknown)
    };
    let os_version = kv
        .get("PRETTY_NAME")
        .or_else(|| kv.get("VERSION"))
        .cloned()
        .unwrap_or_else(unknown);
    let os_revision = kv
        .get("BUILD_ID")
        .or_else(|| kv.get("VERSION_ID"))
        .cloned()
        .unwrap_or_else(unknown);
    Ok(Json(serde_json::json!({
        "platform": m.platform,
        "model": m.model,
        "board_name": m.board_name,
        "hardware_revision": configured.hardware_revision.or_else(|| Some(read_first(&[
            "/etc/hardware-revision",
            "/etc/board-revision",
            "/proc/device-tree/chosen/board-revision",
        ]))),
        "serial_number": configured.serial_number,
        "has_rtc": m.has_rtc,
        "os_name": kv.get("NAME").cloned().unwrap_or_else(unknown),
        "os_version": os_version,
        "os_revision": os_revision,
        "kernel_version": read_first(&["/proc/sys/kernel/osrelease"]),
        "bootloader_version": read_first(&[
            "/etc/bootloader-version",
            "/etc/bootloader_version",
            "/proc/device-tree/chosen/bootloader-version",
        ]),
        "software_name": "WebConfig",
        "software_version": env!("CARGO_PKG_VERSION"),
        "software_revision": std::env::var("WEBCONFIG_REVISION").unwrap_or_else(|_| unknown()),
        "os_release": kv,
        "uptime_secs": webconfig_engine::monitoring::read_vitals(sysroot).uptime_secs,
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
        return Err(ApiError::BadRequest(
            "mode must be settings|settings+users".into(),
        ));
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
    Ok(Json(
        serde_json::json!({ "factory_reset": true, "mode": req.mode }),
    ))
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
                groups: u.groups.clone(),
            })
            .collect(),
    ))
}

#[derive(Serialize)]
pub struct UserPublic {
    pub username: String,
    pub role: String,
    pub groups: Vec<String>,
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
    audit(
        &app,
        &username(&session).await,
        "users.create",
        Some(req.username.clone()),
    );
    Ok(Json(UserPublic {
        username: req.username,
        role: parsed.as_str().to_string(),
        groups: Vec::new(),
    }))
}

#[derive(Serialize)]
pub struct GroupPublic {
    pub name: String,
    pub members: Vec<String>,
}

/// `GET /api/v1/system/groups` — list groups and their members.
pub async fn list_groups(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<GroupPublic>>> {
    let store = app.load_users().map_err(|_| ApiError::Internal)?;
    Ok(Json(
        store
            .groups
            .into_iter()
            .map(|group| GroupPublic {
                name: group.name,
                members: group.members,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
}

/// `POST /api/v1/system/groups` — create a group (admin only).
pub async fn create_group(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<CreateGroupRequest>,
) -> ApiResult<Json<GroupPublic>> {
    role.require_admin("groups.create")?;
    let mut store = app.load_users().map_err(|_| ApiError::Internal)?;
    store.add_group(&req.name)?;
    app.save_users(&store).map_err(|_| ApiError::Internal)?;
    audit(
        &app,
        &username(&session).await,
        "groups.create",
        Some(req.name.clone()),
    );
    Ok(Json(GroupPublic {
        name: req.name,
        members: Vec::new(),
    }))
}

/// `DELETE /api/v1/system/groups/:name` — delete a group (admin only).
pub async fn delete_group(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("groups.delete")?;
    let mut store = app.load_users().map_err(|_| ApiError::Internal)?;
    store.remove_group(&name)?;
    app.save_users(&store).map_err(|_| ApiError::Internal)?;
    audit(
        &app,
        &username(&session).await,
        "groups.delete",
        Some(name.clone()),
    );
    Ok(Json(serde_json::json!({ "deleted": name })))
}

/// `PUT/DELETE /api/v1/system/groups/:name/members/:user` — assign or remove
/// a user from a group (admin only).
pub async fn add_group_member(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path((group, user)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("groups.members.put")?;
    let mut store = app.load_users().map_err(|_| ApiError::Internal)?;
    store.add_member(&group, &user)?;
    app.save_users(&store).map_err(|_| ApiError::Internal)?;
    audit(
        &app,
        &username(&session).await,
        "groups.members.put",
        Some(format!("{group}:{user}")),
    );
    Ok(Json(
        serde_json::json!({ "group": group, "user": user, "member": true }),
    ))
}

pub async fn remove_group_member(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path((group, user)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("groups.members.delete")?;
    let mut store = app.load_users().map_err(|_| ApiError::Internal)?;
    store.remove_member(&group, &user)?;
    app.save_users(&store).map_err(|_| ApiError::Internal)?;
    audit(
        &app,
        &username(&session).await,
        "groups.members.delete",
        Some(format!("{group}:{user}")),
    );
    Ok(Json(
        serde_json::json!({ "group": group, "user": user, "member": false }),
    ))
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
    audit(
        &app,
        &username(&session).await,
        "users.delete",
        Some(name.clone()),
    );
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
    audit(
        &app,
        &username(&session).await,
        "users.set_role",
        Some(name.clone()),
    );
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
    audit(
        &app,
        &username(&session).await,
        "users.change_password",
        Some(name.clone()),
    );
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
    audit(
        &app,
        &username(&session).await,
        "system.root_password",
        None,
    );
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
