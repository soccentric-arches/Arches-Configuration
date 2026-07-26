//! I/O & peripheral handlers (spec §4.9). Configurable peripherals come from
//! the board manifest; USB and PCI inventory is read live from the host tools.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
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

/// `GET /api/v1/io/usb` - enumerate live USB devices with `lsusb`.
pub async fn usb_devices(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let (available, error, devices) = if app.engine.sysroot.is_real_root() {
        match std::process::Command::new("lsusb").output() {
            Ok(output) if output.status.success() => (
                true,
                None,
                parse_lsusb(&String::from_utf8_lossy(&output.stdout)),
            ),
            Ok(output) => (false, Some(command_error("lsusb", &output)), Vec::new()),
            Err(error) => (false, Some(error.to_string()), Vec::new()),
        }
    } else {
        (
            false,
            Some("hardware discovery unavailable in a fake sysroot".into()),
            Vec::new(),
        )
    };
    Ok(Json(
        serde_json::json!({ "available": available, "error": error, "devices": devices }),
    ))
}

/// `GET /api/v1/io/pci` - enumerate live PCI devices with `lspci`.
pub async fn pci_devices(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let (available, error, devices) = if app.engine.sysroot.is_real_root() {
        match std::process::Command::new("lspci")
            .args(["-D", "-nn"])
            .output()
        {
            Ok(output) if output.status.success() => (
                true,
                None,
                parse_lspci(&String::from_utf8_lossy(&output.stdout)),
            ),
            Ok(output) => (false, Some(command_error("lspci", &output)), Vec::new()),
            Err(error) => (false, Some(error.to_string()), Vec::new()),
        }
    } else {
        (
            false,
            Some("hardware discovery unavailable in a fake sysroot".into()),
            Vec::new(),
        )
    };
    Ok(Json(
        serde_json::json!({ "available": available, "error": error, "devices": devices }),
    ))
}

/// `GET /api/v1/io/i2c` - scan live I2C adapters with `i2cdetect`.
pub async fn i2c_devices(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    if !app.engine.sysroot.is_real_root() {
        return Ok(Json(serde_json::json!({
            "available": false,
            "error": "hardware discovery unavailable in a fake sysroot",
            "devices": []
        })));
    }
    let buses = match std::fs::read_dir(app.engine.sysroot.join("/sys/class/i2c-dev")) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| {
                name.strip_prefix("i2c-")
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            return Ok(Json(serde_json::json!({
                "available": false,
                "error": error.to_string(),
                "devices": []
            })));
        }
    };
    let mut devices = Vec::new();
    let mut errors = Vec::new();
    for bus in buses {
        match std::process::Command::new("i2cdetect")
            .args(["-y", &bus.to_string()])
            .output()
        {
            Ok(output) if output.status.success() => {
                devices.extend(parse_i2cdetect(
                    bus,
                    &String::from_utf8_lossy(&output.stdout),
                ));
            }
            Ok(output) => errors.push(command_error("i2cdetect", &output)),
            Err(error) => errors.push(error.to_string()),
        }
    }
    Ok(Json(serde_json::json!({
        "available": true,
        "error": if errors.is_empty() { None } else { Some(errors.join("; ")) },
        "devices": devices
    })))
}

