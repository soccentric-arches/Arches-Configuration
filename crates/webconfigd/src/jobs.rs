//! Background feature workers owned by the daemon process.

use std::sync::Arc;

use webconfig_engine::alert::AlertEngine;
use webconfig_engine::manifest::BoardManifest;

use crate::bootstrap;
use crate::engine_bridge::EngineBridge;

/// Periodically evaluate configured alert rules against current vitals.
pub(crate) async fn run_alert_timer(
    engine: EngineBridge,
    manifest: Arc<BoardManifest>,
    alert_engine: Arc<tokio::sync::Mutex<AlertEngine>>,
) {
    let history = bootstrap::alert_history(&engine);
    let mut tick: u64 = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        tick += 5;
        let config = match engine.load() {
            Ok(config) => config,
            Err(_) => continue,
        };
        let vitals = webconfig_engine::monitoring::read_vitals(&engine.sysroot);
        let events = alert_engine
            .lock()
            .await
            .evaluate(&config.alerts.rules, &vitals, tick);
        for event in events {
            tracing::info!(rule = %event.rule_id, state = ?event.state, "alert");
            if let Err(error) = history.record(&event) {
                tracing::warn!(error = %error, "alert history write failed");
            }
        }
        let _ = &manifest; // platform default rule packs will be merged here
    }
}
