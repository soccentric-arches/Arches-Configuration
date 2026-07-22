//! Alert engine (spec §4.7).
//!
//! Threshold rules: metric (temp, disk %, memory, service state, MCU
//! heartbeat) + comparison + **duration** ("temp > 85 for 60 s") to prevent
//! flapping. Actions: UI red-state + event log (always); webhook POST; MQTT
//! event topic. [v2] email. Alert history: fired/cleared ring-buffer file with
//! timestamps, viewable + exportable. Per-platform default rule packs ship as
//! schema files (Arches: throttling; all: disk-full, watchdog-reset).
//!
//! The engine evaluates rules on the same data the dashboard reads, on a timer
//! inside the daemon — no extra process (spec §4.7).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::monitoring::Vitals;
use crate::store::alerts::AlertRule;

/// One history entry — fired or cleared.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub ts: String,
    pub rule_id: String,
    pub state: AlertState,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertState {
    Fired,
    Cleared,
}

/// In-memory state for the duration anti-flap logic: the timestamp the
/// condition first became true (None = not currently true).
#[derive(Debug, Clone, Default)]
pub struct RuleState {
    pub held_since: Option<u64>,
    pub firing: bool,
}

/// The alert engine. `evaluate` is called on each timer tick with the current
/// vitals and the current monotonic-ish timestamp (seconds, supplied by the
/// caller so the engine stays deterministic and testable).
#[derive(Debug, Default)]
pub struct AlertEngine {
    states: HashMap<String, RuleState>,
}

impl AlertEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluate all rules against `vitals` at wall-clock `now_secs` (supplied
    /// by caller). Returns the set of events to log/emit this tick (fires and
    /// clears). Pure function of inputs + internal state; no I/O.
    pub fn evaluate(
        &mut self,
        rules: &[AlertRule],
        vitals: &Vitals,
        now_secs: u64,
    ) -> Vec<AlertEvent> {
        let mut out = Vec::new();
        let mut seen: HashMap<String, ()> = HashMap::new();
        for rule in rules {
            seen.insert(rule.id.clone(), ());
            let value = match resolve_metric(&rule.metric, vitals) {
                Some(v) => v,
                None => continue, // metric not available this tick
            };
            let cond = compare(value, &rule.comparison, rule.threshold);
            let st = self.states.entry(rule.id.clone()).or_default();

            if cond {
                // First time true — record when we started holding.
                if st.held_since.is_none() {
                    st.held_since = Some(now_secs);
                }
                let held = now_secs.saturating_sub(st.held_since.unwrap());
                if !st.firing && held >= rule.duration_secs {
                    st.firing = true;
                    out.push(AlertEvent {
                        ts: ts_from_secs(now_secs),
                        rule_id: rule.id.clone(),
                        state: AlertState::Fired,
                        value,
                    });
                }
            } else if st.firing {
                // Condition cleared — emit clear.
                st.firing = false;
                st.held_since = None;
                out.push(AlertEvent {
                    ts: ts_from_secs(now_secs),
                    rule_id: rule.id.clone(),
                    state: AlertState::Cleared,
                    value,
                });
            } else if !cond {
                st.held_since = None;
            }
        }
        out
    }

    pub fn is_firing(&self, rule_id: &str) -> bool {
        self.states.get(rule_id).map(|s| s.firing).unwrap_or(false)
    }
}

