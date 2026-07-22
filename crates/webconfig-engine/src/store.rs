//! Canonical config store (spec §3.2).
//!
//! A single versioned JSON tree persisted at `<data>/webconfig/config.json`
//! with atomic write (temp file + `rename`) and a backup copy
//! `config.json.bak`. All mutations go through [`ConfigStore::commit`], which
//! validates against the loaded [`Schema`](crate::schema::Schema) before
//! writing, so the on-disk tree is always schema-valid.
//!
//! The store is synchronous and blocking-friendly: the HTTP layer runs it on a
//! blocking task (spec §3, applier trait). It performs no network or D-Bus I/O.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::schema::Schema;
use crate::sysroot::Sysroot;
use crate::CONFIG_SCHEMA_VERSION;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("config file not found at {0}")]
    NotFound(PathBuf),
    #[error("config parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("schema validation failed: {0}")]
    Validation(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("schema version mismatch: file={file} engine={engine}")]
    Version { file: u32, engine: u32 },
    #[error("schema error: {0}")]
    Schema(#[from] crate::schema::SchemaError),
}

/// The persisted wrapper: schema version + the tree itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub schema_version: u32,
    #[serde(default)]
    pub config: Config,
}

/// The root config tree. Sections mirror the nine functional areas (spec §4).
/// Schema-driven app settings and alert packs live as `serde_json::Value` so
/// new settings can be dropped in as schema files with zero code change
/// (spec §3, schema-driven rendering everywhere).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub system: system::SystemConfig,
    #[serde(default)]
    pub network: network::NetworkConfig,
    #[serde(default)]
    pub services: services::ServicesConfig,
    #[serde(default)]
    pub updates: updates::UpdatesConfig,
    #[serde(default)]
    pub monitoring: monitoring::MonitoringConfig,
    #[serde(default)]
    pub logs: logs::LogsConfig,
    #[serde(default)]
    pub alerts: alerts::AlertsConfig,
    #[serde(default)]
    pub security: security::SecurityConfig,
    #[serde(default)]
    pub io: io::IoConfig,
    /// Schema-driven application settings keyed by app id. The shape of each
    /// value is governed by a schema file in `/usr/share/webconfig/schemas/`.
    #[serde(default)]
    pub app_settings: serde_json::Map<String, serde_json::Value>,
}

impl ConfigFile {
    /// A fresh default config for first boot.
    pub fn new() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            config: Config::default(),
        }
    }
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self::new()
    }
}

/// The canonical config store: load / commit / backup, schema-validated.
#[derive(Debug)]
pub struct ConfigStore {
    sysroot: Sysroot,
    schema: Schema,
}

impl ConfigStore {
    /// Open the store for `sysroot`, validating commits against `schema`.
    pub fn new(sysroot: Sysroot, schema: Schema) -> Self {
        Self { sysroot, schema }
    }

    fn path(&self) -> PathBuf {
        self.sysroot.webconfig_dir().join("config.json")
    }

    fn backup_path(&self) -> PathBuf {
        self.sysroot.webconfig_dir().join("config.json.bak")
    }

    /// Load the config tree, or return a fresh default on first boot.
    /// A backup of the previous file is *not* touched on load.
    pub fn load(&self) -> Result<ConfigFile, StoreError> {
        let path = self.path();
        if !path.exists() {
            tracing::info!("no config.json present — seeding defaults (first boot)");
            return Ok(ConfigFile::new());
        }
        let raw = fs::read_to_string(&path)?;
        let file: ConfigFile = serde_json::from_str(&raw)?;
        if file.schema_version > CONFIG_SCHEMA_VERSION {
            // Newer file than this engine — refuse rather than silently drop.
            return Err(StoreError::Version {
                file: file.schema_version,
                engine: CONFIG_SCHEMA_VERSION,
            });
        }
        // Older file than engine: restore tolerates version skew by keeping
        // unknown fields (serde default) — no migration needed for v1.
        Ok(file)
    }

    /// Validate `file` against the schema, then atomically write it to disk
    /// and refresh the `.bak` backup. Returns the persisted tree.
    pub fn commit(&self, file: &ConfigFile) -> Result<(), StoreError> {
        self.schema.validate(&file.config)?;

        let dir = self.sysroot.webconfig_dir();
        fs::create_dir_all(&dir)?;

        let raw = serde_json::to_vec_pretty(file)?;
        // Atomic write: write to a temp sibling, fsync, then rename over the
        // real path so a crash mid-write never leaves a truncated config.
        let tmp = dir.join("config.json.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&raw)?;
            f.sync_all()?;
        }
        // Backup the existing file (if any) before clobbering it.
        let path = self.path();
        if path.exists() {
            let _ = fs::copy(&path, self.backup_path());
        }
        fs::rename(&tmp, &path)?;
        tracing::info!("config committed ({} bytes)", raw.len());
        Ok(())
    }

