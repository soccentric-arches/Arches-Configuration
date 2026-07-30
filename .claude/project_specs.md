# Arches-Configuration — specs

Parent project: Arches (target hardware: NVIDIA Jetson, Tegra family, L4T/JetPack OS, with an STM32 real-time co-processor)
Leaf type: `<Project>-Configuration`

## What this is

This repo is the home of WebConfig, the Arches on-device UI: one Rust daemon + vanilla-JS frontend + engine, with board-specific data isolated to a single board-manifest file (`board-manifests/arches.json`). See `docs/webconfig-spec.md` for the full spec.

## Architecture notes

* **Secrets/backup** — `POST /updates/backup` with `{ "passphrase" }` returns a passphrase-encrypted envelope (Argon2id key derivation + ChaCha20-Poly1305 AEAD via `ring`, base64 fields). `GET /updates/backup` always redacts secrets. Restore detects `.encrypted` in the uploaded file and prompts for the passphrase.
* **Config backup snapshot** — `config_state_backup` table holds the previous committed row; `commit()` snapshots then upserts inside one `BEGIN`/`COMMIT`.
* **Testing pattern** — integration tests for the binary crate `webconfigd` live as `#[cfg(test)] mod tests;` in `src/main.rs` → `src/tests.rs` (binary crates have no `lib` target for external `tests/` to import from). Drive the axum router with `tower::ServiceExt::oneshot` + `http_body_util::BodyExt` against a tempdir `Sysroot`.
* **Real vs mock system interaction** — system interactions sit behind narrow traits with both real and mock impls (`UpdateBackend`, `SystemdClient`); `make_backend`/`make_client` pick real vs mock from `Sysroot::is_real_root()`.
* **Frontend** — native ES modules (`<script type="module">`), one file per component/page under `frontend/{core,components,pages}/`, no bundler. Add a page by dropping a module in `frontend/pages/` plus a line in `frontend/pages/registry.js`. Styles split under `frontend/styles/*.css`, composed by `main.css` via plain `@import`.
* **System config appliers** — `system.apply` runs hostname, time, ssh appliers in order (spec section-4.1); `network.apply` runs networkd, firewall, wifi, ap appliers (spec section-4.2), each validated + applied explicitly, with `network.rollback` rolling all four back.
* **NTP** — `config.system.ntp_servers` (not under `config.system.time`) is written to a `systemd-timesyncd.conf.d/webconfig.conf` drop-in with a `FileBackup` for rollback.
* **Wi-Fi / AP** — `config.network.wifi` and `regulatory_domain` are applied by `appliers/wifi.rs` to `wpa_supplicant.conf` + `iw reg set`; `config.network.ap_mode` is applied by `appliers/ap.rs` to `hostapd.conf` + `dnsmasq.d` + a static `20-ap.network`.
* **DNS** — global `config.network.dns_servers` is applied by the networkd applier to `/etc/systemd/resolved.conf.d/webconfig.conf`; `connectivity_test` pings the configured gateway, then a public IP, then resolves via the configured DNS.
* **App settings hooks** — `PUT /apps/:id/settings` fires the app schema's `apply_hooks` after commit (restart / reload / sighup), looked up by schema file stem, best-effort and audited.
* **Updates** — `UpdateBackend::mark_good()` (RAUC `rauc status mark-good`) is exposed at `POST /api/v1/updates/confirm` (admin only, audited). The diagnostics bundle carries journal + network state (via the shared `network::enumerate_live(&Sysroot)`) alongside config/slots/dmesg/alert_history/audit.
* **Monitoring** — Vitals carries `load[3]`, `mem_available_kb`, mounts, temperatures (from `thermal_zone*` and `hwmon*/temp*_input`), per-iface net bytes, uptime, boot_count, `last_boot_reason`. `TempZone.throttled` is read from `cdev*/cur_state`, never hardcoded.
* **Logs** — `/events` SSE emits both vitals and journal-follow chunks (cursor-based, polled every 2s because the daemon is `current_thread`). `/logs/download` accepts `?unit=`. The journald applier materializes remote syslog as `webconfig-syslog-forward.service`. The audit log is append-only — there is no delete route; only a full factory reset wipes it.
* **Typography** — fully tokenized (`--text-*`, `--weight-*`, `--leading-*`, `--tracking-*` in `frontend/styles/tokens.css`); `--primary` (#0ea5e9) is the single accent color.

## Architecturally distinct from other -Configuration leaves

Arches-Configuration ("SoCcentric WebConfig") is a small Rust daemon + vanilla-JS frontend, not the Python/FastAPI/Vue "Embedded Linux Device Dashboard" used by the other 8 projects' `-Configuration` leaves. The two approaches coexist unreconciled across the fleet — don't port fleet-wide conventions from one to the other without checking. `REPO_MAP.md` in this repo documents the distinction.
