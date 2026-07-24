# WebConfig REST API reference

Base URL: `https://<device>:8443/api/v1`. All responses are JSON.

## Authentication & authorization

- **Sessions**: httpOnly + Secure cookies via `tower-sessions`. Idle timeout
  configurable (default 15 min, `security.session_idle_timeout_secs`).
- **First-boot gate**: until an admin is created via `POST /auth/first-boot`,
  every protected route returns `401` and `POST /auth/login` returns `503`.
- **Roles**: `admin` (all actions), `operator` (mutate apps/services), `viewer`
  (read-only). Admin-only actions are marked **[admin]** below; service/app
  mutations are **[operator+]**.
- **CSRF**: every non-safe method (POST/PUT/PATCH/DELETE) on protected routes
  requires `X-CSRF-Token: <token>`, where the token is the `csrf` field returned
  by `GET /auth/whoami` (SHA-256 of the session id). Missing/invalid → `400`.

## Conventions

- Errors: `{ "error": "<message>" }` with the status below.
- Status codes: `200`, `201`, `400` (bad request / CSRF), `401` (no session),
  `403` (forbidden), `404`, `409` (conflict), `422` (engine/validation),
  `500`, `503` (not ready / first-boot pending).

## Auth (public)

| Method | Path | Role | Body / Notes |
|---|---|---|---|
| POST | `/auth/first-boot` | — | `{username, password}` — create the first admin + log in. Only when no users exist. |
| POST | `/auth/login` | — | `{username, password}` → `{username, role}`. Throttled (exponential delay). |
| POST | `/auth/logout` | any | clears the session. |
| GET | `/auth/whoami` | — | `{username, role, csrf, first_boot}`. CSRF token source. |

## Meta

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/manifest` | any | the board manifest (platform, peripherals, tiles, service_allowlist, trust flags). |
| GET | `/schemas` | any | all schema descriptors `[{id, descriptor}]`. |
| GET | `/schemas/:section` | any | one descriptor `{id, descriptor}` for the renderer. |

## System (§4.1)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/system` | any | `SystemConfig` (hostname, asset_tag, hardware_revision, serial_number, time, ssh, ntp_servers). `time` includes timezone, NTP, PTP, GPS/GNSS, and manual datetime settings. |
| PUT | `/system` | **[admin]** | `SystemConfig` → saved section. NTP, PTP, and GPS cannot be enabled together. |
| POST | `/system/apply` | **[admin]** | apply hostname + time appliers. |
| GET | `/system/info` | any | platform, model, board_name, has_rtc, os_release, uptime. |
| POST | `/system/reboot` | **[admin]** | reboot. |
| POST | `/system/shutdown` | **[admin]** | power off. |
| POST | `/system/factory-reset` | **[admin]** | `{confirm:"RESET", mode:"settings"|"settings+users"}`. |
| POST | `/system/root-password` | **[admin]** | `{password}` (min 8) via `chpasswd`. |
| GET | `/system/users` | any | `[{username, role}]`. |
| POST | `/system/users` | **[admin]** | `{username, password, role}`. |
| DELETE | `/system/users/:name` | **[admin]** | last-admin guarded. |
| PUT | `/system/users/:name/role` | **[admin]** | `{role}`. |
| PUT | `/system/users/:name/password` | **[admin]** | `{password}` (min 8). |

## Networking (§4.2 — flagship commit/confirm)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/network` | any | `{config: NetworkConfig, live: [{name, mac, operstate}]}`. Interface config may include `mac_address` (`AA:BB:CC:DD:EE:FF`) to assign a persistent MAC through systemd-networkd. |
| PUT | `/network` | **[admin]** | `NetworkConfig` → saved section (does not apply). Wired links are in `interfaces`; Wi-Fi networks are in `wifi`; the optional 5G profile is in `cellular_5g` with `enabled`, modem, APN, SIM, credential, roaming, and auto-connect fields. Firewall `default_open_ports` and interface `open_ports` accept `{port, protocol}` entries for custom TCP/UDP ports. |
| POST | `/network/apply` | **[admin]** | apply networkd + firewall → `{provisional: bool, confirm_window_secs}`. Provisional ⇒ confirm or auto-rollback. |
| POST | `/network/confirm` | **[admin]** | confirm the provisional change → `{confirmed}`. |
| POST | `/network/rollback` | **[admin]** | roll back the provisional change. |
| GET | `/network/scan` | any | Wi-Fi scan `[{ssid, signal_dbm}]`. |
| POST | `/network/connectivity-test` | any | `{gateway:{ok,detail}, public:{ok,detail}, dns:{ok,detail}}`. |

