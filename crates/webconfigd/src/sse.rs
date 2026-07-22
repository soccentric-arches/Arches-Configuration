//! SSE endpoint (spec §3: one SSE endpoint for log follow + dashboard push;
//! spec §4.6 follow mode via SSE).

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::{self, Stream};
use std::convert::Infallible;
use std::time::Duration;

use crate::auth::AuthRole;
use crate::error::ApiResult;
use crate::state::AppState;

/// `GET /api/v1/events` — Server-Sent Events: periodic vitals push + (on the
/// real root) journal follow. The browser subscribes while the Logs/Monitoring
/// pages are open.
pub async fn events(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let sys = app.engine.sysroot.clone();
    let stream = stream::unfold(0u64, move |i| {
        let sys = sys.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let payload = if sys.is_real_root() {
                serde_json::to_string(&webconfig_engine::monitoring::read_vitals(&sys))
                    .unwrap_or_default()
            } else {
                serde_json::json!({ "tick": i }).to_string()
            };
            Some((
                Ok::<_, Infallible>(Event::default().event("vitals").data(payload)),
                i + 1,
            ))
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
