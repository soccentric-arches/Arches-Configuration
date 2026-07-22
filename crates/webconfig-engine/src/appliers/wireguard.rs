//! WireGuard applier (spec §4.8 — WireGuard only, no OpenVPN/IPsec).
//!
//! On/off, on-device keypair generation (private key never leaves the device;
//! only the public key is shown), peer list, allowed IPs. Writes
//! `/etc/wireguard/wg0.conf` and brings the interface up/down via `wg-quick`.
//! The private key is generated once and stored under the data partition with
//! restrictive permissions; it is **never** returned by the API (spec §4.8).

use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;

use crate::applier::{run_cmd, Applier, ApplyError, ApplyOutcome, FileBackup};
use crate::store::Config;
use crate::sysroot::Sysroot;

#[derive(Debug)]
pub struct WireGuardApplier {
    backup: Mutex<Option<FileBackup>>,
}

impl WireGuardApplier {
    pub fn new() -> Self {
        Self {
            backup: Mutex::new(None),
        }
    }

    fn conf_path(sysroot: &Sysroot) -> std::path::PathBuf {
        sysroot.join("/etc/wireguard/wg0.conf")
    }

    /// Where the on-device private key lives (data partition, mode 0600).
    fn key_path(sysroot: &Sysroot) -> std::path::PathBuf {
        sysroot.webconfig_dir().join("wireguard.key")
    }

    /// Render a wg-quick config (spec §4.8). The private key is read from the
    /// data partition (generated on first enable); never embedded by callers.
    pub fn render(config: &Config, private_key: &str) -> String {
        let mut s = String::new();
        s.push_str("[Interface]\n");
        s.push_str(&format!("PrivateKey = {private_key}\n"));
        if config.security.wireguard.port > 0 {
            s.push_str(&format!(
                "ListenPort = {}\n",
                config.security.wireguard.port
            ));
        }
        for p in &config.security.wireguard.peers {
            s.push_str("\n[Peer]\n");
            s.push_str(&format!("PublicKey = {}\n", p.public_key));
            s.push_str(&format!("AllowedIPs = {}\n", p.allowed_ips));
            if let Some(ep) = &p.endpoint {
                s.push_str(&format!("Endpoint = {ep}\n"));
            }
        }
        s
    }

    fn validate(config: &Config) -> Result<(), ApplyError> {
        for p in &config.security.wireguard.peers {
            if p.public_key.is_empty() {
                return Err(ApplyError::Validation {
                    stage: crate::applier::ApplyStage::Validate,
                    message: "WireGuard peer with empty public key".into(),
                });
            }
            if p.allowed_ips.is_empty() {
                return Err(ApplyError::Validation {
                    stage: crate::applier::ApplyStage::Validate,
                    message: format!("peer {} allowed_ips empty", p.public_key),
                });
            }
        }
        Ok(())
    }

    /// Ensure a private key exists on the data partition. In a tempdir sysroot
    /// we write a deterministic placeholder (tests don't need real crypto). On
    /// the real root we generate a real key with `wg genkey`.
    fn ensure_key(sysroot: &Sysroot) -> Result<String, ApplyError> {
        let path = Self::key_path(sysroot);
        if path.exists() {
            return Ok(std::fs::read_to_string(&path)?.trim().to_string());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let key = if sysroot.is_real_root() {
            // Real on-device keypair generation (spec §4.8).
            run_cmd("wg", &["genkey"])?.trim().to_string()
        } else {
            // Fake-sysroot test path: a stable placeholder, not a real key.
            "TEST-PRIVATE-KEY-DO-NOT-USE".to_string()
        };
        std::fs::write(&path, &key)?;
        // Private key: readable only by root (spec §4.8 — never leaves device).
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(key)
    }
}

impl Default for WireGuardApplier {
    fn default() -> Self {
        Self::new()
    }
}

impl Applier for WireGuardApplier {
    fn id(&self) -> &'static str {
        "wireguard"
    }

    fn validate(&self, _sysroot: &Sysroot, config: &Config) -> Result<(), ApplyError> {
        Self::validate(config)
    }

    fn apply(&self, sysroot: &Sysroot, config: &Config) -> Result<ApplyOutcome, ApplyError> {
        Self::validate(config)?;
        let path = Self::conf_path(sysroot);
        let prior = FileBackup::capture(path.clone())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if config.security.wireguard.enabled {
            let key = Self::ensure_key(sysroot)?;
            std::fs::write(&path, Self::render(config, &key))?;
            *self.backup.lock().unwrap() = Some(prior);
            if sysroot.is_real_root() {
                run_cmd("wg-quick", &["down", "wg0"]).ok(); // ignore if down
                run_cmd(
                    "wg-quick",
                    &["up", path.to_str().unwrap_or("/etc/wireguard/wg0.conf")],
                )?;
            }
        } else {
            std::fs::write(&path, "# WireGuard disabled by WebConfig\n")?;
            *self.backup.lock().unwrap() = Some(prior);
            if sysroot.is_real_root() {
                run_cmd("wg-quick", &["down", "wg0"]).ok();
            }
        }
        Ok(ApplyOutcome::Committed)
    }

    fn rollback(&self, sysroot: &Sysroot) -> Result<(), ApplyError> {
        if let Some(b) = self.backup.lock().unwrap().take() {
            b.restore()?;
        }
        if sysroot.is_real_root() {
            run_cmd("wg-quick", &["down", "wg0"]).ok();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::security::{WgPeer, WireGuardConfig};

    fn peer(pk: &str, ips: &str) -> WgPeer {
        WgPeer {
            public_key: pk.into(),
            allowed_ips: ips.into(),
            endpoint: None,
        }
    }

    #[test]
    fn renders_interface_and_peers() {
        let mut c = Config::default();
        c.security.wireguard = WireGuardConfig {
            enabled: true,
            port: 51820,
            peers: vec![peer("PUBKEY1", "10.0.0.2/32")],
        };
        let r = WireGuardApplier::render(&c, "SECRET");
        assert!(r.contains("PrivateKey = SECRET"));
        assert!(r.contains("ListenPort = 51820"));
        assert!(r.contains("PublicKey = PUBKEY1"));
        assert!(r.contains("AllowedIPs = 10.0.0.2/32"));
    }

    #[test]
    fn rejects_empty_peer_key() {
        let a = WireGuardApplier::new();
        let sys = Sysroot::at("/tmp/never");
        let mut c = Config::default();
        c.security.wireguard.peers.push(peer("", "10.0.0.2/32"));
        assert!(a.validate(&sys, &c).is_err());
    }

    #[test]
    fn writes_config_and_generates_test_key_in_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let sys = Sysroot::at(dir.path());
        let a = WireGuardApplier::new();
        let mut c = Config::default();
        c.security.wireguard.enabled = true;
        c.security.wireguard.peers.push(peer("PUB1", "10.0.0.2/32"));
        a.apply(&sys, &c).unwrap();
        let conf = std::fs::read_to_string(sys.join("/etc/wireguard/wg0.conf")).unwrap();
        assert!(conf.contains("TEST-PRIVATE-KEY-DO-NOT-USE"));
        let key = std::fs::read_to_string(sys.webconfig_dir().join("wireguard.key")).unwrap();
        assert_eq!(key, "TEST-PRIVATE-KEY-DO-NOT-USE");
    }
}
