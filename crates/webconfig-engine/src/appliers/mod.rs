//! Appliers (spec §3, §7.3). One module per system verb.
//!
//! Each applier implements [`crate::applier::Applier`] with the exact
//! `validate / apply / rollback` shape, takes a sysroot parameter, and is
//! testable against a tempdir fake sysroot with no hardware. Real system
//! interactions that cannot be fully implemented go through a narrow trait
//! with a real implementation **and** a mock — never a silent TODO (spec §7).

pub mod can;
pub mod firewall;
pub mod hostname;
pub mod journald;
pub mod networkd;
pub mod time;
pub mod wireguard;

pub use can::CanApplier;
pub use firewall::FirewallApplier;
pub use hostname::HostnameApplier;
pub use journald::JournaldApplier;
pub use networkd::NetworkdApplier;
pub use time::TimeApplier;
pub use wireguard::WireGuardApplier;

use crate::applier::Applier;

/// The full ordered applier set for a platform. Order matters only where one
/// applier depends on another's output (e.g. CAN uses interfaces networkd
/// brought up); here each is independent so ordering is by spec §4 section.
pub fn all() -> Vec<Box<dyn Applier>> {
    vec![
        Box::new(HostnameApplier::new()),
        Box::new(TimeApplier::new()),
        Box::new(NetworkdApplier::new()),
        Box::new(FirewallApplier::new()),
        Box::new(CanApplier::new()),
        Box::new(JournaldApplier::new()),
        Box::new(WireGuardApplier::new()),
    ]
}