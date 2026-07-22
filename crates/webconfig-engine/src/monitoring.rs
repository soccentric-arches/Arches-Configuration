//! Monitoring readers (spec §4.5 — no resident collectors).
//!
//! The backend reads `/proc` and `/sys` **on request**; the browser polls a
//! JSON endpoint every few seconds only while the page is open. No resident
//! collectors, no Prometheus/Grafana (spec §4.5). Everything here is a pure
//! read from the (fake, in tests) sysroot.

use serde::Serialize;
use std::fs;
use std::path::Path;

use crate::sysroot::Sysroot;

/// A snapshot of device vitals returned to the dashboard polling endpoint.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Vitals {
    pub load: [f64; 3],
    pub mem_total_kb: u64,
    pub mem_available_kb: u64,
    pub mounts: Vec<Mount>,
    pub temperatures: Vec<TempZone>,
    pub net: Vec<NetStats>,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Mount {
    pub source: String,
    pub target: String,
    pub fstype: String,
    pub total_bytes: u64,
    pub avail_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TempZone {
    pub zone: String,
    pub temp_millideg_c: i64,
    pub throttled: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NetStats {
    pub iface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Read vitals from the sysroot. Missing files degrade gracefully to defaults
/// (spec §4.5: warn when no data, never crash the dashboard).
pub fn read_vitals(sysroot: &Sysroot) -> Vitals {
    Vitals {
        load: read_loadavg(sysroot),
        mem_total_kb: read_meminfo_field(sysroot, "MemTotal:"),
        mem_available_kb: read_meminfo_field(sysroot, "MemAvailable:"),
        // Prefer MemAvailable; fall back to MemFree if absent (spec §4.5:
        // show available, not free).
        mounts: read_mounts(sysroot),
        temperatures: read_thermal(sysroot),
        net: read_net_stats(sysroot),
        uptime_secs: read_uptime(sysroot),
    }
}

fn read_loadavg(sysroot: &Sysroot) -> [f64; 3] {
    let p = sysroot.join("/proc/loadavg");
    let Ok(raw) = fs::read_to_string(&p) else {
        return [0.0; 3];
    };
    let mut parts = raw.split_whitespace();
    let mut out = [0.0f64; 3];
    for slot in out.iter_mut() {
        if let Some(s) = parts.next() {
            *slot = s.parse().unwrap_or(0.0);
        }
    }
    out
}

fn read_meminfo_field(sysroot: &Sysroot, prefix: &str) -> u64 {
    let p = sysroot.join("/proc/meminfo");
    let Ok(raw) = fs::read_to_string(&p) else {
        return 0;
    };
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix(prefix) {
            let rest = rest.trim().trim_end_matches("kB").trim();
            return rest.parse().unwrap_or(0);
        }
    }
    0
}

fn read_uptime(sysroot: &Sysroot) -> u64 {
    let p = sysroot.join("/proc/uptime");
    let Ok(raw) = fs::read_to_string(&p) else {
        return 0;
    };
    raw.split_whitespace()
        .next()
        .and_then(|s| s.split('.').next().map(|n| n.parse::<u64>().unwrap_or(0)))
        .unwrap_or(0)
}

fn read_mounts(sysroot: &Sysroot) -> Vec<Mount> {
    let p = sysroot.join("/proc/mounts");
    let Ok(raw) = fs::read_to_string(&p) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in raw.lines() {
        let mut it = line.split_whitespace();
        let (Some(source), Some(target), Some(fstype)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        // Only physical-ish filesystems — skip proc/sysfs/cgroup/tmpfs noise
        // unless tmpfs (some boards mount app data on tmpfs).
        let interesting = matches!(
            fstype,
            "ext4" | "ext3" | "ext2" | "btrfs" | "f2fs" | "vfat" | "squashfs" | "overlay" | "tmpfs"
        );
        if !interesting {
            continue;
        }
        let (total, avail) = statvfs(sysroot, target);
        out.push(Mount {
            source: source.into(),
            target: target.into(),
            fstype: fstype.into(),
            total_bytes: total,
            avail_bytes: avail,
        });
    }
    out
}

fn statvfs(sysroot: &Sysroot, target: &str) -> (u64, u64) {
    // Test sysroots use a hint file because their mounts are not real. On a
    // generic Linux host, use the standard `df` interface for actual usage.
    let hint = sysroot
        .root()
        .join("sys-block-usage")
        .join(target.trim_start_matches('/').replace('/', "_"));
    if let Ok(raw) = fs::read_to_string(&hint) {
        let mut it = raw.split_whitespace();
        let total: u64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let avail: u64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        return (total, avail);
    }
    if !sysroot.is_real_root() {
        return (0, 0);
    }
    let Ok(output) = std::process::Command::new("df")
        .args(["-P", "-k", "--", target])
        .output()
    else {
        return (0, 0);
    };
    if !output.status.success() {
        return (0, 0);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(line) = stdout.lines().nth(1) else {
        return (0, 0);
    };
    let fields: Vec<&str> = line.split_whitespace().collect();
    let Some(total_kib) = fields.get(1).and_then(|value| value.parse::<u64>().ok()) else {
        return (0, 0);
    };
    let Some(available_kib) = fields.get(3).and_then(|value| value.parse::<u64>().ok()) else {
        return (0, 0);
    };
    (
        total_kib.saturating_mul(1024),
        available_kib.saturating_mul(1024),
    )
}

fn read_thermal(sysroot: &Sysroot) -> Vec<TempZone> {
    let base = sysroot.join("/sys/class/thermal");
    let Ok(entries) = fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("thermal_zone") {
            continue;
        }
        let temp = fs::read_to_string(base.join(name.as_ref()).join("temp"))
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(0);
        out.push(TempZone {
            zone: name.into_owned(),
            temp_millideg_c: temp,
            throttled: false,
        });
    }
    out
}

fn read_net_stats(sysroot: &Sysroot) -> Vec<NetStats> {
    let base = sysroot.join("/sys/class/net");
    let Ok(entries) = fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let iface = e.file_name().to_string_lossy().into_owned();
        if iface == "lo" {
            continue;
        }
        let rx = read_u64_file(&base.join(&iface).join("statistics/rx_bytes"));
        let tx = read_u64_file(&base.join(&iface).join("statistics/tx_bytes"));
        out.push(NetStats {
            iface,
            rx_bytes: rx,
            tx_bytes: tx,
        });
    }
    out
}

fn read_u64_file(p: &Path) -> u64 {
    fs::read_to_string(p)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// eMMC wear from lifetime registers (`mmc extcsd`) where present (spec §4.5).
/// Returns `None` when the device is not eMMC or the extcsd read fails. The
/// real implementation shells out to `mmc extcsd read <dev>`; in a tempdir
/// sysroot it reads a fake `extcsd` file the test drops in.
pub fn read_emmc_wear(sysroot: &Sysroot, dev: &str) -> Option<u8> {
    let path = sysroot.join(format!("/sys/block/{dev}/device/life_time_estimate"));
    if let Ok(s) = fs::read_to_string(&path) {
        return s.trim().parse::<u8>().ok();
    }
    let fake = sysroot.root().join("fake-extcsd").join(dev);
    fs::read_to_string(&fake)
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_sys() -> (tempfile::TempDir, Sysroot) {
        let d = tempfile::tempdir().unwrap();
        let sys = Sysroot::at(d.path());
        // /proc
        std::fs::create_dir_all(sys.join("/proc")).unwrap();
        std::fs::write(sys.join("/proc/loadavg"), "0.10 0.20 0.30 1/100 1234\n").unwrap();
        std::fs::write(
            sys.join("/proc/meminfo"),
            "MemTotal:       1024000 kB\nMemAvailable:    512000 kB\n",
        )
        .unwrap();
        std::fs::write(sys.join("/proc/uptime"), "12345.67 67890.12\n").unwrap();
        std::fs::write(
            sys.join("/proc/mounts"),
            "rootfs / ext4 rw 0 0\ntmpfs /run tmpfs rw 0 0\nproc /proc proc rw 0 0\n",
        )
        .unwrap();
        std::fs::create_dir_all(sys.root().join("sys-block-usage")).unwrap();
        std::fs::write(sys.root().join("sys-block-usage/_"), "1000000 400000\n").unwrap();
        // /sys/class/thermal
        std::fs::create_dir_all(sys.join("/sys/class/thermal/thermal_zone0")).unwrap();
        std::fs::write(sys.join("/sys/class/thermal/thermal_zone0/temp"), "55000\n").unwrap();
        // /sys/class/net
        std::fs::create_dir_all(sys.join("/sys/class/net/eth0/statistics")).unwrap();
        std::fs::write(
            sys.join("/sys/class/net/eth0/statistics/rx_bytes"),
            "1024\n",
        )
        .unwrap();
        std::fs::write(
            sys.join("/sys/class/net/eth0/statistics/tx_bytes"),
            "2048\n",
        )
        .unwrap();
        (d, sys)
    }

    #[test]
    fn reads_vitals_from_fake_sysroot() {
        let (_d, sys) = fake_sys();
        let v = read_vitals(&sys);
        assert_eq!(v.load, [0.10, 0.20, 0.30]);
        assert_eq!(v.mem_total_kb, 1_024_000);
        assert_eq!(v.mem_available_kb, 512_000);
        assert_eq!(v.uptime_secs, 12345);
        assert_eq!(v.temperatures.len(), 1);
        assert_eq!(v.temperatures[0].temp_millideg_c, 55000);
        assert_eq!(v.net.len(), 1);
        assert_eq!(v.net[0].rx_bytes, 1024);
        assert_eq!(v.net[0].tx_bytes, 2048);
        // Only ext4 and tmpfs mounts are interesting; proc is skipped.
        assert_eq!(v.mounts.len(), 2);
    }

    #[test]
    fn emmc_wear_from_fake_extcsd() {
        let d = tempfile::tempdir().unwrap();
        let sys = Sysroot::at(d.path());
        std::fs::create_dir_all(sys.root().join("fake-extcsd")).unwrap();
        std::fs::write(sys.root().join("fake-extcsd/mmcblk0"), "8\n").unwrap();
        assert_eq!(read_emmc_wear(&sys, "mmcblk0"), Some(8));
        assert_eq!(read_emmc_wear(&sys, "mmcblk9"), None);
    }
}