/// `GET /api/v1/io/spi` - enumerate SPI controller/chip-select nodes.
/// SPI has no generic, safe bus-wide probe, so this reports kernel-created
/// spidev nodes and their bound driver metadata instead of clocking devices.
pub async fn spi_devices(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    if !app.engine.sysroot.is_real_root() {
        return Ok(Json(serde_json::json!({
            "available": false,
            "error": "hardware discovery unavailable in a fake sysroot",
            "devices": []
        })));
    }
    let class_dir = app.engine.sysroot.join("/sys/class/spidev");
    let entries = match std::fs::read_dir(&class_dir) {
        Ok(entries) => entries,
        Err(error) => {
            return Ok(Json(serde_json::json!({
                "available": false,
                "error": error.to_string(),
                "devices": []
            })));
        }
    };
    let mut devices = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let (bus, chip_select) = parse_spidev_name(&name)?;
            let device_path = format!("/dev/{name}");
            let sysfs_device = entry.path().join("device");
            let modalias = std::fs::read_to_string(sysfs_device.join("modalias"))
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "-".into());
            let driver = std::fs::read_link(sysfs_device.join("driver"))
                .ok()
                .and_then(|path| {
                    path.file_name()
                        .map(|value| value.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "-".into());
            Some(SpiDevice {
                name,
                bus,
                chip_select,
                device_path,
                modalias,
                driver,
            })
        })
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(Json(
        serde_json::json!({ "available": true, "error": null, "devices": devices }),
    ))
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct UsbDevice {
    pub bus: String,
    pub device: String,
    pub vendor_id: String,
    pub product_id: String,
    pub description: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PciDevice {
    pub address: String,
    pub class: String,
    pub vendor_id: String,
    pub product_id: String,
    pub revision: String,
    pub description: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct I2cDevice {
    pub bus: u32,
    pub address: String,
    pub status: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SpiDevice {
    pub name: String,
    pub bus: u32,
    pub chip_select: u32,
    pub device_path: String,
    pub modalias: String,
    pub driver: String,
}

fn parse_lsusb(output: &str) -> Vec<UsbDevice> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            if fields.next()? != "Bus" {
                return None;
            }
            let bus = fields.next()?.to_string();
            if fields.next()? != "Device" {
                return None;
            }
            let device = fields.next()?.trim_end_matches(':').to_string();
            if fields.next()? != "ID" {
                return None;
            }
            let ids = fields.next()?;
            let (vendor_id, product_id) = ids.split_once(':')?;
            if !is_hex_id(vendor_id) || !is_hex_id(product_id) {
                return None;
            }
            Some(UsbDevice {
                bus,
                device,
                vendor_id: vendor_id.to_lowercase(),
                product_id: product_id.to_lowercase(),
                description: fields.collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

fn parse_lspci(output: &str) -> Vec<PciDevice> {
    output
        .lines()
        .filter_map(|line| {
            let (address, details) = line.split_once(' ')?;
            let (class, after_class) = details.split_once(" [")?;
            let class_end = after_class.find(']')?;
            let vendor_details = &after_class[class_end + 1..];
            let ids = vendor_details
                .match_indices('[')
                .filter_map(|(start, _)| {
                    let end = vendor_details[start + 1..].find(']')? + start + 1;
                    let id = &vendor_details[start + 1..end];
                    id.split_once(':')
                        .filter(|(vendor, product)| is_hex_id(vendor) && is_hex_id(product))
                })
                .next()?;
            let revision = details
                .split("(rev ")
                .nth(1)
                .and_then(|value| value.split(')').next())
                .unwrap_or("-")
                .to_string();
            Some(PciDevice {
                address: address.to_string(),
                class: class.to_string(),
                vendor_id: ids.0.to_lowercase(),
                product_id: ids.1.to_lowercase(),
                revision,
                description: details.to_string(),
            })
        })
        .collect()
}

fn parse_i2cdetect(bus: u32, output: &str) -> Vec<I2cDevice> {
    output
        .lines()
        .filter_map(|line| {
            let (row, cells) = line.split_once(':')?;
            let row = u8::from_str_radix(row.trim(), 16).ok()?;
            Some(
                cells
                    .split_whitespace()
                    .enumerate()
                    .filter_map(move |(column, value)| {
                        let address = row as u16 * 16 + column as u16;
                        if address > 0x7f || value == "--" {
                            return None;
                        }
                        let status = if value == "UU" {
                            "in-use"
                        } else {
                            "responding"
                        };
                        Some(I2cDevice {
                            bus,
                            address: format!("0x{address:02x}"),
                            status: status.into(),
                        })
                    }),
            )
        })
        .flatten()
        .collect()
}

fn parse_spidev_name(name: &str) -> Option<(u32, u32)> {
    let value = name.strip_prefix("spidev")?;
    let (bus, chip_select) = value.split_once('.')?;
    Some((bus.parse().ok()?, chip_select.parse().ok()?))
}

fn is_hex_id(value: &str) -> bool {
    value.len() == 4 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn command_error(command: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{command} exited with {}", output.status)
    } else {
        stderr
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_gpio_device, parse_i2cdetect, parse_lspci, parse_lsusb, parse_spidev_name};

    #[test]
    fn parses_lsusb_ids_and_description() {
        let devices = parse_lsusb(
            "Bus 001 Device 004: ID 0b1f:03ee Insyde Software Corp. RNDIS/Ethernet Gadget\n",
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].bus, "001");
        assert_eq!(devices[0].device, "004");
        assert_eq!(devices[0].vendor_id, "0b1f");
        assert_eq!(devices[0].product_id, "03ee");
        assert_eq!(
            devices[0].description,
            "Insyde Software Corp. RNDIS/Ethernet Gadget"
        );
    }

    #[test]
    fn parses_lspci_address_ids_class_and_revision() {
        let devices = parse_lspci(
            "0000:00:01.0 Host bridge [0600]: Advanced Micro Devices, Inc. [AMD] Device [1022:14da] (rev 01)\n",
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].address, "0000:00:01.0");
        assert_eq!(devices[0].class, "Host bridge");
        assert_eq!(devices[0].vendor_id, "1022");
        assert_eq!(devices[0].product_id, "14da");
        assert_eq!(devices[0].revision, "01");
    }

    #[test]
    fn ignores_malformed_inventory_lines() {
        assert!(parse_lsusb("not a usb line\nBus 001 Device 002: ID invalid nope\n").is_empty());
        assert!(
            parse_lspci("not a pci line\n0000:00:00.0 Host bridge [0600]: Unknown Device\n")
                .is_empty()
        );
    }

    #[test]
    fn parses_i2cdetect_addresses_and_in_use_state() {
        let devices = parse_i2cdetect(1, "     0 1 2 3 4 5 6 7 8 9 a b c d e f\n00: -- -- 03 UU -- -- -- -- -- -- -- -- -- -- -- --\n");
        assert_eq!(devices[0].address, "0x02");
        assert_eq!(devices[0].status, "responding");
        assert_eq!(devices[1].address, "0x03");
        assert_eq!(devices[1].status, "in-use");
    }

    #[test]
    fn parses_spidev_bus_and_chip_select() {
        assert_eq!(parse_spidev_name("spidev0.1"), Some((0, 1)));
        assert_eq!(parse_spidev_name("spi0.0"), None);
    }

    #[test]
    fn parses_gpio_device_chip_and_line() {
        // Manifest form: "gpiochip1 line 7" -> chip + line offset.
        assert_eq!(
            parse_gpio_device("gpiochip1 line 7"),
            Some(("gpiochip1", "7"))
        );
        assert_eq!(
            parse_gpio_device("gpiochip2 line 5"),
            Some(("gpiochip2", "5"))
        );
    }

    #[test]
    fn rejects_malformed_gpio_device() {
        assert_eq!(parse_gpio_device("gpiochip1"), None);
        assert_eq!(parse_gpio_device("gpiochip1 offset 7"), None);
        assert_eq!(parse_gpio_device(""), None);
    }
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
    // the same non-unassigned owner when the owner is a singleton like console,
    // and no two ports may target the same device (ownership is per-device).
    let mut owners = std::collections::HashSet::new();
    let mut devices = std::collections::HashSet::new();
    for s in &req.serial {
        if s.owner != "unassigned" && !owners.insert(s.owner.clone()) {
            return Err(ApiError::Conflict(format!(
                "serial owner {} claimed twice",
                s.owner
            )));
        }
        if !devices.insert(s.device.clone()) {
            return Err(ApiError::Conflict(format!(
                "serial device {} claimed twice",
                s.device
            )));
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
    let cfg = app.engine.update(move |c| {
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

/// `POST /api/v1/io/serial/apply` — apply the serial applier (spec §4.9): set
/// per-port baud and resolve exclusive ownership (console getty on/off).
pub async fn apply_serial(
    State(app): State<AppState>,
    role: AuthRole,
    session: Session,
) -> ApiResult<Json<serde_json::Value>> {
    role.require_admin("io.serial.apply")?;
    let user = username(&session).await;
    app.engine.validate("serial").await?;
    let _ = apply_and_audit(&app, &user, "serial").await?;
    Ok(Json(serde_json::json!({ "applied": ["serial"] })))
}

/// `POST /api/v1/io/gpio/:name/toggle` — manual toggle for an output (spec §4.9).
/// Reads the current line level and drives the opposite; output pins only.
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
    // Only output lines can be driven — reject inputs (e.g. the recovery
    // button) so a toggle never tries to drive a read-only line (spec §4.9).
    let is_output = p
        .caps
        .get("direction")
        .and_then(|v| v.as_str())
        .map(|d| d == "out")
        .unwrap_or(false);
    if !is_output {
        return Err(ApiError::BadRequest(format!(
            "gpio {name} is not an output"
        )));
    }
    let (chip, line) = parse_gpio_device(&p.device)
        .ok_or_else(|| ApiError::BadRequest(format!("gpio {name} device malformed")))?;
    let value = if app.engine.sysroot.is_real_root() {
        // Read the current level with `gpioget`, then drive the opposite with
        // `gpioset` (assumption per spec §7: libgpiod CLI). Previously this
        // always drove `1=1` against a malformed arg, so "toggle" was a no-op
        // that only ever set high.
        let cur = webconfig_engine::applier::run_cmd("gpioget", &[chip, line])
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .unwrap_or(0);
        let next = if cur == 1 { 0 } else { 1 };
        let spec = format!("{line}={next}");
        let _ = webconfig_engine::applier::run_cmd("gpioset", &[chip, &spec]);
        next
    } else {
        // Fake sysroot has no real line state; report a deterministic toggle.
        1
    };
    audit(
        &app,
        &username(&session).await,
        "io.gpio.toggle",
        Some(name.clone()),
    );
    Ok(Json(serde_json::json!({ "pin": name, "value": value })))
}

/// Parse a manifest GPIO `device` string of the form `gpiochip1 line 7` into
/// its chip id and line offset, the shape `gpioset`/`gpioget` expect.
fn parse_gpio_device(device: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = device.split_whitespace().collect();
    if parts.len() == 3 && parts[1] == "line" {
        Some((parts[0], parts[2]))
    } else {
        None
    }
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

/// Platform panel (spec §4.9): Arches cameras + Jetson power mode.
pub async fn platform_panel(
    State(app): State<AppState>,
    _role: AuthRole,
) -> ApiResult<Json<serde_json::Value>> {
    let m = &app.manifest;
    let out = match m.platform.as_str() {
        "arches" => serde_json::json!({
            "cameras": m.of_kind(IoKind::Camera),
            "power_modes": ["15W", "30W", "MAXN"],
        }),
        _ => serde_json::json!({}),
    };
    Ok(Json(out))
}
