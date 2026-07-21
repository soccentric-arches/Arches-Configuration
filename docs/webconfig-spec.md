# PROMPT: Generate "SoCcentric WebConfig" — an embedded device configuration web UI

You are a senior embedded-systems and Rust engineer. Generate a complete, production-quality implementation of **WebConfig**, an on-device configuration web UI for Yocto-based embedded Linux products. Produce real, buildable code — not pseudocode or placeholders — following every requirement below.

---

## 1. Context

SoCcentric ships six embedded Linux platforms, all running a common Yocto-based OS:

| Platform | Silicon | Real-time companion | Notable |
|---|---|---|---|
| Arches | NVIDIA Jetson | STM32 MCU | GPU/ML, nvpmodel power modes, fan, cameras |
| Acadia | Raspberry Pi CM4/CM5 | Pico W (RP2040) | Lowest-resource target (1 GB RAM SKU exists) |
| Zion | Xilinx Zynq-7000 / UltraScale+ | — (FPGA fabric) | FPGA bitstream management, PetaLinux BSP |
| Pinnacle | NXP i.MX | optional | Multi-Ethernet, displays/HMI, medical/industrial |
| Joshua | TI Sitara | PRU cores | PRU firmware, industrial protocols |
| Sequoia | Intel/AMD x86 | none (PREEMPT_RT/hypervisor) | UEFI boot, standard x86 sensors |

WebConfig ships inside the Yocto image on **all six platforms** from **one codebase**. Users are field installers, operators, and customer engineers accessing it from a browser (desktop or phone) over the LAN or via the device's provisioning Wi-Fi AP.

## 2. Hard constraints (non-negotiable)

