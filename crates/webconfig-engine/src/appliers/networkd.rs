//! systemd-networkd applier (spec §4.2 — flagship commit/confirm rollback).
//!
//! Generates `.network` files from the config store under
//! `/etc/systemd/network/<iface>.network` and brings interfaces up. The
//! **commit/confirm** flagship feature: any change that could sever the
//! current connection is applied *provisionally* and returns
//! [`ApplyOutcome::Provisional`]. The HTTP layer starts a tokio timer for
//! `confirm_window_secs` (default 90); if the user does not reconfirm on the
//! new address within the window, the backend automatically rolls back by
//! calling [`Applier::rollback`] (spec §4.2).
//!
//! Pre-apply validation (spec §4.2): gateway inside the interface subnet, no
//! duplicate IPs across interfaces, prefix sanity. Generates networkd files
//! only — Wi-Fi (`iwd`/`wpa_supplicant`), hostapd AP, and nftables firewall
//! are separate appliers.

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Mutex;

use crate::applier::{Applier, ApplyError, ApplyOutcome, FileBackup};
use crate::store::Config;
use crate::sysroot::Sysroot;

pub struct NetworkdApplier {
    /// Per-interface file backups, so rollback restores the prior `.network`
    /// files (and removes ones we created fresh).
    backups: Mutex<Vec<FileBackup>>,
    /// Whether the last apply was provisional (awaiting confirm).
    provisional: Mutex<bool>,
}

impl NetworkdApplier {
    pub fn new() -> Self {
        Self {
            backups: Mutex::new(Vec::new()),
            provisional: Mutex::new(false),
        }
    }

    fn network_dir(sysroot: &Sysroot) -> std::path::PathBuf {
        sysroot.join("/etc/systemd/network")
    }

    fn network_file(sysroot: &Sysroot, iface: &str) -> std::path::PathBuf {
        Self::network_dir(sysroot).join(format!("10-{iface}.network"))
    }

    /// Render a networkd `.network` unit for one interface (spec §4.2).
    fn render(iface: &crate::store::network::Interface) -> String {
        let mut s = String::new();
        s.push_str("[Match]\n");
        s.push_str(&format!("Name={}\n\n", iface.name));
        s.push_str("[Link]\n");
        s.push_str(&format!(
            "RequiredForOnline={}\n",
            if iface.enabled { "yes" } else { "no" }
        ));
        if let Some(mtu) = iface.mtu {
            s.push_str(&format!("MTU={mtu}\n"));
        }
        s.push('\n');
        s.push_str("[Network]\n");
        if iface.dhcp {
            s.push_str("DHCP=ipv4\n");
        } else if let (Some(addr), Some(prefix)) = (&iface.address, iface.prefix) {
            s.push_str(&format!("Address={addr}/{prefix}\n"));
        }
        if let Some(gw) = &iface.gateway {
            s.push_str(&format!("Gateway={gw}\n"));
        }
        for dns in &iface.dns {
            s.push_str(&format!("DNS={dns}\n"));
        }
        s
    }

    /// Pre-apply validation (spec §4.2): gateway inside subnet, no dup IPs.
    fn validate_interfaces(
        sysroot: &Sysroot,
        interfaces: &[crate::store::network::Interface],
    ) -> Result<(), ApplyError> {
        let mut seen_ips: HashSet<String> = HashSet::new();
        for iface in interfaces {
            if iface.name.is_empty() {
                return Err(ApplyError::Validation {
                    stage: crate::applier::ApplyStage::Validate,
                    message: "interface with empty name".into(),
                });
            }
            if !iface.dhcp {
                let Some(addr_s) = &iface.address else {
                    return Err(ApplyError::Validation {
                        stage: crate::applier::ApplyStage::Validate,
                        message: format!("static iface {} missing address", iface.name),
                    });
                };
                let Some(prefix) = iface.prefix else {
                    return Err(ApplyError::Validation {
                        stage: crate::applier::ApplyStage::Validate,
                        message: format!("static iface {} missing prefix", iface.name),
                    });
                };
                if prefix > 32 {
                    return Err(ApplyError::Validation {
                        stage: crate::applier::ApplyStage::Validate,
                        message: format!("iface {} prefix > 32", iface.name),
                    });
                }
                let Ok(addr) = addr_s.parse::<IpAddr>() else {
                    return Err(ApplyError::Validation {
                        stage: crate::applier::ApplyStage::Validate,
                        message: format!("iface {} bad address {addr_s}", iface.name),
                    });
                };
                if !seen_ips.insert(addr_s.clone()) {
                    return Err(ApplyError::Validation {
                        stage: crate::applier::ApplyStage::Validate,
                        message: format!("duplicate IP {addr_s} on iface {}", iface.name),
                    });
                }
                if let Some(gw) = &iface.gateway {
                    let Ok(g) = gw.parse::<IpAddr>() else {
                        return Err(ApplyError::Validation {
                            stage: crate::applier::ApplyStage::Validate,
                            message: format!("iface {} bad gateway {gw}", iface.name),
                        });
                    };
                    if !ip_in_subnet(addr, prefix, g) {
                        return Err(ApplyError::Validation {
                            stage: crate::applier::ApplyStage::Validate,
                            message: format!(
                                "iface {}: gateway {gw} not in subnet {addr}/{prefix}",
                                iface.name
                            ),
                        });
                    }
                }
            }
        }
        let _ = sysroot;
        Ok(())
    }
}