    /// Load → mutate → commit in one call. The closure receives a mutable
    /// config tree; if it returns `Ok`, the result is validated and persisted.
    pub fn update<F>(&self, f: F) -> Result<ConfigFile, StoreError>
    where
        F: FnOnce(&mut Config) -> Result<(), StoreError>,
    {
        let mut file = self.load()?;
        f(&mut file.config)?;
        self.commit(&file)?;
        Ok(file)
    }

    /// Path on disk (for diagnostics / backup download).
    pub fn file_path(&self) -> PathBuf {
        self.path()
    }
}

// ---------------------------------------------------------------------------
// Section models. Kept small and typed; schema-driven app settings live as
// `serde_json::Value` above. Adding a new *typed* field is a schema-version
// bump; adding a new schema-driven app setting is not.
// ---------------------------------------------------------------------------

pub mod system {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct SystemConfig {
        #[serde(default)]
        pub hostname: Option<String>,
        /// Free-text asset tag / site label — config store only, never written
        /// to the OS hostname (spec §4.1).
        #[serde(default)]
        pub asset_tag: Option<String>,
        #[serde(default)]
        pub time: TimeConfig,
        #[serde(default)]
        pub ssh: SshConfig,
        #[serde(default)]
        pub ntp_servers: Vec<String>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct TimeConfig {
        #[serde(default)]
        pub timezone: Option<String>,
        #[serde(default)]
        pub ntp_enabled: bool,
        /// Manual date/time for air-gapped sites (ISO-8601 local). Applied via
        /// `timedatectl set-time` (spec §4.1).
        #[serde(default)]
        pub manual_datetime: Option<String>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct SshConfig {
        #[serde(default)]
        pub sshd_enabled: bool,
        #[serde(default)]
        pub serial_getty_enabled: bool,
        #[serde(default)]
        pub authorized_keys: String,
    }
}

pub mod network {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct NetworkConfig {
        #[serde(default)]
        pub interfaces: Vec<Interface>,
        #[serde(default)]
        pub wifi: Vec<WifiNetwork>,
        #[serde(default)]
        pub regulatory_domain: Option<String>,
        #[serde(default)]
        pub dns_servers: Vec<String>,
        #[serde(default)]
        pub firewall: FirewallConfig,
        /// Commit/confirm rollback window in seconds (spec §4.2 flagship
        /// feature). Default 90.
        #[serde(default = "default_confirm_window")]
        pub confirm_window_secs: u64,
        /// AP provisioning mode trigger (spec §4.2).
        #[serde(default)]
        pub ap_mode: ApMode,
        /// [v2] schema slots — designed now, not implemented (spec §4.2).
        #[serde(default)]
        pub vlan: Vec<serde_json::Value>,
        #[serde(default)]
        pub bridges: Vec<serde_json::Value>,
        #[serde(default)]
        pub cellular: Vec<serde_json::Value>,
    }

