//! Networking section handlers (spec §4.2 — flagship commit/confirm rollback).
//!
//! Ethernet (dynamic enumeration), Wi-Fi scan/join, AP provisioning mode, DNS,
//! service-based firewall, connectivity test. Any severing change is applied
//! *provisionally*; the user must `POST /network/confirm` within the window or
//! the backend auto-rolls-back via the engine bridge's confirm timer
//! (spec §4.2).

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use tower_sessions::Session;
use webconfig_engine::store::network::NetworkConfig;

use crate::api::{apply_and_audit, audit, username};
use crate::auth::AuthRole;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /api/v1/network` — the network section + enumerated live interfaces.
pub async fn get(State(app): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let cfg = app.engine.load()?;
    // Live interface enumeration from /sys/class/net (spec §4.2: never
    // hardcoded; Pinnacle/Sequoia have multiple MACs).
    let netdir = app.engine.sysroot.join("/sys/class/net");
    let mut live = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&netdir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name == "lo" {
                continue;
            }
            let mac = std::fs::read_to_string(netdir.join(&name).join("address"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let operstate = std::fs::read_to_string(netdir.join(&name).join("operstate"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".into());
            live.push(serde_json::json!({ "name": name, "mac": mac, "operstate": operstate }));
        }
    }
    Ok(Json(serde_json::json!({
        "config": cfg.network,
        "live": live,
    })))
}

/// `PUT /api/v1/network` — update the network section. Admin only (operator
/// may not edit network — severing risk).
pub async fn put(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<NetworkConfig>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("network.put")?;
    let user = username(&session).await;
    let cfg = app.engine.update(move |c| {
        c.network = req.clone();
        Ok(())
    })?;
    audit(&app, &user, "network.put", None);
    Ok(Json(serde_json::to_value(&cfg.network).unwrap_or_default()))
}

/// `POST /api/v1/network/apply` — apply networkd + firewall. Returns
/// `provisional: true` when the change could sever the session (spec §4.2).
pub async fn apply(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("network.apply")?;
    let user = username(&session).await;
    app.engine.validate("networkd").await?;
    app.engine.validate("firewall").await?;
    let outcome = apply_and_audit(&app, &user, "networkd").await?;
    let _ = apply_and_audit(&app, &user, "firewall").await?;
    let provisional = matches!(outcome, webconfig_engine::ApplyOutcome::Provisional { .. });
    let window = if let webconfig_engine::ApplyOutcome::Provisional {
        confirm_window_secs,
    } = outcome
    {
        confirm_window_secs
    } else {
        0
    };
    Ok(Json(serde_json::json!({
        "provisional": provisional,
        "confirm_window_secs": window,
    })))
}

/// `POST /api/v1/network/confirm` — confirm the provisional change (spec §4.2).
pub async fn confirm(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("network.confirm")?;
    let user = username(&session).await;
    let ok = app.engine.confirm_pending().await?;
    audit(
        &app,
        &user,
        "network.confirm",
        Some(format!("confirmed={ok}")),
    );
    Ok(Json(serde_json::json!({ "confirmed": ok })))
}

/// `POST /api/v1/network/rollback` — manually roll back the provisional change.
pub async fn rollback(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("network.rollback")?;
    let user = username(&session).await;
    app.engine.confirm_pending().await.ok(); // clear pending
    app.engine.rollback("networkd").await?;
    app.engine.rollback("firewall").await.ok();
    audit(&app, &user, "network.rollback", None);
    Ok(Json(serde_json::json!({ "rolled_back": true })))
}

/// `GET /api/v1/network/scan` — Wi-Fi scan (SSID/signal list, spec §4.2).
/// On the real root shells out to `iw dev wlan0 scan`; fake sysroot returns
/// the manifest-declared wlan0 + an empty list so the UI renders.
pub async fn scan(State(app): State<AppState>, _role: AuthRole) -> ApiResult<Json<Vec<WifiScan>>> {
    if app.engine.sysroot.is_real_root() {
        let out = webconfig_engine::applier::run_cmd("iw", &["dev", "wlan0", "scan"])
            .map_err(ApiError::from)?;
        // Parse the `iw scan` dump for SSID + signal. Best-effort.
        let mut nets = Vec::new();
        let mut cur: Option<WifiScan> = None;
        for line in out.lines() {
            let line = line.trim();
            if line.starts_with("BSS ") {
                if let Some(c) = cur.take() {
                    nets.push(c);
                }
                cur = Some(WifiScan::default());
            } else if let Some(s) = line.strip_prefix("SSID: ") {
                if let Some(c) = cur.as_mut() {
                    c.ssid = s.trim().to_string();
                }
            } else if let Some(s) = line.strip_prefix("signal: ") {
                if let Some(c) = cur.as_mut() {
                    c.signal_dbm = s
                        .split_whitespace()
                        .next()
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0);
                }
            }
        }
        if let Some(c) = cur.take() {
            nets.push(c);
        }
        return Ok(Json(nets));
    }
    Ok(Json(Vec::new()))
}

#[derive(Serialize, Default, serde::Deserialize)]
pub struct WifiScan {
    pub ssid: String,
    pub signal_dbm: i32,
}

/// `POST /api/v1/network/connectivity-test` — ping gateway → public IP → DNS
/// resolve, reporting which layer failed (spec §4.2).
#[derive(Serialize)]
pub struct ConnectivityResult {
    pub gateway: LayerResult,
    pub public: LayerResult,
    pub dns: LayerResult,
}

#[derive(Serialize)]
pub struct LayerResult {
    pub ok: bool,
    pub detail: String,
}

pub async fn connectivity_test(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<ConnectivityResult>> {
    let sys = &app.engine.sysroot;
    let run = |cmd: &str, args: &[&str]| -> (bool, String) {
        if !sys.is_real_root() {
            return (true, "skipped (fake sysroot)".into());
        }
        match webconfig_engine::applier::run_cmd(cmd, args) {
            Ok(_) => (true, "ok".into()),
            Err(e) => (false, e.to_string()),
        }
    };
    Ok(Json(ConnectivityResult {
        gateway: {
            let (ok, d) = run("ping", &["-c", "1", "-W", "1", "192.168.1.1"]);
            LayerResult { ok, detail: d }
        },
        public: {
            let (ok, d) = run("ping", &["-c", "1", "-W", "1", "1.1.1.1"]);
            LayerResult { ok, detail: d }
        },
        dns: {
            let (ok, d) = run("getent", &["hosts", "example.com"]);
            LayerResult { ok, detail: d }
        },
    }))
}
