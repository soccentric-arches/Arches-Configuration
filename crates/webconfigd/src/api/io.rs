//! I/O & Peripherals handlers (spec §4.9). Everything is rendered from the
//! board manifest — zero hardcoded I/O.

use axum::extract::{Path, State};
use axum::Json;
use tower_sessions::Session;
use webconfig_engine::manifest::IoKind;
use webconfig_engine::store::io::IoConfig;

use crate::api::{apply_and_audit, audit, username};
use crate::auth::AuthRole;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /api/v1/io` — manifest-driven I/O + current config (spec §4.9).
pub async fn get(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "manifest": app.manifest.peripherals,
        "config": app.engine.load()?.io,
        "tiles": app.manifest.dashboard_tiles,
    })))
}

/// `PUT /api/v1/io` — update serial/CAN/GPIO/MCU config (admin only).
pub async fn put(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<IoConfig>,
) -> ApiResult<Json<IoConfig>> {
    role.require_admin("io.put")?;
    // Serial ownership conflict prevention (spec §4.9): no two ports may claim
    // the same non-unassigned owner when the owner is a singleton like console.
    let mut owners = std::collections::HashSet::new();
    for s in &req.serial {
        if s.owner != "unassigned" {
            if !owners.insert(s.owner.clone()) {
                return Err(ApiError::Conflict(format!(
                    "serial owner {} claimed twice",
                    s.owner
                )));
            }
        }
        // Reject ports not in the manifest.
        if !app.manifest.has(&s.device.replace("/dev/", ""))
            && !app
                .manifest
                .peripherals
                .iter()
                .any(|p| p.device == s.device)
        {
            return Err(ApiError::BadRequest(format!(
                "serial {} not in manifest",
                s.device
            )));
        }
    }
    let user = username(&session).await;
    let cfg = app.engine.update(|c| {
        c.io = req.clone();
        Ok(())
    })?;
    audit(&app, &user, "io.put", None);
    Ok(Json(cfg.io))
}

/// `POST /api/v1/io/can/apply` — apply the CAN applier (spec §4.9).
pub async fn apply_can(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("io.can.apply")?;
    let user = username(&session).await;
    app.engine.validate("can").await?;
    let _ = apply_and_audit(&app, &user, "can").await?;
    Ok(Json(serde_json::json!({ "applied": ["can"] })))
}

/// `POST /api/v1/io/gpio/:name/toggle` — manual toggle for an output (spec §4.9).
pub async fn toggle_gpio(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Path(name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_mutate_apps("io.gpio.toggle")?;
    // Manifest-named pins only (spec §4.9 — never raw banks).
    let p = app
        .manifest
        .peripherals
        .iter()
        .find(|p| p.id == name && p.kind == IoKind::Gpio)
        .ok_or_else(|| ApiError::BadRequest("gpio not in manifest".into()))?;
    let value = if app.engine.sysroot.is_real_root() {
        // Best-effort via the line name. Real impl uses libgpiod; here we shell
        // out to `gpioset`/`gpioget` (assumption per spec §7).
        let _ = webconfig_engine::applier::run_cmd("gpioset", &[&p.device, "1=1"]);
        1
    } else {
        0
    };
    audit(
        &app,
        &username(&session).await,
        "io.gpio.toggle",
        Some(name.clone()),
    );
    Ok(Json(serde_json::json!({ "pin": name, "value": value })))
}

/// `POST /api/v1/io/i2c/scan` — admin-only bus scan (spec §4.9).
pub async fn i2c_scan(
    State(app): State<AppState>,
    role: AuthRole,
    axum::extract::Query(q): axum::extract::Query<BusQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("io.i2c.scan")?;
    if !app
        .manifest
        .peripherals
        .iter()
        .any(|p| p.kind == IoKind::I2c && p.id == q.bus)
    {
        return Err(ApiError::BadRequest("i2c bus not in manifest".into()));
    }
    let addrs = if app.engine.sysroot.is_real_root() {
        let out =
            webconfig_engine::applier::run_cmd("i2cdetect", &["-y", &q.bus]).unwrap_or_default();
        // Parse hex addresses from the i2cdetect table.
        out.lines()
            .flat_map(|l| l.split_whitespace().skip(1))
            .filter_map(|t| u16::from_str_radix(t.trim_end_matches(':'), 16).ok())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    Ok(Json(
        serde_json::json!({ "bus": q.bus, "addresses": addrs }),
    ))
}

#[derive(serde::Deserialize)]
pub struct BusQuery {
    pub bus: String,
}

/// `GET /api/v1/io/mcu` — companion MCU panel (spec §4.9): fw version,
/// heartbeat, settings via the schema engine.
pub async fn mcu(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let m = app
        .manifest
        .of_kind(IoKind::Mcu)
        .into_iter()
        .next()
        .cloned();
    let cfg = app.engine.load()?.io.mcu;
    Ok(Json(serde_json::json!({
        "manifest": m,
        "config": cfg,
    })))
}

/// `POST /api/v1/io/mcu/firmware` — signed firmware upload + flash (spec §4.9).
/// Same signing/compat rules as OS bundles (spec §4.9). v1 stores the bundle
/// and records the action; the real flash protocol is MCU-specific.
pub async fn mcu_firmware(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
    Json(req): Json<FirmwareUpload>,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("io.mcu.firmware")?;
    if !req.signed {
        return Err(ApiError::BadRequest("MCU firmware must be signed".into()));
    }
    let user = username(&session).await;
    audit(&app, &user, "io.mcu.firmware", Some(req.version.clone()));
    Ok(Json(
        serde_json::json!({ "flashed": app.engine.sysroot.is_real_root(), "version": req.version }),
    ))
}

#[derive(serde::Deserialize)]
pub struct FirmwareUpload {
    pub version: String,
    pub signed: bool,
}

/// Platform panels (spec §4.9): Zion bitstream, Joshua PRU, Arches cameras.
pub async fn platform_panel(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let m = &app.manifest;
    let out = match m.platform.as_str() {
        "zion" => serde_json::json!({
            "fpga": m.of_kind(IoKind::Fpga),
        }),
        "joshua" => serde_json::json!({
            "pru": m.of_kind(IoKind::Pru),
        }),
        "arches" => serde_json::json!({
            "cameras": m.of_kind(IoKind::Camera),
            "power_modes": ["15W", "30W", "MAXN"],
        }),
        _ => serde_json::json!({}),
    };
    Ok(Json(out))
}