impl Default for NetworkdApplier {
    fn default() -> Self {
        Self::new()
    }
}

impl Applier for NetworkdApplier {
    fn id(&self) -> &'static str {
        "networkd"
    }

    fn validate(&self, sysroot: &Sysroot, config: &Config) -> Result<(), ApplyError> {
        Self::validate_interfaces(sysroot, &config.network.interfaces)
    }

    fn apply(&self, sysroot: &Sysroot, config: &Config) -> Result<ApplyOutcome, ApplyError> {
        Self::validate_interfaces(sysroot, &config.network.interfaces)?;

        let dir = Self::network_dir(sysroot);
        std::fs::create_dir_all(&dir)?;

        let mut backups = self.backups.lock().unwrap();
        backups.clear();
        for iface in &config.network.interfaces {
            let path = Self::network_file(sysroot, &iface.name);
            let backup = FileBackup::capture(path.clone())?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, Self::render(iface))?;
            backups.push(backup);
        }

        if sysroot.is_real_root() {
            // Real system verbs. `networkctl reload` picks up the new units.
            crate::applier::run_cmd("networkctl", &["reload"])?;
        }

        // Heuristic: if any *enabled static* interface is being changed, the
        // change could sever the current session, so require commit/confirm
        // (spec §4.2 flagship feature). DHCP-only changes are non-severing.
        let severing = config
            .network
            .interfaces
            .iter()
            .any(|i| i.enabled && !i.dhcp);

        if severing {
            *self.provisional.lock().unwrap() = true;
            Ok(ApplyOutcome::Provisional {
                confirm_window_secs: config.network.confirm_window_secs,
            })
        } else {
            *self.provisional.lock().unwrap() = false;
            Ok(ApplyOutcome::Committed)
        }
    }

    fn rollback(&self, sysroot: &Sysroot) -> Result<(), ApplyError> {
        let mut backups = self.backups.lock().unwrap();
        for b in backups.drain(..) {
            b.restore()?;
        }
        *self.provisional.lock().unwrap() = false;
        if sysroot.is_real_root() {
            crate::applier::run_cmd("networkctl", &["reload"])?;
        }
        Ok(())
    }
}

/// True if `gateway` is inside the `addr/`prefix` subnet. Handles both v4 and
/// v6 via the generic IpAddr representation.
fn ip_in_subnet(addr: IpAddr, prefix: u8, gateway: IpAddr) -> bool {
    match (addr, gateway) {
        (IpAddr::V4(a), IpAddr::V4(g)) => v4_in_subnet(a, prefix, g),
        (IpAddr::V6(a), IpAddr::V6(g)) => v6_in_subnet(a, prefix, g),
        _ => false, // mixed families — never in subnet
    }
}

fn v4_in_subnet(addr: std::net::Ipv4Addr, prefix: u8, gateway: std::net::Ipv4Addr) -> bool {
    if prefix > 32 {
        return false;
    }
    let mask: u32 = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    let a = u32::from(addr) & mask;
    let g = u32::from(gateway) & mask;
    a == g
}

