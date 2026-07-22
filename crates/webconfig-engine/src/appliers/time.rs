//! Time / NTP applier (spec §4.1).
//!
//! Applies timezone, NTP on/off, and manual date/time via `timedatectl`:
//!   - `timedatectl set-timezone <tz>`
//!   - `timedatectl set-ntp true|false`
//!   - `timedatectl set-time 'YYYY-MM-DD HH:MM:SS'` (air-gapped sites; only
//!     when NTP is off)
//!
//! After a manual set, where an RTC is present, `timedatectl` syncs the RTC
//! automatically; the manifest `has_rtc` flag drives the UI warning banner
//! when no RTC and NTP is unreachable (spec §4.1).
//!
//! Validation rejects an unknown timezone by checking it exists under
//! `/usr/share/zoneinfo` (real root) or skipping the check in a tempdir
//! sysroot so tests stay hardware-free.

use std::sync::Mutex;

use crate::applier::{run_cmd, Applier, ApplyError, ApplyOutcome};
use crate::store::Config;
use crate::sysroot::Sysroot;

#[derive(Debug)]
pub struct TimeApplier {
    last: Mutex<Option<TimeSnapshot>>,
}

#[derive(Debug, Clone)]
struct TimeSnapshot {
    timezone: Option<String>,
    ntp: Option<bool>,
}

impl TimeApplier {
    pub fn new() -> Self {
        Self {
            last: Mutex::new(None),
        }
    }

    fn zoneinfo(sysroot: &Sysroot, tz: &str) -> std::path::PathBuf {
        sysroot.join(format!("/usr/share/zoneinfo/{tz}"))
    }

    fn validate_timezone(sysroot: &Sysroot, tz: &str) -> Result<(), ApplyError> {
        if tz.is_empty()
            || tz
                .chars()
                .any(|c| !(c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-'))
        {
            return Err(ApplyError::Validation {
                stage: crate::applier::ApplyStage::Validate,
                message: format!("invalid timezone characters: {tz}"),
            });
        }
        // On the real root, confirm tzdata actually ships the zone. In a
        // tempdir sysroot the zone file won't exist, so skip the existence
        // check there (spec §6: no hardware, no tzdata in tempdir).
        if sysroot.is_real_root() && !Self::zoneinfo(sysroot, tz).exists() {
            return Err(ApplyError::Validation {
                stage: crate::applier::ApplyStage::Validate,
                message: format!("unknown timezone: {tz}"),
            });
        }
        Ok(())
    }

    fn parse_manual(s: &str) -> Result<(), ApplyError> {
        // Accept 'YYYY-MM-DD HH:MM:SS'. timedatectl enforces semantics; we
        // only check the shape so we never pass garbage to the verb.
        let stripped = s.trim();
        if stripped.len() < 19
            || !stripped[4..5].contains('-')
            || !stripped[7..8].contains('-')
            || !stripped[10..11].contains(' ')
            || !stripped[13..14].contains(':')
            || !stripped[16..17].contains(':')
        {
            return Err(ApplyError::Validation {
                stage: crate::applier::ApplyStage::Validate,
                message: format!("manual_datetime must be 'YYYY-MM-DD HH:MM:SS', got {s:?}"),
            });
        }
        Ok(())
    }
}

impl Default for TimeApplier {
    fn default() -> Self {
        Self::new()
    }
}

impl Applier for TimeApplier {
    fn id(&self) -> &'static str {
        "time"
    }

    fn validate(&self, sysroot: &Sysroot, config: &Config) -> Result<(), ApplyError> {
        if let Some(tz) = &config.system.time.timezone {
            Self::validate_timezone(sysroot, tz)?;
        }
        if let Some(manual) = &config.system.time.manual_datetime {
            Self::parse_manual(manual)?;
            if config.system.time.ntp_enabled {
                return Err(ApplyError::Validation {
                    stage: crate::applier::ApplyStage::Validate,
                    message: "manual_datetime set while NTP enabled — disable NTP first".into(),
                });
            }
        }
        Ok(())
    }

    fn apply(&self, sysroot: &Sysroot, config: &Config) -> Result<ApplyOutcome, ApplyError> {
        let t = &config.system.time;
        if let Some(tz) = &t.timezone {
            Self::validate_timezone(sysroot, tz)?;
            if sysroot.is_real_root() {
                run_cmd("timedatectl", &["set-timezone", tz])?;
            }
        }
        // Set NTP off before a manual time set, then re-enable as requested.
        if let Some(manual) = &t.manual_datetime {
            Self::parse_manual(manual)?;
            if sysroot.is_real_root() {
                run_cmd("timedatectl", &["set-ntp", "false"])?;
                run_cmd("timedatectl", &["set-time", manual])?;
            }
        }
        if sysroot.is_real_root() {
            run_cmd(
                "timedatectl",
                &["set-ntp", if t.ntp_enabled { "true" } else { "false" }],
            )?;
        }
        *self.last.lock().unwrap() = Some(TimeSnapshot {
            timezone: t.timezone.clone(),
            ntp: Some(t.ntp_enabled),
        });
        Ok(ApplyOutcome::Committed)
    }

    fn rollback(&self, sysroot: &Sysroot) -> Result<(), ApplyError> {
        if let Some(prev) = self.last.lock().unwrap().take() {
            if sysroot.is_real_root() {
                if let Some(tz) = prev.timezone {
                    run_cmd("timedatectl", &["set-timezone", &tz])?;
                }
                if let Some(ntp) = prev.ntp {
                    run_cmd(
                        "timedatectl",
                        &["set-ntp", if ntp { "true" } else { "false" }],
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_timezone_shape() {
        let sys = Sysroot::at("/tmp/never");
        let a = TimeApplier::new();
        let mut c = Config::default();
        c.system.time.timezone = Some("Europe/Berlin".into());
        assert!(a.validate(&sys, &c).is_ok());
        c.system.time.timezone = Some("../etc/passwd".into());
        assert!(a.validate(&sys, &c).is_err());
    }

    #[test]
    fn rejects_manual_time_with_ntp_on() {
        let a = TimeApplier::new();
        let sys = Sysroot::at("/tmp/never");
        let mut c = Config::default();
        c.system.time.ntp_enabled = true;
        c.system.time.manual_datetime = Some("2026-01-02 03:04:05".into());
        assert!(a.validate(&sys, &c).is_err());
    }

    #[test]
    fn accepts_manual_time_with_ntp_off() {
        let a = TimeApplier::new();
        let sys = Sysroot::at("/tmp/never");
        let mut c = Config::default();
        c.system.time.ntp_enabled = false;
        c.system.time.manual_datetime = Some("2026-01-02 03:04:05".into());
        assert!(a.validate(&sys, &c).is_ok());
    }

    #[test]
    fn rejects_malformed_manual_time() {
        let a = TimeApplier::new();
        let sys = Sysroot::at("/tmp/never");
        let mut c = Config::default();
        c.system.time.manual_datetime = Some("not a date".into());
        assert!(a.validate(&sys, &c).is_err());
    }
}