- **Footprint budget:** backend < 15 MB resident RAM, < 10 MB added to image, near-zero CPU when idle, exactly **one daemon**.
- **Backend:** Rust. Single static-ish binary. Crates: **axum** (HTTP), **tokio** (`current_thread` runtime), **rustls** (TLS; no OpenSSL), **rcgen** (self-signed cert generation), **serde/serde_json**, **jsonschema** (settings validation), **argon2** (password hashing), **zbus** (systemd/timedated D-Bus; no libdbus), **tower-http** (static serving + compression), **tower-sessions** or HMAC-signed cookies. **No database** — JSON files and ring-buffer files only.
- **Release profile:** `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, stripped. Target 3–6 MB binary.
- **Cross-targets:** `aarch64-unknown-linux-gnu`, `armv7-unknown-linux-gnueabihf` (Zynq-7000, older i.MX), `x86_64-unknown-linux-gnu`. All crates must build on all three; avoid crates broken on 32-bit.
- **Crates vendored** (`cargo vendor`) — builds must never require crates.io at build time.
- **Frontend:** static files served by the daemon, executing entirely in the user's browser. Vanilla JS **or** a vendored micro-library (Preact or Lit committed to the repo). **No npm/node at build time. No build step.** Total assets < 500 KB uncompressed. Must work on evergreen browsers and recent Safari, and be fully usable at phone width (installers use phones via the AP flow). No server-side HTML rendering — JSON APIs + static files only.
- **Security boundary is the API**, never the frontend. Every input re-validated server-side.
- **Filesystem model:** read-only rootfs + writable data partition (e.g. `/data`). All mutable state lives on the data partition.

## 3. Core architecture

One daemon, three layers:

1. **HTTP layer** — TLS, auth/sessions/CSRF, static assets, JSON REST, one SSE endpoint (log follow + dashboard push). Runs as a **non-root** user.
2. **Config engine** — the canonical config store: a single versioned JSON tree at `/data/webconfig/config.json` (with atomic write + backup copy). The UI **never edits /etc directly**. Instead, **appliers** translate store → system.
3. **Platform layer** — reads BSP-provided files: a **board manifest** (`/usr/share/webconfig/board-manifest.json`, declares available I/O: CAN/serial/GPIO/I2C/MCU/FPGA/PRU with Linux device names and human names) and a **platform status file**. Per-platform dashboard tiles and panels are declared as data, not code. The six platforms differ *only* in this layer.

**Applier trait** (define exactly this shape): synchronous `validate(&Config) -> Result`, `apply(&Config) -> Result`, `rollback() -> Result`. Appliers run in a dedicated blocking task; the async HTTP layer communicates with the engine over a channel. Every applier takes a **sysroot path parameter** (default `/`) so CI can run the entire config engine against a tempdir fake sysroot on x86 with no hardware.

**Privilege split:** HTTP layer non-root; privileged operations go through systemd D-Bus with polkit rules, or a minimal root-side helper with a fixed verb API.

**Schema-driven rendering everywhere:** application settings, dashboard tiles, alert rule packs, MCU settings, and board I/O are all rendered from declarative schema/manifest files dropped into `/usr/share/webconfig/schemas/` by other Yocto recipes. The frontend contains a generic schema→form renderer (types: string, int, bool, enum, secret; validation rules; grouping; an `apply_hook` declaring "restart unit X" or "SIGHUP pid of X"). Adding a customer setting must require **zero UI code changes**.

## 4. Functional requirements — nine sections

Implement each page below. Items marked **[v2]** are deferred: design the config schema so they can be added without migration, but do not implement.

### 4.1 System
- Hostname (via `hostnamectl`) **plus** a separate free-text asset tag / site label (config store only).
- Timezone dropdown (from tzdata), NTP on/off with editable server list (systemd-timesyncd), manual date/time set for air-gapped sites (`timedatectl`), RTC sync after manual set where present, warning banner when no RTC and NTP unreachable.
- Web-UI user management: local user store at `/data/webconfig/users.json`, argon2 hashes, roles **admin / operator / viewer** (admin: everything; operator: app settings + service restart only; viewer: read-only). Separate single action: change Linux root password (`chpasswd`).
- Toggles for `sshd` and serial getty units, SSH authorized-keys textarea. Warn on actions that could cut off the current session.
- Factory reset: wipe data partition and reboot; require typed confirmation ("RESET"); offer "settings only" vs "settings + user data".
- Reboot/shutdown with a "rebooting, reconnecting…" page that polls until the device returns.
- Read-only device info panel: platform, model, serial, OS version/build date (os-release + a manufacturing identity file), uptime, MACs.

### 4.2 Networking
- Backend against **systemd-networkd** (wired) + **iwd** or wpa_supplicant (Wi-Fi). Generate `.network` files from the config store.
- Ethernet per interface (enumerated dynamically, never hardcoded — Pinnacle/Sequoia have multiple MACs): DHCP/static (IP, prefix, gateway), MTU, enable/disable, live MAC + link state.
- **Commit/confirm rollback (flagship feature):** any change that could sever the current connection is applied provisionally; if the user does not reconfirm on the new address within a configurable window (default 90 s), the backend automatically rolls back. Implement as a tokio timer task + applier `rollback()`.
- Pre-apply validation: gateway inside subnet, no duplicate IPs, etc.
- Wi-Fi client: scan with SSID/signal list, join with PSK, multiple remembered networks with priority, forget, regulatory domain setting (`iw reg set`). **[v2]** 802.1X/EAP.
- **AP provisioning mode:** when unconfigured (or on trigger), start hostapd + a DHCP server, serve the UI at 192.168.4.1. Trigger configurable: no-config-present or GPIO button.
- DNS servers (global + per-interface via resolved), NTP servers (shared with System page — same store keys).
- **Service-based firewall:** no rule editor. Checkboxes per known service (Web UI 443, SSH 22, MQTT 1883, Modbus 502, …) per interface; backend generates nftables rules. Default: everything closed except the UI.
- Connectivity test button: ping gateway → ping public IP → DNS resolve, reporting which layer failed.
- **[v2]** VLANs, bridging, cellular/ModemManager (design schema slots now).

### 4.3 Services & Apps
- Curated service list from an allowlist file per platform image — never enumerate all units. Per service: status, enable-on-boot toggle, start/stop/restart (via zbus → systemd), and the last ~20 lines of `journalctl -u` inline when failed.
- The **schema-driven app settings engine** described in §3 — this is the centerpiece.
- MQTT/cloud panel: broker host/port, TLS toggle, auth (username/password or client cert selected from the Security cert store), client ID, base topic, live connection indicator, "test connection" button.
- Logging config lives in the Logs section (below), not here.

### 4.4 Updates & Maintenance
- Integrate with **RAUC** (assume RAUC; keep the integration behind a small trait so SWUpdate could substitute). A/B slots; U-Boot on ARM platforms, GRUB/systemd-boot on Sequoia.
- Browser upload of a signed bundle → signature verification → **compatibility check** (bundle declares platform/variant; refuse mismatches — an Arches bundle must never flash onto an Acadia) → write inactive slot → honest staged progress (verify/write/done) → "reboot to apply".
- Slot status display: "Slot A: v2.3 (running, confirmed) / Slot B: v2.2", including rollback state. New images require confirmation (health-check or manual) or the bootloader rolls back after N failed boots.
- OTA: server URL + "check now" + notify-only in v1. **[v2]** auto-apply, maintenance windows.
- Backup/restore: download the config store as a file; restore validates schema version and tolerates version skew (drop unknown fields gracefully). **Secrets excluded by default**, or included only inside a passphrase-encrypted backup.
- **Diagnostics bundle button:** tarball of journal excerpt, config store (secrets redacted), versions, slot status, dmesg, network state, alert history, audit log.
- Factory reset duplicated here (same action as System).
- Explicitly do NOT implement package-level (opkg/apt) updates.

### 4.5 Monitoring
- **No resident collectors, no Prometheus/Grafana.** Backend reads `/proc`, `/sys` on request; browser polls a JSON endpoint every few seconds only while the page is open.
- Vitals: CPU load + sparkline, memory (show *available*, not free), disk per mount, **temperature** from `/sys/class/thermal` + hwmon, thermal-throttle state on Arches.
- eMMC wear from lifetime registers (`mmc extcsd`) where present.
- Network summary + connectivity test (shared with Networking), per-interface traffic counters from `/sys/class/net/*/statistics`. No live bandwidth graphs.
- Service health strip (green/yellow/red) linking to Services. Uptime, boot count, last boot reason where the platform/MCU reports it.
- **Platform tiles via manifest/schema plugin:** Arches (GPU util/mem, power mode, fan RPM), Zion (loaded bitstream, PL status), Joshua (PRU state), Acadia/Arches (companion MCU heartbeat + fw version), Sequoia (standard sensors).

### 4.6 Logs
- Journal viewer: filter by unit, priority, time range; **follow mode via SSE**; last-N paging. No regex/search builder.
- Download: full journal export and per-unit export.
- Log configuration: journald persistent vs volatile (default volatile; persistent option size-capped — eMMC wear), max journal size, per-service log level where the service's schema declares one, remote syslog destination (host/port/protocol, one destination).

### 4.7 Alerts
- Threshold rules: metric (temp, disk %, memory, service state, MCU heartbeat) + comparison + **duration** ("temp > 85 for 60 s") to prevent flapping.
- Actions: UI red-state + event log (always); webhook POST; MQTT event topic. **[v2]** email.
- Alert history: fired/cleared ring-buffer file with timestamps, viewable + exportable.
- Per-platform default rule packs shipped as schema files (Arches: throttling; all: disk-full, watchdog-reset).
- Engine = rule evaluation on the same data the dashboard reads, on a timer inside the daemon. No extra process.

### 4.8 Security
- Auth: the §4.1 user store; sessions as httpOnly+Secure cookies, configurable idle timeout (default 15 min), **CSRF protection on every mutating route**, incremental delay on failed logins.
- **First-boot flow: no default password** — force admin password creation on first access (CRA/SB-327 compliance).
- TLS: per-device self-signed cert auto-generated on first boot via rcgen (never a shared baked-in cert); upload of customer cert/key; CSR generation. HTTP→HTTPS redirect on by default; plain HTTP only as explicit admin opt-in.
- Device-wide certificate store: CA + client certs for MQTT/VPN, uploaded here, referenced elsewhere; expiry dates shown; "expires < 30 days" feeds the alert engine.
- **WireGuard only:** on/off, on-device keypair generation (private key never leaves the device; display public key only), peer list, allowed IPs. No OpenVPN/IPsec.
- Exposure summary panel: one glanceable statement of currently reachable services per interface (derived from firewall + service state).
- Trust status (read-only): secure boot state, update signing key ID, TPM presence, verity/encryption status — read from the BSP platform-status file. Never configure secure boot from the UI.
- **Audit log:** append-only ring file of all config-UI actions (user, action, timestamp), viewable under Logs, included in the diagnostics bundle.

### 4.9 I/O & Peripherals
- Everything rendered from the **board manifest** — zero hardcoded I/O.
- Serial ports: mode (RS-232/RS-485 where switchable), baud/format, and exclusive **ownership assignment** (console / customer app / Modbus service / unassigned) with conflict prevention.
- **CAN** (lives here, not Networking): per interface bitrate, CAN-FD + data bitrate, listen-only, restart-ms, bring-up on boot (generated systemd unit doing `ip link set … type can …`); live bus state (error-active/passive/bus-off), error counters, last-10-frames peek; termination on/off where manifest declares GPIO-switchable termination.
- GPIO: manifest-**named** pins only (never raw banks): direction, initial state, active-low, live value, manual toggle for outputs.
- I2C: manifest-declared devices + admin-only bus scan button. SPI: display only.
- **Companion MCU panel** (Arches STM32, Acadia Pico W): fw version, heartbeat, **signed firmware upload + flash** (same signing/compat rules as OS bundles), reset button; MCU-side settings rendered via the same schema engine if the protocol exposes them.
- Platform panels: **Zion** — active bitstream, signed bitstream upload, load timing (boot vs on-demand); **Joshua** — PRU firmware selection per core, start/stop; **Arches** — camera enumeration, nvpmodel power mode.
- **[v2]** Display/HMI settings (resolution, rotation, brightness, blank timeout).

## 5. Yocto integration

Generate a `meta-soccentric-webconfig` layer containing:
- Daemon recipe (`inherit cargo`, vendored crate tarball with checksums), static-assets recipe, systemd unit (non-root user, hardening directives: `ProtectSystem`, `NoNewPrivileges`, capability bounding), polkit rules for the privileged D-Bus operations.
- A `webconfig-schemas` install directory convention other recipes use to register app settings, dashboard tiles, and alert packs.
- Per-machine board-manifest generation hook and per-platform service allowlist files.
- SPDX/SBOM generation enabled.

## 6. Testing

- Config engine + all appliers: unit/integration tests against a fake sysroot tempdir, runnable on x86 CI with no hardware.
- CI builds all three cross-targets on every commit.
- API tests for auth, CSRF, role enforcement, and the commit/confirm rollback timer.

## 7. Deliverables and output format

Produce, in order:
1. Repository tree overview.
2. `Cargo.toml` with the exact release profile.
3. Backend source, organized by the three layers, with the applier trait and at least these appliers fully implemented: hostname, time/NTP, networkd (with commit/confirm), firewall/nftables, CAN, journald, WireGuard.
4. The schema format specification + the generic frontend schema→form renderer.
5. Board manifest format specification + one example manifest per platform.
6. Frontend: all nine pages, the SSE client, the polling dashboard, mobile-width layout.
7. REST API reference (routes, methods, roles required, request/response JSON).
8. The Yocto layer (recipes, unit file, polkit rules).
9. Test suite skeleton with the fake-sysroot harness and representative tests.

Where a real system interaction cannot be fully implemented (e.g., RAUC D-Bus specifics), implement against a narrow trait and provide both the real implementation and a mock — never silently stub with a TODO. State any assumption you make in one line where you make it.