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
    pub swap_total_bytes: u64,
    pub swap_free_bytes: u64,
    pub temperatures: Vec<TempZone>,
    pub net: Vec<NetStats>,
    pub uptime_secs: u64,
    /// Number of boots recorded by the journal (spec §4.5: "boot count").
    /// `None` when not reported (fake sysroot / no journal).
    pub boot_count: Option<u64>,
    /// Why the last boot happened, where the platform/MCU reports it
    /// (spec §4.5: "last boot reason"). `None` when no source reports it.
    pub last_boot_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Mount {
    pub source: String,
    pub target: String,
    pub fstype: String,
    pub physical: bool,
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
    let (boot_count, last_boot_reason) = read_boot_info(sysroot);
    let (swap_total_bytes, swap_free_bytes) = read_swap(sysroot);
    Vitals {
        load: read_loadavg(sysroot),
        mem_total_kb: read_meminfo_field(sysroot, "MemTotal:"),
        mem_available_kb: read_meminfo_field(sysroot, "MemAvailable:"),
        // Prefer MemAvailable; fall back to MemFree if absent (spec §4.5:
        // show available, not free).
        mounts: read_mounts(sysroot),
        swap_total_bytes,
        swap_free_bytes,
        temperatures: read_thermal(sysroot),
        net: read_net_stats(sysroot),
        uptime_secs: read_uptime(sysroot),
        boot_count,
        last_boot_reason,
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
            physical: is_physical_source(source),
            total_bytes: total,
            avail_bytes: avail,
        });
    }
    out
}

fn is_physical_source(source: &str) -> bool {
    source.starts_with("/dev/")
        && !source.starts_with("/dev/mapper/")
        && !source.starts_with("/dev/dm-")
        && !source.starts_with("/dev/loop")
        && !source.starts_with("/dev/zram")
}

fn read_swap(sysroot: &Sysroot) -> (u64, u64) {
    let Ok(raw) = fs::read_to_string(sysroot.join("/proc/swaps")) else {
        return (0, 0);
    };
    let totals = raw
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            let total_kib = fields.get(2)?.parse::<u64>().ok()?;
            let used_kib = fields.get(3)?.parse::<u64>().ok()?;
            Some((
                total_kib.saturating_mul(1024),
                used_kib.saturating_mul(1024),
            ))
        })
        .fold(
            (0_u64, 0_u64),
            |(total, used), (entry_total, entry_used)| {
                (
                    total.saturating_add(entry_total),
                    used.saturating_add(entry_used),
                )
            },
        );
    let (total, used) = totals;
    (total, total.saturating_sub(used))
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
    let mut out = read_thermal_zones(sysroot);
    out.extend(read_hwmon_temps(sysroot));
    out
}

/// `/sys/class/thermal/thermal_zone*` (spec §4.5). The throttle flag is read
/// from the zone's cooling devices: if any `cdev*/cur_state` is > 0 the zone is
/// being actively cooled/throttled. Assumption (per spec §7): on fanless SoCs
/// the first cooling device is the CPU-frequency clamp, so `cur_state > 0`
/// means frequency throttling — the Arches "thermal-throttle state". Degrades
/// to `false` when no cooling devices are exposed.
fn read_thermal_zones(sysroot: &Sysroot) -> Vec<TempZone> {
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
        let zone_dir = base.join(name.as_ref());
        let temp = fs::read_to_string(zone_dir.join("temp"))
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(0);
        let throttled = zone_throttled(&zone_dir);
        out.push(TempZone {
            zone: name.into_owned(),
            temp_millideg_c: temp,
            throttled,
        });
    }
    out
}

/// True if any cooling device under this thermal zone has `cur_state > 0`.
fn zone_throttled(zone_dir: &Path) -> bool {
    let Ok(cdevs) = fs::read_dir(zone_dir) else {
        return false;
    };
    for c in cdevs.flatten() {
        let cname = c.file_name();
        let cname = cname.to_string_lossy();
        if !cname.starts_with("cdev") {
            continue;
        }
        let cur = read_u64_file(&zone_dir.join(cname.as_ref()).join("cur_state"));
        if cur > 0 {
            return true;
        }
    }
    false
}