/// Resolve a metric id to a numeric value from the vitals snapshot.
/// Supported metric ids (spec §4.7):
///   - `temp` (max zone, °C), `temp.<zone>`
///   - `disk.pct` (max mount used %)
///   - `memory.pct` (used % of total)
///   - `service.<unit>` (1 = failed, 0 = ok) — wired by the caller via a
///     custom vitals extension; here returns None if not present.
///   - `mcu.heartbeat` (0 = stale, 1 = ok)
fn resolve_metric(metric: &str, vitals: &Vitals) -> Option<f64> {
    if metric == "temp" {
        return vitals
            .temperatures
            .iter()
            .map(|z| z.temp_millideg_c as f64 / 1000.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .into_option();
    }
    if let Some(zone) = metric.strip_prefix("temp.") {
        return vitals
            .temperatures
            .iter()
            .find(|z| z.zone == zone)
            .map(|z| z.temp_millideg_c as f64 / 1000.0);
    }
    if metric == "disk.pct" {
        return vitals
            .mounts
            .iter()
            .map(used_pct)
            .fold(0.0_f64, f64::max)
            .into_option();
    }
    if metric == "memory.pct" {
        if vitals.mem_total_kb == 0 {
            return None;
        }
        let used = vitals.mem_total_kb.saturating_sub(vitals.mem_available_kb);
        return Some(used as f64 * 100.0 / vitals.mem_total_kb as f64);
    }
    None
}

fn used_pct(m: &crate::monitoring::Mount) -> f64 {
    if m.total_bytes == 0 {
        return 0.0;
    }
    let used = m.total_bytes.saturating_sub(m.avail_bytes);
    used as f64 * 100.0 / m.total_bytes as f64
}

fn compare(value: f64, op: &str, threshold: f64) -> bool {
    match op {
        ">" => value > threshold,
        "<" => value < threshold,
        ">=" => value >= threshold,
        "<=" => value <= threshold,
        "==" => (value - threshold).abs() < f64::EPSILON,
        "!=" => (value - threshold).abs() >= f64::EPSILON,
        _ => false,
    }
}

fn ts_from_secs(secs: u64) -> String {
    // Deterministic ISO-ish stamp from a tick count; the real timestamp is
    // applied by the caller. Tests use small numbers.
    format!("t={secs}")
}

/// A bounded ring-buffer history file (spec §4.7).
pub struct AlertHistory {
    path: std::path::PathBuf,
    max_entries: usize,
}

impl AlertHistory {
    pub fn new(path: impl Into<std::path::PathBuf>, max_entries: usize) -> Self {
        Self {
            path: path.into(),
            max_entries,
        }
    }

    pub fn record(&self, event: &AlertEvent) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(event)?)?;
        drop(f);
        self.trim()?;
        Ok(())
    }

    pub fn read_all(&self) -> std::io::Result<Vec<AlertEvent>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let raw = std::fs::read_to_string(&self.path)?;
        Ok(raw
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<AlertEvent>(l).ok())
            .collect())
    }

    fn trim(&self) -> std::io::Result<()> {
        let raw = std::fs::read_to_string(&self.path).unwrap_or_default();
        let mut entries: Vec<AlertEvent> = raw
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        if entries.len() <= self.max_entries {
            return Ok(());
        }
        let drop = entries.len() - self.max_entries;
        entries.drain(..drop);
        let mut s = String::new();
        for e in &entries {
            s.push_str(&serde_json::to_string(e).unwrap());
            s.push('\n');
        }
        std::fs::write(&self.path, s)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Helper trait so `f64::NEG_INFINITY.into_option()` reads cleanly.
trait IntoOption {
    fn into_option(self) -> Option<f64>;
}
impl IntoOption for f64 {
    fn into_option(self) -> Option<f64> {
        if self.is_finite() {
            Some(self)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, metric: &str, op: &str, thr: f64, dur: u64) -> AlertRule {
        AlertRule {
            id: id.into(),
            metric: metric.into(),
            comparison: op.into(),
            threshold: thr,
            duration_secs: dur,
            actions: vec!["ui".into()],
        }
    }

    fn vitals(temp_c: f64) -> Vitals {
        let mut v = Vitals::default();
        v.temperatures.push(crate::monitoring::TempZone {
            zone: "thermal_zone0".into(),
            temp_millideg_c: (temp_c * 1000.0) as i64,
            throttled: false,
        });
        v
    }

    #[test]
    fn fires_only_after_duration_holds() {
        let mut eng = AlertEngine::new();
        let rules = vec![rule("hot", "temp", ">", 85.0, 60)];

        // Tick 0: condition true, but not held long enough.
        let e = eng.evaluate(&rules, &vitals(90.0), 0);
        assert!(e.is_empty());
        assert!(!eng.is_firing("hot"));

        // Tick 30: still holding, not yet 60s.
        let e = eng.evaluate(&rules, &vitals(90.0), 30);
        assert!(e.is_empty());

        // Tick 60: fires.
        let e = eng.evaluate(&rules, &vitals(90.0), 60);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].state, AlertState::Fired);
        assert!(eng.is_firing("hot"));
    }

    #[test]
    fn clears_when_condition_drops() {
        let mut eng = AlertEngine::new();
        let rules = vec![rule("hot", "temp", ">", 85.0, 60)];
        // warm up to firing
        for t in [0, 60] {
            eng.evaluate(&rules, &vitals(90.0), t);
        }
        assert!(eng.is_firing("hot"));
        let e = eng.evaluate(&rules, &vitals(40.0), 70);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].state, AlertState::Cleared);
        assert!(!eng.is_firing("hot"));
    }

    #[test]
    fn does_not_refire_if_dip_is_brief() {
        let mut eng = AlertEngine::new();
        let rules = vec![rule("hot", "temp", ">", 85.0, 60)];
        eng.evaluate(&rules, &vitals(90.0), 0);
        eng.evaluate(&rules, &vitals(90.0), 60); // fires
                                                 // brief dip then back up — must hold a fresh 60s, not refire instantly
        eng.evaluate(&rules, &vitals(40.0), 61); // clear
        let e = eng.evaluate(&rules, &vitals(90.0), 62);
        assert!(e.is_empty(), "should not refire immediately");
        let e = eng.evaluate(&rules, &vitals(90.0), 122);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].state, AlertState::Fired);
    }

    #[test]
    fn history_records_and_trims() {
        let dir = tempfile::tempdir().unwrap();
        let h = AlertHistory::new(dir.path().join("alert.jsonl"), 3);
        for i in 0..10 {
            h.record(&AlertEvent {
                ts: format!("t={i}"),
                rule_id: "r".into(),
                state: AlertState::Fired,
                value: i as f64,
            })
            .unwrap();
        }
        let raw = std::fs::read_to_string(h.path()).unwrap();
        let count = raw.lines().filter(|l| !l.is_empty()).count();
        assert_eq!(count, 3);
    }
}
