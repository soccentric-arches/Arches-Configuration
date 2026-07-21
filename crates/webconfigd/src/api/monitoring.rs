//! Monitoring handlers (spec §4.5 — no resident collectors; polled on request).

use axum::extract::{Path, State};
use axum::Json;
use webconfig_engine::monitoring;

use crate::auth::AuthRole;
use crate::error::ApiResult;
use crate::state::AppState;

/// `GET /api/v1/monitoring/vitals` — polled every few seconds while the page
/// is open (spec §4.5).
pub async fn vitals(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<monitoring::Vitals>> {
    Ok(Json(monitoring::read_vitals(&app.engine.sysroot)))
}

/// `GET /api/v1/monitoring/emmc/:dev` — eMMC wear (spec §4.5).
pub async fn emmc_wear(
    State(app): State<AppState>,
    _role: AuthRole,
    Path(dev): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let wear = monitoring::read_emmc_wear(&app.engine.sysroot, &dev);
    Ok(Json(serde_json::json!({ "dev": dev, "life_time_estimate": wear })))
}

/// `GET /api/v1/monitoring/tiles` — platform tiles declared in the manifest
/// (spec §4.5). The frontend pulls the listed metrics from `/vitals` /
/// platform-specific endpoints.
pub async fn tiles(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<webconfig_engine::manifest::DashboardTile>>> {
    Ok(Json(app.manifest.dashboard_tiles.clone()))
}