## Services & Apps (§4.3)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/services` | any | `[{unit, status:{unit,load,active,sub,enabled}}]` (manifest allowlist only). |
| GET | `/services/health` | any | `[{unit, state}]` health strip. |
| POST | `/services/:unit/start` | **[op+]** | |
| POST | `/services/:unit/stop` | **[op+]** | |
| POST | `/services/:unit/restart` | **[op+]** | |
| PUT | `/services/:unit/enabled` | **[op+]** | `{enabled: bool}` — enable-on-boot. |
| GET | `/services/:unit/journal` | any | `{unit, tail}` last ~20 lines. |
| GET | `/services/mqtt` | any | `MqttConfig`. |
| PUT | `/services/mqtt` | **[op+]** | `MqttConfig`. |
| POST | `/services/mqtt/test` | any | `{reachable: bool}`. |
| GET | `/apps/:id/settings` | any | the app settings slice (schema-driven). |
| PUT | `/apps/:id/settings` | **[op+]** | write a schema-driven settings slice (server re-validates). |

## Updates (§4.4)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/updates/slots` | any | `[{slot, version, running, confirmed, state}]`. |
| POST | `/updates/install` | **[admin]** | `{bundle_path}` → signature verify + compat check + write inactive slot → `{manifest, stages, reboot_to_apply}`. |
| POST | `/updates/reboot-to-apply` | **[admin]** | boot into the newly written slot. |
| GET | `/updates/ota` | any | Read OTA server configuration. |
| PUT | `/updates/ota` | **[admin]** | Save OTA server configuration. |
| GET | `/updates/ota/check` | any | Query the configured server for `{update_available, version, platform}` JSON. |
| GET | `/updates/backup/config` | any | Read `{enabled, server_url, backup_enabled, restore_enabled}`. |
| PUT | `/updates/backup/config` | **[admin]** | Save remote backup connection and local feature flags. |
| GET | `/updates/backup/check` | any | Test the configured backup server; returns `{checked, connected, reason}`. |
| GET | `/updates/backup?include_secrets=false` | any | config store (secrets redacted by default). |
| POST | `/updates/restore` | **[admin]** | restore a backup (tolerates version skew). |
| GET | `/updates/diagnostics` | any | journal excerpt + config + slots + audit + dmesg + alert history. |
| POST | `/updates/factory-reset` | **[admin]** | same as `/system/factory-reset`. |

## Monitoring (§4.5)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/monitoring/vitals` | any | `Vitals` (load, mem available, mounts with `physical` flags, swap totals/free bytes, temperatures from `/sys/class/thermal` + hwmon with throttle flag, per-iface net counters, uptime, boot_count, last_boot_reason). |
| GET | `/monitoring/emmc/:dev` | any | `{dev, life_time_estimate}`. |
| GET | `/monitoring/tiles` | any | manifest-declared dashboard tiles. |
| GET | `/monitoring/telemetry` | any | Read `{enabled, server_url, username, password, publish_interval_secs, include_diagnostics}`. |
| PUT | `/monitoring/telemetry` | **[admin]** | Save telemetry publishing settings and the telemetry server credentials. |

## Logs (§4.6)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/logs?unit=&priority=&since=&until=&limit=` | any | `{lines: [...]}`. |
| GET | `/logs/sources` | any | Available local sources: system journal, kernel, syslog, audit, and WebConfig daemon. |
| GET | `/logs/config` | any | `LogsConfig`. |
| PUT | `/logs/config` | **[admin]** | `LogsConfig` (applies journald). |
| GET | `/logs/audit` | any | `[{ts, user, action, detail}]`. Append-only (spec §4.8) — no `DELETE` route exists; only a full factory reset wipes audit. |
| GET | `/logs/download` | any | full journal export, or per-unit export with `?unit=` (spec §4.6). |

