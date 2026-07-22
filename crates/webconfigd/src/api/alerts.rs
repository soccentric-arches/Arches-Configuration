//! Alerts handlers (spec §4.7): threshold rules with duration, history,
//! webhook/MQTT actions.

use axum::extract::State;
use axum::Json;
use tower_sessions::Session;
use webconfig_engine::alert::AlertHistory;
use webconfig_engine::store::alerts::{AlertRule, AlertsConfig};

use crate::api::{audit, username};
use crate::auth::AuthRole;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /api/v1/alerts` — rules + firing state + history + the per-platform
/// default rule packs shipped in the board manifest (spec §4.7). Defaults are
/// read-only here; they are merged with `rules` at evaluation time, and an
/// operator overrides or disables one by adding a same-id rule to `rules`.
pub async fn get(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let cfg = app.engine.load()?;
    let history = AlertHistory::new(app.engine.sysroot.webconfig_dir().join("config.db"), 1000)
        .read_all()
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "enabled": cfg.alerts.enabled,
        "rules": cfg.alerts.rules,
        "default_rules": app.manifest.default_alert_rules,
        "webhooks": cfg.alerts.webhooks,
        "mqtt_event_topic": cfg.alerts.mqtt_event_topic,
        "history": history,
    })))
}

/// `PUT /api/v1/alerts` — update alert rules (admin only).
pub async fn put(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<AlertsConfig>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("alerts.put")?;
    for r in &req.rules {
        validate_rule(r)?;
    }
    let user = username(&session).await;
    let cfg = app.engine.update(move |c| {
        c.alerts = req.clone();
        Ok(())
    })?;
    audit(&app, &user, "alerts.put", None);
    Ok(Json(serde_json::to_value(&cfg.alerts).unwrap_or_default()))
}

fn validate_rule(r: &AlertRule) -> ApiResult<()> {
    if r.id.is_empty() {
        return Err(ApiError::BadRequest("rule id empty".into()));
    }
    if !matches!(r.comparison.as_str(), ">" | "<" | ">=" | "<=" | "==" | "!=") {
        return Err(ApiError::BadRequest(format!(
            "rule {} bad comparison",
            r.id
        )));
    }
    Ok(())
}

/// `GET /api/v1/alerts/history` — standalone history read (spec §4.7).
pub async fn history(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<Vec<webconfig_engine::alert::AlertEvent>>> {
    let h = AlertHistory::new(app.engine.sysroot.webconfig_dir().join("config.db"), 1000)
        .read_all()
        .unwrap_or_default();
    Ok(Json(h))
}
