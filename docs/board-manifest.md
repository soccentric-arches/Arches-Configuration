# Board-manifest format

The **board manifest** is a BSP-provided, read-only JSON file at
`/usr/share/webconfig/board-manifest.json` (installed by the
`webconfig-manifest` Yocto recipe). It is the **only platform-specific data**:
the six platforms (Arches, Acadia, Zion, Pinnacle, Joshua, Sequoia) share one
daemon, one frontend, and one engine, and differ only in this file (spec §3).

The daemon loads it at boot. Missing it falls back to an embedded example for
the configured `WEBCONFIG_PLATFORM` (dev/CI only).

## Top-level shape

```jsonc
{
  "platform": "arches",        // arches | acadia | zion | pinnacle | joshua | sequoia
  "model": "nvidia-jetson",    // free-text silicon id
  "board_name": "NVIDIA Jetson (Arches)",
  "has_rtc": true,             // trust panel
  "has_tpm": false,            // trust panel
  "secure_boot": true,         // trust panel
  "service_allowlist": [ "sshd.service", "..." ],  // curated services (§4.3)
  "peripherals": [ { /* Peripheral */ } ],
  "dashboard_tiles": [ { /* DashboardTile */ } ]
}
```

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

## DashboardTile

```jsonc
{
  "id": "gpu",
  "title": "GPU utilization",
  "metrics": ["gpu.util", "gpu.mem"],   // metric ids pulled from /monitoring/vitals or platform endpoints
  "render": "sparkline"                 // sparkline | value | state | bar
}
```

Per-platform tiles ship as **data** (Arches: GPU/power/fan; all: disk-full,
watchdog). The frontend renders them generically (spec §4.5).

## Service allowlist

`service_allowlist` is the curated set of systemd units the Services page may
show and control. The daemon **never enumerates all units** — only this list
(spec §4.3). The polkit rule in the Yocto layer mirrors this list for
authorization; update both together when a platform image changes its curated
services.

## Per-platform selection

The Yocto layer ships all six manifests in
`meta-soccentric-webconfig/recipes-core/webconfig/files/` and selects one per
BSP `MACHINE` in `webconfig-manifest.bbappend`:

```bitbake
WEBCONFIG_MANIFEST_FILE:jetson-orin-nano = "arches.json"
```

## Reference manifests

- [`board-manifests/arches.json`](../board-manifests/arches.json) — Jetson (CAN,
  UART, I2C, SPI, GPIO, STM32 MCU, cameras, thermal, eth/wlan, nvpmodel tiles)
- `acadia.json`, `zion.json`, `pinnacle.json`, `joshua.json`, `sequoia.json`
  — same shape, platform-specific peripherals/tiles/allowlist.