fn v6_in_subnet(addr: std::net::Ipv6Addr, prefix: u8, gateway: std::net::Ipv6Addr) -> bool {
    if prefix > 128 {
        return false;
    }
    let a = addr.octets();
    let g = gateway.octets();
    let full = (prefix / 8) as usize;
    if a[..full] != g[..full] {
        return false;
    }
    let rem = prefix % 8;
    if rem == 0 {
        return true;
    }
    let mask = 0xFFu8 << (8 - rem);
    (a[full] & mask) == (g[full] & mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::network::Interface;

    fn iface(
        name: &str,
        dhcp: bool,
        addr: Option<&str>,
        prefix: Option<u8>,
        gw: Option<&str>,
    ) -> Interface {
        Interface {
            name: name.into(),
            enabled: true,
            dhcp,
            address: addr.map(Into::into),
            prefix,
            gateway: gw.map(Into::into),
            mtu: None,
            dns: Vec::new(),
            open_services: Vec::new(),
        }
    }

    #[test]
    fn renders_network_unit_for_static_iface() {
        let i = iface(
            "eth0",
            false,
            Some("192.168.1.10"),
            Some(24),
            Some("192.168.1.1"),
        );
        let out = NetworkdApplier::render(&i);
        assert!(out.contains("Name=eth0"));
        assert!(out.contains("Address=192.168.1.10/24"));
        assert!(out.contains("Gateway=192.168.1.1"));
        assert!(out.contains("RequiredForOnline=yes"));
    }

    #[test]
    fn renders_dhcp_unit() {
        let i = iface("eth0", true, None, None, None);
        let out = NetworkdApplier::render(&i);
        assert!(out.contains("DHCP=ipv4"));
    }

    #[test]
    fn rejects_gateway_outside_subnet() {
        let sys = Sysroot::at("/tmp/never");
        let a = NetworkdApplier::new();
        let mut c = Config::default();
        c.network.interfaces.push(iface(
            "eth0",
            false,
            Some("192.168.1.10"),
            Some(24),
            Some("10.0.0.1"),
        ));
        let err = a.validate(&sys, &c).unwrap_err();
        assert!(err.to_string().contains("not in subnet"));
    }

    #[test]
    fn rejects_duplicate_ips() {
        let sys = Sysroot::at("/tmp/never");
        let a = NetworkdApplier::new();
        let mut c = Config::default();
        c.network
            .interfaces
            .push(iface("eth0", false, Some("192.168.1.10"), Some(24), None));
        c.network
            .interfaces
            .push(iface("eth1", false, Some("192.168.1.10"), Some(24), None));
        let err = a.validate(&sys, &c).unwrap_err();
        assert!(err.to_string().contains("duplicate IP"));
    }

    #[test]
    fn static_apply_is_provisional_and_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let sys = Sysroot::at(dir.path());
        let a = NetworkdApplier::new();
        let mut c = Config::default();
        c.network.confirm_window_secs = 30;
        c.network.interfaces.push(iface(
            "eth0",
            false,
            Some("192.168.1.10"),
            Some(24),
            Some("192.168.1.1"),
        ));
        let outcome = a.apply(&sys, &c).unwrap();
        assert!(matches!(
            outcome,
            ApplyOutcome::Provisional {
                confirm_window_secs: 30
            }
        ));
        let f = sys.join("/etc/systemd/network/10-eth0.network");
        assert!(f.exists());

        a.rollback(&sys).unwrap();
        assert!(!f.exists(), "rollback should remove the newly-created file");
    }

    #[test]
    fn dhcp_apply_is_committed() {
        let dir = tempfile::tempdir().unwrap();
        let sys = Sysroot::at(dir.path());
        let a = NetworkdApplier::new();
        let mut c = Config::default();
        c.network
            .interfaces
            .push(iface("eth0", true, None, None, None));
        let outcome = a.apply(&sys, &c).unwrap();
        assert!(matches!(outcome, ApplyOutcome::Committed));
    }

    #[test]
    fn v4_subnet_check_examples() {
        use std::net::Ipv4Addr;
        assert!(v4_in_subnet(
            "192.168.1.10".parse().unwrap(),
            24,
            "192.168.1.1".parse::<Ipv4Addr>().unwrap()
        ));
        assert!(!v4_in_subnet(
            "192.168.1.10".parse().unwrap(),
            24,
            "192.168.2.1".parse::<Ipv4Addr>().unwrap()
        ));
        assert!(v4_in_subnet(
            "10.0.0.1".parse().unwrap(),
            8,
            "10.255.255.254".parse::<Ipv4Addr>().unwrap()
        ));
    }

    #[test]
    fn v6_subnet_check_examples() {
        use std::net::Ipv6Addr;
        let a: Ipv6Addr = "fd00::1".parse().unwrap();
        let g: Ipv6Addr = "fd00::fffe".parse().unwrap();
        assert!(v6_in_subnet(a, 64, g));
        let g2: Ipv6Addr = "fd01::1".parse().unwrap();
        assert!(!v6_in_subnet(a, 64, g2));
    }
}
