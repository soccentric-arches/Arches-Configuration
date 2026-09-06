//! Update backend selection (spec §4.4 / §7).
//!
//! Re-exports the engine's narrow `UpdateBackend` trait and picks the real
//! (RAUC) or mock backend based on the sysroot: a tempdir/fake sysroot uses
//! the mock so CI runs with no hardware (spec §6); the real root uses RAUC.

pub use webconfig_engine::update::{
    MockUpdateBackend, RaucUpdateBackend, SlotStatus, UnavailableUpdateBackend, UpdateBackend,
};

use std::process::Command;
use std::sync::Arc;
use webconfig_engine::Sysroot;

/// Pick the backend: RAUC on a real root when the RAUC executable is present,
/// mock on a fake sysroot (dev/CI), and an explicitly *unavailable* backend on
/// a real device with no update engine.
///
/// The third case used to be the second case. A real root without rauc fell
/// through to the mock, seeded with a plausible Slot A/B pair, behind a
/// `tracing::warn!`. The Updates page then reported slot status that was
/// invented and installs that succeeded without flashing anything. That is
/// live on every Arches image: RAUC is in no recipe, layer or package in
/// Arches-OS. See Task.md B-04.
pub fn make_backend(sysroot: &Sysroot) -> Arc<dyn UpdateBackend> {
    let real_root = sysroot.is_real_root();
    let rauc_ok = rauc_available();
    if real_root && rauc_ok {
        return Arc::new(RaucUpdateBackend::new());
    }
    if is_degraded_fallback(real_root, rauc_ok) {
        // A real device with no update engine. Fail loudly on every update
        // operation instead of fabricating state: "the operator believes the
        // device updated" is the failure being prevented, and a journal
        // warning does not prevent it.
        tracing::error!(
            "no OS update backend on this device: rauc is not available on a real root. \
             Update routes will return an explicit error; nothing will be flashed."
        );
        return Arc::new(UnavailableUpdateBackend::new(
            "the RAUC update engine is not installed",
        ));
    }
    // Fake sysroot: the intentional dev/CI case (spec §6). Seed the mock with
    // a plausible A/B pair so the Updates page has data to render.
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

/// Whether falling back to the mock backend is an *unexpected, degraded*
/// state (real hardware with no rauc on PATH) versus the *intentional*
/// dev/CI fake-sysroot case. Split out as a pure predicate so the decision is
/// unit-testable without depending on the actual filesystem root or a real
/// `rauc` binary being present.
fn is_degraded_fallback(is_real_root: bool, rauc_available: bool) -> bool {
    is_real_root && !rauc_available
}

/// Whether `rauc --version` actually succeeds.
///
/// `output().is_ok()` alone is not enough: it reports success whenever the
/// process could be *spawned*, including when rauc exits non-zero because it
/// is broken or misconfigured. A backend that cannot report its own version is
/// not one to hand an OS update to, so the exit status is checked too.
fn rauc_available() -> bool {
    Command::new("rauc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real device with no update engine must produce a backend that fails
    /// loudly, not one that fabricates slot data (Task.md B-04).
    #[test]
    fn unavailable_backend_errors_on_every_operation() {
        let b = UnavailableUpdateBackend::new("the RAUC update engine is not installed");

        // slots() must be an error, not an empty list: an empty list renders
        // as "no slots" and reads like a device without A/B, which is a
        // different and survivable condition.
        assert!(b.slots().is_err());
        assert!(b.verify_bundle("/tmp/whatever.raucb").is_err());
        assert!(b.install("/tmp/whatever.raucb", &mut |_| {}).is_err());
        assert!(b.reboot_to_apply().is_err());
        assert!(b.mark_good().is_err());
    }

    /// The operator has to be able to tell why, from the message alone.
    #[test]
    fn unavailable_backend_explains_itself() {
        let b = UnavailableUpdateBackend::new("the RAUC update engine is not installed");
        let msg = b.slots().unwrap_err().to_string();
        assert!(msg.contains("no OS update backend"), "{msg}");
        assert!(msg.contains("RAUC update engine is not installed"), "{msg}");
        // Must not read as transient; this is a provisioning fault and
        // retrying will never fix it.
        assert!(msg.contains("provisioning fault"), "{msg}");
    }

    #[test]
    fn degraded_fallback_only_when_real_root_without_rauc() {
        // Checkpoint 1.5: the mock fallback is only "degraded" (worth a
        // warning) on real hardware with rauc missing — never on a fake
        // sysroot (dev/CI, spec §6), and never when rauc is actually there.
        assert!(is_degraded_fallback(true, false));
        assert!(!is_degraded_fallback(true, true));
        assert!(!is_degraded_fallback(false, false));
        assert!(!is_degraded_fallback(false, true));
    }
}
