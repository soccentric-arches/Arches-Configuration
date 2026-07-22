//! Services & Apps handlers (spec §4.3).
//!
//! Curated service list from the manifest allowlist (never enumerate all
//! units). Per service: status, enable-on-boot, start/stop/restart, last ~20
//! journal lines when failed. The schema-driven app settings engine: GET/PUT
//! `/apps/:id/settings` reads/writes a slice of `config.app_settings`, the
//! shape governed by a schema file — adding a setting is zero UI code (spec
//! §3/§4.3). MQTT/cloud panel.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use std::sync::Arc;
use tower_sessions::Session;
use webconfig_engine::store::services::MqttConfig;

use crate::api::{audit, username};
use crate::auth::AuthRole;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::systemd::SystemdClient;

/// `GET /api/v1/services` — curated list + status (spec §4.3).
pub async fn list(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let mut out = Vec::new();
    for unit in &app.manifest.service_allowlist {
        let st = app.systemd.status(unit).await.ok();
        out.push(serde_json::json!({
            "unit": unit,
            "status": st,
        }));
    }
    Ok(Json(out))
}

/// `POST /api/v1/services/:unit/start` (operator+, spec §4.1).
pub async fn start(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(unit): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_mutate_apps("services.start")?;
    guard_allowed(&app, &unit)?;
    app.systemd.start(&unit).await.ok();
    audit(
        &app,
        &username(&session).await,
        "services.start",
        Some(unit.clone()),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn stop(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(unit): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_mutate_apps("services.stop")?;
    guard_allowed(&app, &unit)?;
    app.systemd.stop(&unit).await.ok();
    audit(
        &app,
        &username(&session).await,
        "services.stop",
        Some(unit.clone()),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn restart(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(unit): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_mutate_apps("services.restart")?;
    guard_allowed(&app, &unit)?;
    app.systemd.restart(&unit).await.ok();
    audit(
        &app,
        &username(&session).await,
        "services.restart",
        Some(unit.clone()),
    );
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(serde::Deserialize)]
pub struct EnableRequest {
    pub enabled: bool,
}

/// `PUT /api/v1/services/:unit/enabled` — enable-on-boot toggle (spec §4.3).
pub async fn set_enabled(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(unit): Path<String>,
    Json(req): Json<EnableRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_mutate_apps("services.enable")?;
    guard_allowed(&app, &unit)?;
    let _ = if req.enabled {
        app.systemd.enable(&unit).await
    } else {
        app.systemd.disable(&unit).await
    };
    audit(
        &app,
        &username(&session).await,
        "services.set_enabled",
        Some(format!("{unit}={}", req.enabled)),
    );
    Ok(Json(
        serde_json::json!({ "unit": unit, "enabled": req.enabled }),
    ))
}

/// `GET /api/v1/services/:unit/journal` — last ~20 lines when failed (spec §4.3).
pub async fn journal(
    State(app): State<AppState>,
    _role: AuthRole,
    Path(unit): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    guard_allowed(&app, &unit)?;
    let tail = app
        .systemd
        .journal_tail(&unit, 20)
        .await
        .unwrap_or_default();
    Ok(Json(serde_json::json!({ "unit": unit, "tail": tail })))
}

fn guard_allowed(app: &AppState, unit: &str) -> ApiResult<()> {
    if app.manifest.service_allowlist.iter().any(|u| u == unit) {
        Ok(())
    } else {
        Err(ApiError::BadRequest("unit not in allowlist".into()))
    }
}

// --- MQTT / cloud (spec §4.3) ---------------------------------------------

/// `GET /api/v1/services/mqtt`
pub async fn get_mqtt(State(app): State<AppState>, _role: AuthRole) -> ApiResult<Json<MqttConfig>> {
    Ok(Json(app.engine.load()?.services.mqtt))
}

/// `PUT /api/v1/services/mqtt` (operator+).
pub async fn put_mqtt(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<MqttConfig>,
) -> ApiResult<Json<MqttConfig>> {
    role.require_mutate_apps("services.mqtt.put")?;
    let user = username(&session).await;
    let cfg = app.engine.update(move |c| {
        c.services.mqtt = req.clone();
        Ok(())
    })?;
    audit(&app, &user, "services.mqtt.put", None);
    Ok(Json(cfg.services.mqtt))
}

/// `POST /api/v1/services/mqtt/test` — live connection test (spec §4.3).
pub async fn test_mqtt(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let m = app.engine.load()?.services.mqtt;
    let host = m.host.unwrap_or_default();
    if host.is_empty() {
        return Err(ApiError::BadRequest("mqtt host not set".into()));
    }
    // Best-effort TCP connect to the broker; full MQTT handshake is out of
    // scope for v1 and would pull a broker client crate (spec §7: state it).
    let ok = std::net::TcpStream::connect(format!("{}:{}", host, m.port.unwrap_or(1883))).is_ok();
    Ok(Json(serde_json::json!({ "reachable": ok })))
}

// --- Schema-driven app settings (spec §3 / §4.3 centerpiece) ---------------

/// `GET /api/v1/schemas` — schema descriptors for the frontend renderer.
pub async fn schemas(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let out = app
        .schema
        .descriptors()
        .iter()
        .map(|(k, d)| {
            serde_json::json!({ "id": k, "descriptor": serde_json::to_value(d).unwrap_or_default() })
        })
        .collect();
    Ok(Json(out))
}

/// `GET /api/v1/apps/:id/settings` — a slice of `config.app_settings`.
pub async fn get_app_settings(
    State(app): State<AppState>,
    _role: AuthRole,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let cfg = app.engine.load()?;
    let val = cfg
        .app_settings
        .get(&id)
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    Ok(Json(val))
}

/// `PUT /api/v1/apps/:id/settings` — write a schema-driven app settings slice
/// (operator+). Validation is performed by the store's schema validator on
/// commit (spec §3: server-side re-validation).
pub async fn put_app_settings(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_mutate_apps("apps.settings.put")?;
    let user = username(&session).await;
    let id_for_update = id.clone();
    let req_for_update = req.clone();
    app.engine.update(move |c| {
        c.app_settings.insert(id_for_update, req_for_update);
        Ok(())
    })?;
    audit(&app, &user, "apps.settings.put", Some(id.clone()));
    Ok(Json(req))
}

// Re-export the SystemdClient trait so handlers that take it via FromRef can
// name the Arc<dyn> extractor without re-importing at every call site.
#[allow(dead_code)]
type _Sys = Arc<dyn SystemdClient>;

/// Services-page summary (health strip: green/yellow/red, spec §4.5).
#[derive(Serialize)]
pub struct ServiceHealth {
    pub unit: String,
    pub state: String,
}

pub async fn health_strip(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<ServiceHealth>>> {
    let mut out = Vec::new();
    for unit in &app.manifest.service_allowlist {
        let state = app
            .systemd
            .status(unit)
            .await
            .map(|s| s.active)
            .unwrap_or_else(|_| "unknown".into());
        out.push(ServiceHealth {
            unit: unit.clone(),
            state,
        });
    }
    Ok(Json(out))
}
