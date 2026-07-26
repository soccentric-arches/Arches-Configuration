# Board-manifest format

The **board manifest** is a BSP-provided, read-only JSON file at
`/usr/share/webconfig/board-manifest.json` (installed by the
`webconfig-manifest` Yocto recipe). It is the **only board-specific data**:
the daemon, frontend, and engine are Arches-only, and board-specific behavior
lives solely in this file (spec §3).

The daemon loads it at boot, in this order:
1. `/etc/webconfig/board.yaml` — an optional, intentional field-override
   escape hatch (YAML, for easy hand-editing): a technician can override a
   device's manifest without repackaging firmware.
2. `/usr/share/webconfig/board-manifest.json` — the canonical BSP-installed
   manifest described below.
3. The embedded `board-manifests/arches.json` (dev/CI only, compiled into the
   binary), if neither of the above exists or fails to parse.

There is intentionally only one canonical BSP-installed format (JSON) and
path — an earlier YAML variant at `<share>/board.yaml` has been removed.

## Top-level shape

```jsonc
{
  "platform": "arches",        // always "arches" — this repo is Arches-only
  "model": "nvidia-jetson",    // free-text silicon id
  "board_name": "NVIDIA Jetson (Arches)",
  "has_rtc": true,             // trust panel
  "has_tpm": false,            // trust panel
  "secure_boot": true,         // trust panel
  "service_allowlist": [ "sshd.service", "..." ],  // curated services (§4.3)
  "peripherals": [ { /* Peripheral */ } ],
  "dashboard_tiles": [ { /* DashboardTile */ } ],
  "default_alert_rules": [ { /* AlertRule */ } ],  // per-platform default rule packs (§4.7)
  "config_defaults": { /* a full Config tree, optional */ }
}
```

`config_defaults` seeds the live SQLite configuration **only on first boot**
(`ConfigStore::with_defaults`, `crates/webconfigd/src/bootstrap.rs`) — every
subsequent edit is stored in SQLite and does not re-read this field. It is
optional; when absent, `Config::default()` is used. `board-manifests/
generic.yaml` uses it to seed a sane starting point (e.g. NTP enabled) for
the generic-Linux dev/CI fallback; `board-manifests/arches.json` does not
currently declare one (Phase 5 data-quality note: confirm whether Arches
should also ship first-boot defaults, or whether relying on
`Config::default()` is intentional).

## Peripheral

```jsonc
{
  "id": "can0",                       // stable id; config references peripherals by id
  "device": "can0",                   // Linux device name ("/dev/ttyTHS0", "gpiochip1 line 7", ...)
  "human_name": "CAN channel 0",      // UI label
  "kind": "can",                      // IoKind (see below)
  "caps": { "fd": true, "termination_gpio": "gpio-can0-term" }  // free-form, platform quirks
}
```

### `IoKind` (fixed enum, lowercase)

`serial` · `can` · `gpio` · `i2c` · `spi` · `mcu` · `fpga` · `pru` · `camera` ·
`display` · `thermal` · `net`

`caps` is intentionally free-form JSON so new platform quirks (GPIO-switchable
CAN termination, an MCU's `settings_schema` id, etc.) need **no code change**.
The I/O page renders peripherals generically from `kind` + `human_name` +
`device`; `caps` is shown as detail and consumed by platform panels (§4.9).

For a `kind: "mcu"` peripheral, two `caps` keys are meaningful to
`POST /io/mcu/firmware` (spec §4.9):
- `firmware_pubkey_ed25519` — hex-encoded 32-byte Ed25519 public key, the
  trust anchor a firmware upload's detached signature is verified against.
  Required: without it, firmware upload is refused (409) rather than accepted
  unverified.
- `flash_command` — optional shell command invoked with the staged firmware
  path as its sole argument on the real root after signature verification
  succeeds. Absent means no flash mechanism exists for this platform yet; the
  route still verifies + stores the firmware but reports `flashed: false`
  with an explicit "not implemented" reason instead of fabricating success.

## DashboardTile

```jsonc
{
  "id": "gpu",
  "title": "GPU utilization",
  "metrics": ["gpu.util", "gpu.mem"],   // metric ids pulled from /monitoring/vitals or platform endpoints
  "render": "sparkline"                 // sparkline | value | state | bar
}
```

Tiles ship as **data** (Arches: GPU/power/fan; disk-full, watchdog). The
frontend renders them generically (spec §4.5).

## Service allowlist

`service_allowlist` is the curated set of systemd units the Services page may
show and control. The daemon **never enumerates all units** — only this list
(spec §4.3). The polkit rule in the Yocto layer mirrors this list for
authorization; update both together when the Arches image changes its curated
services.

## Manifest selection

The Yocto layer's `webconfig-manifest.bb` recipe references
`board-manifests/arches.json` directly from the repo root (packaged inside the
same source tarball `webconfigd.bb`/`webconfig-static.bb` fetch — never copied
into the layer, so there is exactly one source of truth) and defaults to it:

```bitbake
WEBCONFIG_BOARD_FILE ?= "arches.json"
```

`webconfig-manifest.bbappend` overrides this per `MACHINE` only if a future
board revision needs a distinct manifest file:

```bitbake
WEBCONFIG_BOARD_FILE:jetson-orin-nano = "arches.json"
```

## Reference manifest

- [`board-manifests/arches.json`](../board-manifests/arches.json) — Jetson (CAN,
  UART, I2C, SPI, GPIO, STM32 MCU, cameras, thermal, eth/wlan, nvpmodel tiles)