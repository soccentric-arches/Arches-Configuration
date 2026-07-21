//! REST API handlers for the nine functional sections (spec §4).
//!
//! Routes are mounted under `/api/v1/...` by [`crate::routes`]. Every mutating
//! route requires an authenticated session (admin or operator per action) and
//! a valid CSRF token (spec §4.8). The security boundary is the API: all input
//! is re-validated server-side by the engine + appliers (spec §2).

pub mod alerts;
pub mod io;
pub mod logs;
pub mod meta;
pub mod monitoring;
pub mod network;
pub mod security;
pub mod services;
pub mod system;
pub mod updates;

use axum::extract::State;
use webconfig_engine::audit::AuditEntry;

use crate::auth::AuthRole;
use crate::error::ApiResult;
use crate::state::AppState;

/// Append an audit entry (spec §4.8). Failures are logged, not fatal — the
/// protected action already succeeded.
pub fn audit(app: &AppState, user: &str, action: &str, detail: Option<String>) {
    if let Err(e) = app.audit.append(&AuditEntry {
        ts: now_iso(),
        user: user.to_string(),
        action: action.to_string(),
        detail,
    }) {
        tracing::warn!(error = %e, "audit append failed");
    }
}

/// Resolve the username from the session role extension. The `AuthRole`
/// extractor only carries the role; the username travels via the session and
/// is read here for audit attribution.
pub async fn username(session: &tower_sessions::Session) -> String {
    session
        .get::<String>("username")
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string())
}

/// Reusable "admin required" gate for handlers that take a role + session.
pub fn require_admin(role: &AuthRole, app: &AppState, user: &str, action: &str) -> ApiResult<()> {
    role.require_admin(action)?;
    audit(app, user, action, None);
    Ok(())
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("t={secs}")
}

/// Helper to apply a section's applier by id and audit the result.
pub async fn apply_and_audit(
    app: &AppState,
    user: &str,
    applier_id: &str,
) -> ApiResult<webconfig_engine::ApplyOutcome> {
    let outcome = app.engine.apply(applier_id).await?;
    audit(
        app,
        user,
        &format!("{applier_id}.apply"),
        Some(format!("{:?}", outcome)),
    );
    Ok(outcome)
}