    fn default_confirm_window() -> u64 {
        90
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct Interface {
        pub name: String,
        #[serde(default)]
        pub enabled: bool,
        #[serde(default = "default_dhcp")]
        pub dhcp: bool,
        #[serde(default)]
        pub address: Option<String>,
        #[serde(default)]
        pub prefix: Option<u8>,
        #[serde(default)]
        pub gateway: Option<String>,
        #[serde(default)]
        pub mtu: Option<u32>,
        /// Per-interface DNS (spec §4.2). Empty = use global list.
        #[serde(default)]
        pub dns: Vec<String>,
        /// Firewall services open on this interface (spec §4.2 service-based
        /// firewall). Values are service ids from the allowlist.
        #[serde(default)]
        pub open_services: Vec<String>,
    }

    fn default_dhcp() -> bool {
        true
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct WifiNetwork {
        pub ssid: String,
        #[serde(default)]
        pub psk: Option<String>,
        #[serde(default)]
        pub priority: i32,
        #[serde(default)]
        pub hidden: bool,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct FirewallConfig {
        /// Services globally open regardless of interface binding.
        #[serde(default)]
        pub default_open: Vec<String>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct ApMode {
        #[serde(default)]
        pub enabled: bool,
        /// `no-config` or `gpio-button` (spec §4.2).
        #[serde(default = "default_ap_trigger")]
        pub trigger: String,
        #[serde(default = "default_ap_ssid")]
        pub ssid: String,
        #[serde(default = "default_ap_passphrase")]
        pub passphrase: String,
    }

    fn default_ap_trigger() -> String {
        "no-config".to_string()
    }
    fn default_ap_ssid() -> String {
        "SoCcentric-Setup".to_string()
    }
    fn default_ap_passphrase() -> String {
        // AP must be open-ish but spec mandates WPA; installer sets a real one.
        "change-me-now".to_string()
    }
}

pub mod services {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct ServicesConfig {
        #[serde(default)]
        pub mqtt: MqttConfig,
        /// Per-service log level overrides (keyed by unit name) where the
        /// service schema declares one (spec §4.3 / §4.6).
        #[serde(default)]
        pub log_levels: std::collections::BTreeMap<String, String>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct MqttConfig {
        #[serde(default)]
        pub host: Option<String>,
        #[serde(default)]
        pub port: Option<u16>,
        #[serde(default)]
        pub tls: bool,
        #[serde(default)]
        pub username: Option<String>,
        #[serde(default)]
        pub password: Option<String>,
        /// Client cert id from the Security cert store, or None.
        #[serde(default)]
        pub client_cert_id: Option<String>,
        #[serde(default)]
        pub client_id: Option<String>,
        #[serde(default)]
        pub base_topic: Option<String>,
    }
}

pub mod updates {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct UpdatesConfig {
        #[serde(default)]
        pub ota: OtaConfig,
        /// Whether HTTP→HTTPS redirect is on (mirrors security; kept here for
        /// the Updates page convenience).
        #[serde(default)]
        pub https_redirect: bool,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct OtaConfig {
        #[serde(default)]
        pub server_url: Option<String>,
        /// v1: notify-only (spec §4.4). [v2] auto-apply + maintenance windows.
        #[serde(default)]
        pub auto_apply: bool,
        #[serde(default)]
        pub maintenance_window: Option<serde_json::Value>,
    }
}

pub mod monitoring {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct MonitoringConfig {
        /// Dashboard poll interval in seconds while the page is open (spec §4.5).
        #[serde(default = "default_poll_interval_secs")]
        pub poll_interval_secs: u64,
    }

    fn default_poll_interval_secs() -> u64 {
        3
    }
}

pub mod logs {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct LogsConfig {
        /// `volatile` (default) or `persistent` (spec §4.6).
        #[serde(default = "default_storage")]
        pub storage: String,
        /// Max journal size in bytes when persistent (eMMC wear, spec §4.6).
        #[serde(default = "default_max_size")]
        pub max_size_bytes: u64,
        #[serde(default)]
        pub remote_syslog: Option<RemoteSyslog>,
    }

    fn default_storage() -> String {
        "volatile".to_string()
    }
    fn default_max_size() -> u64 {
        64 * 1024 * 1024
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct RemoteSyslog {
        pub host: String,
        pub port: u16,
        /// `udp` or `tcp` (spec §4.6 — one destination).
        pub protocol: String,
    }
}

pub mod alerts {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct AlertsConfig {
        #[serde(default)]
        pub rules: Vec<AlertRule>,
        /// Webhook POST targets (spec §4.7).
        #[serde(default)]
        pub webhooks: Vec<String>,
        /// MQTT event topic base (spec §4.7).
        #[serde(default)]
        pub mqtt_event_topic: Option<String>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct AlertRule {
        pub id: String,
        pub metric: String,
        /// `>`, `<`, `==`, `!=`.
        pub comparison: String,
        pub threshold: f64,
        /// Duration in seconds the condition must hold to fire (anti-flap,
        /// spec §4.7).
        #[serde(default)]
        pub duration_secs: u64,
        /// `webhook`, `mqtt`, `ui` (always UI + event log).
        #[serde(default)]
        pub actions: Vec<String>,
    }
}

pub mod security {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct SecurityConfig {
        #[serde(default = "default_idle")]
        pub session_idle_timeout_secs: u64,
        /// Plain HTTP admin opt-in (spec §4.8; default off).
        #[serde(default)]
        pub allow_plain_http: bool,
        #[serde(default)]
        pub wireguard: WireGuardConfig,
        /// Cert ids present in the device cert store (metadata only here;
        /// the certs themselves live on the data partition).
        #[serde(default)]
        pub certificates: Vec<CertMeta>,
    }

    fn default_idle() -> u64 {
        900
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct WireGuardConfig {
        #[serde(default)]
        pub enabled: bool,
        /// Listen port.
        #[serde(default)]
        pub port: u16,
        /// Peers (spec §4.8).
        #[serde(default)]
        pub peers: Vec<WgPeer>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct WgPeer {
        pub public_key: String,
        #[serde(default)]
        pub allowed_ips: String,
        #[serde(default)]
        pub endpoint: Option<String>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct CertMeta {
        pub id: String,
        pub kind: String, // "ca" | "client"
        pub subject: String,
        pub not_after: String, // ISO-8601
    }

    // Re-impl default idle timeout at the section level via a helper, since
    // serde `default=` can't call the function above without it being in scope
    // for the field. We expose a typed accessor instead.
    impl SecurityConfig {
        pub fn idle_timeout_secs(&self) -> u64 {
            if self.session_idle_timeout_secs == 0 {
                default_idle()
            } else {
                self.session_idle_timeout_secs
            }
        }
    }
}

pub mod io {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct IoConfig {
        #[serde(default)]
        pub serial: Vec<SerialPort>,
        #[serde(default)]
        pub can: Vec<CanInterface>,
        #[serde(default)]
        pub gpio: Vec<GpioPin>,
        #[serde(default)]
        pub mcu: Option<McuConfig>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct SerialPort {
        pub device: String,
        #[serde(default)]
        pub baud: u32,
        #[serde(default)]
        pub mode: String, // "rs232" | "rs485"
        /// Ownership assignment (spec §4.9): console | customer | modbus | unassigned.
        #[serde(default = "default_owner")]
        pub owner: String,
    }

    fn default_owner() -> String {
        "unassigned".to_string()
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct CanInterface {
        pub name: String,
        #[serde(default)]
        pub bitrate: u32,
        #[serde(default)]
        pub fd: bool,
        #[serde(default)]
        pub data_bitrate: u32,
        #[serde(default)]
        pub listen_only: bool,
        #[serde(default)]
        pub restart_ms: u32,
        #[serde(default)]
        pub up_on_boot: bool,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct GpioPin {
        pub name: String,
        #[serde(default)]
        pub direction: String, // "in" | "out"
        #[serde(default)]
        pub active_low: bool,
        #[serde(default)]
        pub initial_state: Option<bool>,
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct McuConfig {
        pub kind: String, // "stm32" | "picow"
        #[serde(default)]
        pub firmware_version: Option<String>,
        #[serde(default)]
        pub settings: serde_json::Map<String, serde_json::Value>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Schema;

    #[test]
    fn commit_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let sys = Sysroot::at(dir.path());
        let schema = Schema::default_permissive();
        let store = ConfigStore::new(sys, schema);

        let mut file = store.load().unwrap();
        file.config.system.hostname = Some("arches-01".into());
        store.commit(&file).unwrap();

        let reloaded = ConfigStore::new(Sysroot::at(dir.path()), Schema::default_permissive())
            .load()
            .unwrap();
        assert_eq!(
            reloaded.config.system.hostname.as_deref(),
            Some("arches-01")
        );
    }

    #[test]
    fn commit_is_atomic_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let sys = Sysroot::at(dir.path());
        let store = ConfigStore::new(sys, Schema::default_permissive());

        let mut file = store.load().unwrap();
        file.config.system.hostname = Some("first".into());
        store.commit(&file).unwrap();

        file.config.system.hostname = Some("second".into());
        store.commit(&file).unwrap();

        // Backup should hold the previous ("first") value.
        let bak =
            std::fs::read_to_string(dir.path().join("data/webconfig/config.json.bak")).unwrap();
        assert!(bak.contains("first"));
        let cur = std::fs::read_to_string(dir.path().join("data/webconfig/config.json")).unwrap();
        assert!(cur.contains("second"));
        // No leftover tmp file.
        assert!(!dir.path().join("data/webconfig/config.json.tmp").exists());
    }

    #[test]
    fn update_helper_validates_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(Sysroot::at(dir.path()), Schema::default_permissive());
        let out = store
            .update(|c| {
                c.system.hostname = Some("z".into());
                Ok(())
            })
            .unwrap();
        assert_eq!(out.config.system.hostname.as_deref(), Some("z"));
    }
}