## Alerts (§4.7)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/alerts` | any | `{enabled, rules, default_rules, webhooks, mqtt_event_topic, history}`. `default_rules` are the read-only per-platform rule packs shipped in the board manifest (Arches: throttling; all: disk-full, watchdog-reset) — merged with `rules` at evaluation time; override/disable a default by adding a same-id rule. |
| PUT | `/alerts` | **[admin]** | `AlertsConfig` (rules validated). Each rule carries `actions` (`ui`, `webhook`, `mqtt`); fired/cleared events are POSTed to each `webhooks` URL and published to `<mqtt_event_topic>/<rule_id>` on the configured MQTT broker. |
| GET | `/alerts/history` | any | `[{ts, rule_id, state, value}]` — ring-buffer, viewable + exportable (also included in `/updates/diagnostics`). |

## Security (§4.8)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/security` | any | `SecurityConfig` (idle timeout, plain-HTTP opt-in, wireguard, certificates). |
| PUT | `/security` | **[admin]** | `SecurityConfig`. |
| POST | `/security/apply` | **[admin]** | apply WireGuard. |
| GET | `/security/wireguard/pubkey` | any | `{public_key, has_private_key}`. |
| POST | `/security/csr` | **[admin]** | `{cn}` → `{csr, private_key}`. |
| POST | `/security/tls` | **[admin]** | `{cert, key}` (PEM pair) → `{ok, restart_required}`. Replaces the device TLS identity (`<data>/webconfig/cert.pem`+`key.pem`). The pair is validated by building a real `rustls` `ServerConfig` first — a cert/key mismatch or unsupported key is rejected (400) **before** any file is written, so a bad upload can never brick the listener on the next boot. The key is written mode 0600. The running listener holds the prior cert in memory, so the new identity takes effect on the next daemon restart (`restart_required: true`). |
| GET | `/security/exposure` | any | per-interface open services. |
| GET | `/security/trust` | any | secure boot / TPM / verity / encryption status (read-only). |
| GET | `/security/certs` | any | `[{id, kind, subject, not_after}]`. |
| POST | `/security/certs` | **[admin]** | `{id, kind, subject, not_after, pem}` → `CertMeta`. Validates the PEM parses (contains ≥1 cert via rustls_pemfile), writes `<data>/webconfig/certs/<id>.pem`, upserts metadata into `config.security.certificates`. `id` is alnum/-/_ only (path-traversal safe). `kind` is `ca`\|`client`; `not_after` is ISO-8601. |
| DELETE | `/security/certs/:id` | **[admin]** | remove the PEM + metadata; 404 if absent. |

## I/O (§4.9)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/io` | any | `{manifest, config, tiles}`. `config.board_leds` contains system, OS, hardware, and optional application LED assignments. |
| PUT | `/io` | **[admin]** | `IoConfig` (rejects serial not in manifest; owner-conflict guard). `serial_enabled`, `can_enabled`, `gpio_enabled`, and `cameras_enabled` gate their subsystem detail settings. Each serial, CAN, GPIO, and camera entry also has an `enabled` field. `board_leds` controls the automatic status policy. |
| POST | `/io/can/apply` | **[admin]** | apply CAN. |
| POST | `/io/gpio/:name/toggle` | **[op+]** | manifest-named pins only. |
| POST | `/io/i2c/scan?bus=` | **[admin]** | `{bus, addresses}`. |
| GET | `/io/mcu` | any | companion MCU panel. |
| POST | `/io/mcu/firmware` | **[admin]** | signed firmware upload (version, signed). |
| GET | `/io/platform-panel` | any | Zion FPGA / Joshua PRU / Arches cameras. |

Feature configuration endpoints expose an explicit `enabled` field before their
detail settings are applied. Alert rules also carry their own `enabled` field.
This includes MQTT cloud connectivity, alerting,
firewall policy, OTA updates, and remote syslog. Existing firewall
configurations default to enabled; the optional services default to disabled
when no explicit state was previously stored.

## SSE

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/events` | any | Server-Sent Events: `vitals` (snapshot) + `journal` follow chunks (cursor-based, real root only, spec §4.6 follow mode). |

> **[admin]** = admin only. **[op+]** = operator or admin. "any" = any
> authenticated session. No route is usable unauthenticated except the public
> auth routes.