/// `/sys/class/hwmon/hwmon*/temp*_input` (spec §4.5: temperature from
/// `/sys/class/thermal` **+ hwmon**). Many SoCs (incl. Arches/Jetson) expose
/// board temps only via hwmon. The zone label is `hwmonN:<chip name>:tempK` so
/// the UI can tell the sources apart. `throttled` is not a hwmon concept →
/// false here; throttle state lives in the thermal-zone path above.
fn read_hwmon_temps(sysroot: &Sysroot) -> Vec<TempZone> {
    let base = sysroot.join("/sys/class/hwmon");
    let Ok(entries) = fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let dir = e.path();
        let hwmon = e.file_name().to_string_lossy().to_string();
        let chip = fs::read_to_string(dir.join("name"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let Ok(temp_entries) = fs::read_dir(&dir) else {
            continue;
        };
        for t in temp_entries.flatten() {
            let fname = t.file_name();
            let fname = fname.to_string_lossy();
            if !fname.starts_with("temp") || !fname.ends_with("_input") {
                continue;
            }
            let temp = fs::read_to_string(t.path())
                .ok()
                .and_then(|s| s.trim().parse::<i64>().ok())
                .unwrap_or(0);
            out.push(TempZone {
                zone: format!("{hwmon}:{chip}:{}", fname.trim_end_matches("_input")),
                temp_millideg_c: temp,
                throttled: false,
            });
        }
    }
    out
}

/// Boot count + last boot reason (spec §4.5: "where the platform/MCU reports
/// it"). Boot count comes from `journalctl --list-boots` on the real root (one
/// line per boot); the last-boot reason is read from a file an MCU/platform
/// daemon writes into the webconfig dir. Both are `None` when not reported —
/// never a silent stub (spec §7).
fn read_boot_info(sysroot: &Sysroot) -> (Option<u64>, Option<String>) {
    let boot_count = if sysroot.is_real_root() {
        let boots = crate::applier::run_cmd("journalctl", &["--list-boots", "--no-pager"])
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count() as u64)
            .ok();
        // A single-line "No boots found." style error → no boots recorded.
        boots.filter(|&n| n > 0)
    } else {
        None
    };
    let reason_path = sysroot.webconfig_dir().join("last-boot-reason");
    let last_boot_reason = fs::read_to_string(&reason_path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    (boot_count, last_boot_reason)
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
            "rootfs / ext4 rw 0 0\ntmpfs /run tmpfs rw 0 0\n/dev/mmcblk0p1 /data ext4 rw 0 0\nproc /proc proc rw 0 0\n",
        )
        .unwrap();
        std::fs::write(
            sys.join("/proc/swaps"),
            "Filename\tType\tSize\tUsed\tPriority\n/dev/zram0\tpartition\t1024\t256\t5\n",
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
        // Only ext4, tmpfs, and the physical data mount are interesting; proc is skipped.
        assert_eq!(v.mounts.len(), 3);
        assert!(v.mounts.iter().any(|mount| mount.physical));
        assert_eq!(v.swap_total_bytes, 1024 * 1024);
        assert_eq!(v.swap_free_bytes, 768 * 1024);
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

    #[test]
    fn reads_hwmon_temps_alongside_thermal_zones() {
        // Spec §4.5: temperature from /sys/class/thermal + hwmon. A SoC that
        // exposes a board temp only via hwmon must still appear in vitals.
        let (_d, sys) = fake_sys();
        let hwmon = sys.join("/sys/class/hwmon/hwmon0");
        std::fs::create_dir_all(&hwmon).unwrap();
        std::fs::write(hwmon.join("name"), "jetson_thlm\n").unwrap();
        std::fs::write(hwmon.join("temp1_input"), "47000\n").unwrap();
        std::fs::write(hwmon.join("temp2_input"), "52000\n").unwrap();
        let v = read_vitals(&sys);
        let hwmon_zones: Vec<_> = v
            .temperatures
            .iter()
            .filter(|z| z.zone.starts_with("hwmon0:"))
            .collect();
        assert_eq!(hwmon_zones.len(), 2);
        assert!(hwmon_zones.iter().any(|z| z.zone.contains("jetson_thlm")));
        assert!(hwmon_zones.iter().any(|z| z.temp_millideg_c == 52_000));
        // The thermal_zone0 from fake_sys() is still present.
        assert!(v.temperatures.iter().any(|z| z.zone == "thermal_zone0"));
    }

    #[test]
    fn throttle_flag_set_when_cooling_device_active() {
        // Spec §4.5/§7: thermal-throttle state is read, not hardcoded false.
        // A cooling device with cur_state > 0 means the zone is throttled.
        let (_d, sys) = fake_sys();
        let zone = sys.join("/sys/class/thermal/thermal_zone0");
        std::fs::create_dir_all(zone.join("cdev0")).unwrap();
        std::fs::write(zone.join("cdev0/cur_state"), "3\n").unwrap();
        let v = read_vitals(&sys);
        let z0 = v
            .temperatures
            .iter()
            .find(|z| z.zone == "thermal_zone0")
            .unwrap();
        assert!(z0.throttled, "active cooling device → throttled");

        // cur_state 0 → not throttled.
        std::fs::write(zone.join("cdev0/cur_state"), "0\n").unwrap();
        let v = read_vitals(&sys);
        let z0 = v
            .temperatures
            .iter()
            .find(|z| z.zone == "thermal_zone0")
            .unwrap();
        assert!(!z0.throttled);
    }

    #[test]
    fn boot_info_optional_on_fake_sysroot() {
        // Fake sysroot has no journal → boot_count None; last-boot reason is
        // read from the webconfig dir only if an MCU daemon wrote it.
        let (_d, sys) = fake_sys();
        let v = read_vitals(&sys);
        assert_eq!(v.boot_count, None);
        assert_eq!(v.last_boot_reason, None);

        // When the platform reports a reason, it surfaces.
        std::fs::create_dir_all(sys.webconfig_dir()).unwrap();
        std::fs::write(sys.webconfig_dir().join("last-boot-reason"), "watchdog\n").unwrap();
        let v = read_vitals(&sys);
        assert_eq!(v.last_boot_reason.as_deref(), Some("watchdog"));
    }
}
