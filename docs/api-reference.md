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
| GET | `/system` | any | `SystemConfig` (hostname, asset_tag, hardware_revision, serial_number, time, ssh, ntp_servers). |
| PUT | `/system` | **[admin]** | `SystemConfig` → saved section. |
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
| PUT | `/network` | **[admin]** | `NetworkConfig` → saved section (does not apply). |
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
| GET | `/updates/backup?include_secrets=false` | any | config store (secrets redacted by default). |
| POST | `/updates/restore` | **[admin]** | restore a backup (tolerates version skew). |
| GET | `/updates/diagnostics` | any | journal excerpt + config + slots + audit + dmesg + alert history. |
| POST | `/updates/factory-reset` | **[admin]** | same as `/system/factory-reset`. |

## Monitoring (§4.5)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/monitoring/vitals` | any | `Vitals` (load, mem, mounts, temperatures, net, uptime). |
| GET | `/monitoring/emmc/:dev` | any | `{dev, life_time_estimate}`. |
| GET | `/monitoring/tiles` | any | manifest-declared dashboard tiles. |

## Logs (§4.6)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/logs?unit=&priority=&since=&until=&limit=` | any | `{lines: [...]}`. |
| GET | `/logs/config` | any | `LogsConfig`. |
| PUT | `/logs/config` | **[admin]** | `LogsConfig` (applies journald). |
| GET | `/logs/audit` | any | `[{ts, user, action, detail}]`. |
| DELETE | `/logs/audit` | **[admin]** | Clear the persisted audit log. |
| GET | `/logs/download` | any | full journal export. |

## Alerts (§4.7)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/alerts` | any | `{rules, webhooks, mqtt_event_topic, history}`. |
| PUT | `/alerts` | **[admin]** | `AlertsConfig` (rules validated). |
| GET | `/alerts/history` | any | `[{ts, rule_id, state, value}]`. |

## Security (§4.8)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/security` | any | `SecurityConfig` (idle timeout, plain-HTTP opt-in, wireguard, certificates). |
| PUT | `/security` | **[admin]** | `SecurityConfig`. |
| POST | `/security/apply` | **[admin]** | apply WireGuard. |
| GET | `/security/wireguard/pubkey` | any | `{public_key, has_private_key}`. |
| POST | `/security/csr` | **[admin]** | `{cn}` → `{csr, private_key}`. |
| GET | `/security/exposure` | any | per-interface open services. |
| GET | `/security/trust` | any | secure boot / TPM / verity / encryption status (read-only). |
| GET | `/security/certs` | any | `[{id, kind, subject, not_after}]`. |

## I/O (§4.9)

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/io` | any | `{manifest, config, tiles}`. |
| PUT | `/io` | **[admin]** | `IoConfig` (rejects serial not in manifest; owner-conflict guard). |
| POST | `/io/can/apply` | **[admin]** | apply CAN. |
| POST | `/io/gpio/:name/toggle` | **[op+]** | manifest-named pins only. |
| POST | `/io/i2c/scan?bus=` | **[admin]** | `{bus, addresses}`. |
| GET | `/io/mcu` | any | companion MCU panel. |
| POST | `/io/mcu/firmware` | **[admin]** | signed firmware upload (version, signed). |
| GET | `/io/platform-panel` | any | Zion FPGA / Joshua PRU / Arches cameras. |

## SSE

| Method | Path | Role | Notes |
|---|---|---|---|
| GET | `/events` | any | Server-Sent Events: `vitals` (snapshot) + `heartbeat` ticks. |

> **[admin]** = admin only. **[op+]** = operator or admin. "any" = any
> authenticated session. No route is usable unauthenticated except the public
> auth routes.