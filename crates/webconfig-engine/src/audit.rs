//! Append-only audit log (spec §4.8).
//!
//! An append-only ring file of all config-UI actions (user, action, timestamp),
//! viewable under Logs and included in the diagnostics bundle (spec §4.4).
//! Implemented as a bounded ring buffer on disk so it never grows unbounded on
//! eMMC. No database (spec §2).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// One audit entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// ISO-8601 timestamp (UTC) supplied by the caller — the engine is
    /// deterministic and never calls the wall clock itself, so the HTTP layer
    /// stamps events.
    pub ts: String,
    pub user: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Bounded ring-buffer audit log. When the file exceeds `max_bytes`, the oldest
/// entries are dropped on the next `rotate_if_needed`.
#[derive(Debug)]
pub struct AuditLog {
    path: std::path::PathBuf,
    max_bytes: u64,
}

impl AuditLog {
    pub fn new(path: impl Into<std::path::PathBuf>, max_bytes: u64) -> Self {
        Self {
            path: path.into(),
            max_bytes,
        }
    }

    pub fn append(&self, entry: &AuditEntry) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let line = serde_json::to_string(entry)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{line}")?;
        drop(f);
        self.rotate_if_needed()?;
        Ok(())
    }

    /// Read all current entries (newest last).
    pub fn read_all(&self) -> std::io::Result<Vec<AuditEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let raw = std::fs::read_to_string(&self.path)?;
        let mut out = Vec::new();
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(e) = serde_json::from_str::<AuditEntry>(line) {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// Trim the oldest lines until under the byte budget.
    fn rotate_if_needed(&self) -> std::io::Result<()> {
        let len = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if len <= self.max_bytes {
            return Ok(());
        }
        let entries = self.read_all()?;
        let mut kept = entries;
        // Drop oldest until under ~80% of the budget.
        let target = self.max_bytes * 4 / 5;
        loop {
            let est: usize = kept
                .iter()
                .map(|e| serde_json::to_string(e).map(|s| s.len() + 1).unwrap_or(0))
                .sum();
            if (est as u64) <= target || kept.is_empty() {
                break;
            }
            kept.remove(0);
        }
        let mut s = String::new();
        for e in &kept {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::new(dir.path().join("audit.jsonl"), 1 << 20);
        log.append(&AuditEntry {
            ts: "2026-07-21T10:00:00Z".into(),
            user: "admin".into(),
            action: "network.apply".into(),
            detail: Some("eth0".into()),
        })
        .unwrap();
        let all = log.read_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].action, "network.apply");
    }

    #[test]
    fn rotates_when_over_budget() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::new(dir.path().join("audit.jsonl"), 300);
        for i in 0..50 {
            log.append(&AuditEntry {
                ts: format!("2026-07-21T10:00:{i:02}Z"),
                user: "admin".into(),
                action: format!("act-{i}"),
                detail: None,
            })
            .unwrap();
        }
        let len = std::fs::metadata(log.path()).unwrap().len();
        assert!(len <= 300, "len={len}");
        assert!(len > 0);
    }
}
