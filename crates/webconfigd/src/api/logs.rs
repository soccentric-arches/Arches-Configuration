//! Logs handlers (spec §4.6): journal viewer + SSE follow, log config, audit
//! log view, download.

use axum::extract::State;
use axum::Json;
use tower_sessions::Session;
use webconfig_engine::store::logs::LogsConfig;

use crate::api::{apply_and_audit, audit, username};
use crate::auth::AuthRole;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /api/v1/logs?unit=&priority=&since=&until=&limit=` — journal read.
/// On the real root shells out to `journalctl`; fake sysroot returns empty.
pub async fn read(
    State(app): State<AppState>,
    _role: AuthRole,
    axum::extract::Query(q): axum::extract::Query<LogsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if !app.engine.sysroot.is_real_root() {
        return Ok(Json(serde_json::json!({ "lines": [] })));
    }
    let mut args: Vec<String> = vec!["--no-pager".into(), "--output=json".into()];
    if !q.unit.is_empty() {
        args.push("-u".into());
        args.push(q.unit.clone());
    }
    if !q.priority.is_empty() {
        args.push("-p".into());
        args.push(q.priority.clone());
    }
    if !q.since.is_empty() {
        args.push("--since".into());
        args.push(q.since.clone());
    }
    if !q.until.is_empty() {
        args.push("--until".into());
        args.push(q.until.clone());
    }
    let limit = q.limit.unwrap_or(200);
    args.push("-n".into());
    args.push(limit.to_string());
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = webconfig_engine::applier::run_cmd("journalctl", &refs)
        .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "lines": out.lines().collect::<Vec<_>>() })))
}

#[derive(serde::Deserialize, Default)]
pub struct LogsQuery {
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub since: String,
    #[serde(default)]
    pub until: String,
    pub limit: Option<usize>,
}

/// `GET /api/v1/logs/config` — journald config (spec §4.6).
pub async fn get_config(State(app): State<AppState>, _role: AuthRole) -> ApiResult<Json<LogsConfig>> {
    Ok(Json(app.engine.load()?.logs))
}

/// `PUT /api/v1/logs/config` — apply journald config (admin only; writes via
/// the journald applier, spec §4.6).
pub async fn put_config(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<LogsConfig>,
) -> ApiResult<Json<LogsConfig>> {
    role.require_admin("logs.put")?;
    let user = username(&session).await;
    let cfg = app
        .engine
        .update(|c| {
            c.logs = req.clone();
            Ok(())
        })?;
    let _ = apply_and_audit(&app, &user, "journald").await?;
    Ok(Json(cfg.logs))
}

/// `GET /api/v1/logs/audit` — the audit log, viewable under Logs (spec §4.8).
pub async fn audit_log(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<webconfig_engine::audit::AuditEntry>>> {
    Ok(Json(app.audit.read_all().unwrap_or_default()))
}

/// `GET /api/v1/logs/download` — full journal export (spec §4.6).
pub async fn download(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    if !app.engine.sysroot.is_real_root() {
        return Ok(Json(serde_json::json!({ "export": "" })));
    }
    let out = webconfig_engine::applier::run_cmd("journalctl", &["--no-pager"])
        .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "export": out })))
}