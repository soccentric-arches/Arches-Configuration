//! Board manifest (spec §3 platform layer).
//!
//! The board manifest is a BSP-provided, read-only file at
//! `/usr/share/webconfig/board-manifest.json` declaring available I/O
//! (CAN/serial/GPIO/I2C/MCU/FPGA/PRU) with Linux device names and human names.
//! Per-platform dashboard tiles and panels are declared as **data, not code**;
//! the six platforms differ only in this layer (spec §3).
//!
//! I/O rendering is 100% manifest-driven — zero hardcoded I/O (spec §4.9).

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("manifest io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest not found at {0}")]
    NotFound(String),
}

/// Kind of peripheral a manifest entry describes (spec §4.9 / §4.5 tiles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IoKind {
    #[default]
    Serial,
    Can,
    Gpio,
    I2c,
    Spi,
    Mcu,
    Fpga,
    Pru,
    Camera,
    Thermal,
    Net,
}

/// One manifest peripheral entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Peripheral {
    /// Stable id, e.g. "can0", "ttyS2", "gpio-led-status".
    pub id: String,
    /// Linux device name, e.g. "can0", "/dev/ttyS2", "gpiochip3 line 7".
    #[serde(default)]
    pub device: String,
    /// Human-readable name shown in the UI.
    #[serde(default)]
    pub human_name: String,
    pub kind: IoKind,
    /// Capabilities specific to the kind, kept as free-form JSON so new
    /// platform quirks (e.g. GPIO-switchable CAN termination) need no code
    /// change (spec §4.9).
    #[serde(default)]
    pub caps: serde_json::Map<String, serde_json::Value>,
}

/// A dashboard tile declared as data (spec §4.5 platform tiles via manifest
/// plugin). The frontend renders these generically.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DashboardTile {
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// Metric ids this tile pulls from the monitoring endpoint.
    #[serde(default)]
    pub metrics: Vec<String>,
    /// Render hint: "sparkline" | "value" | "state" | "bar".
    #[serde(default)]
    pub render: String,
}

/// The board manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BoardManifest {
    /// Platform id: arches | acadia | zion | pinnacle | joshua | sequoia.
    pub platform: String,
    #[serde(default)]
    pub model: String,
    /// Human-readable board name.
    #[serde(default)]
    pub board_name: String,
    #[serde(default)]
    pub peripherals: Vec<Peripheral>,
    #[serde(default)]
    pub dashboard_tiles: Vec<DashboardTile>,
    /// Curated service allowlist for this platform image (spec §4.3 — never
    /// enumerate all units).
    #[serde(default)]
    pub service_allowlist: Vec<String>,
    /// Platform-specific capability flags read by the trust panel (spec §4.8).
    #[serde(default)]
    pub has_rtc: bool,
    #[serde(default)]
    pub has_tpm: bool,
    #[serde(default)]
    pub secure_boot: bool,
}

impl BoardManifest {
    /// Load from `<share>/board-manifest.json`. Returns `NotFound` if absent.
    pub fn load(share_dir: &Path) -> Result<Self, ManifestError> {
        let path = share_dir.join("board-manifest.json");
        if !path.exists() {
            return Err(ManifestError::NotFound(path.display().to_string()));
        }
        let raw = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Peripherals of a given kind.
    pub fn of_kind(&self, kind: IoKind) -> Vec<&Peripheral> {
        self.peripherals.iter().filter(|p| p.kind == kind).collect()
    }

    /// Whether a peripheral id is declared (used to reject config referencing
    /// hardware the board does not have).
    pub fn has(&self, id: &str) -> bool {
        self.peripherals.iter().any(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_arches_manifest() {
        let raw = include_str!("../../../board-manifests/arches.json");
        let m: BoardManifest = serde_json::from_str(raw).unwrap();
        assert_eq!(m.platform, "arches");
        assert!(m.of_kind(IoKind::Can).iter().any(|p| p.id == "can0"));
        assert!(m.has("mcu-stm32"));
    }

    #[test]
    fn every_platform_manifest_parses() {
        let cases: &[(&str, &str)] = &[
            (
                "arches",
                include_str!("../../../board-manifests/arches.json"),
            ),
            (
                "acadia",
                include_str!("../../../board-manifests/acadia.json"),
            ),
            ("zion", include_str!("../../../board-manifests/zion.json")),
            (
                "pinnacle",
                include_str!("../../../board-manifests/pinnacle.json"),
            ),
            (
                "joshua",
                include_str!("../../../board-manifests/joshua.json"),
            ),
            (
                "sequoia",
                include_str!("../../../board-manifests/sequoia.json"),
            ),
        ];
        for (platform, raw) in cases {
            let m: BoardManifest = serde_json::from_str(raw).unwrap();
            assert_eq!(&m.platform.as_str(), platform, "platform mismatch");
        }
    }
}
