//! Update backend selection (spec §4.4 / §7).
//!
//! Re-exports the engine's narrow `UpdateBackend` trait and picks the real
//! (RAUC) or mock backend based on the sysroot: a tempdir/fake sysroot uses
//! the mock so CI runs with no hardware (spec §6); the real root uses RAUC.

pub use webconfig_engine::update::{
    check_compatibility, BundleManifest, InstallProgress, InstallStage, MockUpdateBackend,
    RaucUpdateBackend, SlotStatus, UpdateBackend, UpdateError,
};

use std::sync::Arc;
use webconfig_engine::Sysroot;

/// Pick the backend: RAUC on the real root, mock on a fake sysroot.
pub fn make_backend(sysroot: &Sysroot) -> Arc<dyn UpdateBackend> {
    if sysroot.is_real_root() {
        Arc::new(RaucUpdateBackend::new())
    } else {
        // Seed the mock with a plausible A/B pair so the Updates page has data.
        Arc::new(MockUpdateBackend::new(vec![
            SlotStatus {
                slot: "A".into(),
                version: "1.0.0".into(),
                running: true,
                confirmed: true,
                state: "good".into(),
            },
            SlotStatus {
                slot: "B".into(),
                version: "0.9.0".into(),
                running: false,
                confirmed: false,
                state: "unknown".into(),
            },
        ]))
    }
}
