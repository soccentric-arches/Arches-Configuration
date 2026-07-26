# WebConfig Checkpoint — Acceptance Criteria for MasterPlan.md

Purpose: verifiable, check-off acceptance criteria for every item in `MasterPlan.md`. An implementing agent/engineer should not mark an item `[x]` until the stated **Verify** step actually passes — checking the box without running the verification is not acceptable. Do not edit `MasterPlan.md` to mark things done; update this file instead, and leave a one-line note of what changed (file/commit) next to each checked item.

Global exit gate (must pass at the end of every phase, and again at the very end):
```
make build && make test && make lint
```
No item in this document is considered done if this gate is failing.

---

## Phase 0 — P0 Blockers (must all be checked before any field deployment)

- [x] **0.1 MCU firmware upload/flash is real, not a no-op**
  Where: `crates/webconfigd/src/api/io.rs` (`mcu_firmware` handler), `frontend/pages/mcu.js`.
  Acceptance:
  - [x] A real file-upload path exists (multipart or equivalent) that lands firmware bytes on disk — no longer just `{version, signed}` JSON.
  - [x] The server cryptographically verifies the firmware signature using the same trust chain as `update_backend.rs::verify_bundle` (or an equivalent, explicitly documented mechanism) — a client-supplied `signed: true` boolean is never trusted on its own.
  - [x] An actual flash operation is invoked on real hardware; on the fake sysroot it clearly simulates without claiming a real flash occurred.
  - [x] If flashing is genuinely out of scope for this release, the route/UI explicitly returns/displays "not implemented" rather than `{"flashed": true}`.
  - Verify: `cargo test mcu_firmware_verifies_real_signature_and_rejects_tampering` — real
    Ed25519 keypair generated via `ring`; tampered bytes and a garbage signature are both
    rejected with 400; a real signature over the actual bytes succeeds, writes to
    `<data>/webconfig/mcu-firmware/<version>.bin`, and the fake sysroot response reports
    `flashed: false` with a `"simulated"` reason (never a fabricated `true`). Design note:
    used a raw-bytes upload (`application/octet-stream` body + `?version=&signature=` query)
    instead of multipart, to avoid adding axum's `multipart` feature (a new vendored
    dependency) — see `docs/api-reference.md` and `docs/board-manifest.md` (new
    `caps.firmware_pubkey_ed25519` / `caps.flash_command` manifest fields) for the contract.
    No `flash_command` is declared for Arches yet, so real-root flashing explicitly reports
    "not implemented" until BSP wires one up. `make build && make test && make lint` green.

- [x] **0.2 Service start/stop/restart/enable report the real outcome**
  Where: `crates/webconfigd/src/api/services.rs` (`start`, `stop`, `restart`, `set_enabled`).
  Acceptance:
  - [x] `.ok()` / swallowed `Result` on the systemd call is removed from all four handlers.
  - [x] A failed systemd operation returns a non-200 `ApiError`, not `{"ok": true}`.
  - [x] The audit log entry reflects actual success/failure, not just "attempted."
  - Verify: `cargo test services_verbs_report_real_failure_not_fake_ok` (commit 0f49bb7) — a
    `FailingSystemd` test double drives all four verbs, asserts HTTP 422 on each, and asserts
    the audit log records "failed" for all 4 `services.*` entries. `make build && make test &&
    make lint` green.

- [x] **0.3 `/security/trust` no longer permanently hardcodes unknown values**
  Where: `crates/webconfigd/src/api/security.rs` (`trust` handler).
  Acceptance:
  - [x] `update_signing_key_id`, `verity_status`, `encryption_status` are either read from a real BSP platform-status source, or the API/UI explicitly marks them "not available on this platform" (a distinct signal from a value that failed to read).
  - [x] No field silently reports `"unknown"`/`null` while looking like a completed read.
  - Verify: `cargo test trust_status_distinguishes_unavailable_from_real_read` (commit 2ad4fb2)
    — asserts `platform_status_available: false` + null fields with no `platform-status.json`
    present, then writes a real file and asserts `platform_status_available: true` with the
    real values populated. `frontend/pages/secure-boot.js` renders a distinct "not available
    on this platform" badge instead of concatenating `${undefined}`. `make build && make test
    && make lint` green.

- [x] **0.4 `secret`-typed app-schema fields are redacted in backup/diagnostics**
  Where: `crates/webconfigd/src/api/updates.rs` (`redact_secrets`).
  Acceptance:
  - [x] `redact_secrets` walks `cfg.app_settings` using each app's loaded schema and blanks any field typed `secret`, not just a hardcoded top-level `Config` field list.
  - [x] A regression test defines an app schema with a `secret`-typed field, sets a value, and asserts the value never appears in `GET /updates/backup` (plaintext) or `GET /updates/diagnostics` output.
  - Verify: `cargo test app_schema_secret_field_never_appears_in_backup_or_diagnostics`
    (commit cf5718b) — plants a secret via an app schema, asserts the raw response body
    of both routes never contains the planted string, and that the field renders as
    `"[redacted]"` while a sibling non-secret field survives untouched. `make build &&
    make test && make lint` green.

- [ ] **0.5 Yocto layer installs the real Arches board manifest**
  Where: `meta-webconfig/recipes-core/webconfig/webconfig-manifest.bb` + `.bbappend`, `meta-webconfig/recipes-core/webconfig/files/`.
  Acceptance:
  - [ ] The real Arches manifest (`arches.json`) is present in the layer, in the format the manifest loader actually expects (see 0.6).
  - [ ] The `.bbappend` is wired to the real Arches manifest file, not left commented out.
  - [ ] Building with no matching manifest fails the build loudly, rather than silently substituting `generic.yaml`.
  - Verify: run (or simulate) a bitbake config check / recipe parse and confirm it resolves to the Arches manifest file, not the generic fallback.

- [ ] **0.6 Board-manifest format is consistent across docs, code, and Yocto recipe**
  Where: `docs/board-manifest.md`, `crates/webconfigd/src/bootstrap.rs`, `meta-webconfig/recipes-core/webconfig/webconfig-manifest.bb`, `board-manifests/*.json`.
  Acceptance:
  - [ ] One canonical format (JSON or YAML) and one canonical on-device path is chosen.
  - [ ] `docs/board-manifest.md` matches the real recipe and the real loader exactly (path, format, variable names).
  - [ ] The dead code path (whichever format was not chosen) is removed from `bootstrap.rs`'s fallback chain, or explicitly documented as an intentional legacy fallback with a reason.
  - Verify: follow `docs/board-manifest.md` step-by-step from a clean checkout to produce a running device config — it must work exactly as documented, no format/path mismatches.

**Phase 0 gate:** all six boxes above checked, global exit gate passing, and a fresh `make build && make test && make lint` run attached/logged.

---

## Phase 1 — P1 Engine + Daemon items

- [ ] **1.1 GPIO applier exists and is registered**
  Where: new `crates/webconfig-engine/src/appliers/gpio.rs`, `appliers/mod.rs::all()`.
  Acceptance: validate/apply/rollback + `FileBackup` pattern matching `can.rs`; direction/active_low/initial-state for named GPIO pins and board LEDs materialize to a real boot-time mechanism (udev rule or oneshot unit) on the real sysroot; registered in `appliers::all()`.
  Verify: unit tests mirroring `can.rs`'s test suite (validation rejection cases + apply/rollback round trip against the fake sysroot); manual check that `appliers::all()` includes the new applier.

- [ ] **1.2 RAUC variant compatibility check works on real hardware**
  Where: `crates/webconfig-engine/src/update.rs::parse_manifest_from_rauc_info`.
  Acceptance: the real backend parses an actual variant field from `rauc info` (or equivalent RAUC bundle metadata), no longer hardcodes `variant: None`.
  Verify: a test using real `rauc info`-shaped text (not just the mock backend) that proves an Arches `agx` bundle is rejected against an Arches `orin-nano` target.

- [ ] **1.3 `network.cellular_5g` is resolved one way or the other**
  Acceptance: either (a) a real applier exists (ModemManager/mmcli-backed) with validate/apply/rollback, or (b) `FiveGConfig` is explicitly demoted to the same `serde_json::Value` v2-deferred treatment as `vlan`/`bridges`/`cellular`, and the API/schema/docs reflect that it's not active in v1.
  Verify: if (a), a full applier test suite; if (b), confirm the field can no longer be set via `PUT /network` expecting it to take effect, and docs say so.

- [ ] **1.4 OS update bundle browser upload works end-to-end**
  Where: `crates/webconfigd/src/api/updates.rs`.
  Acceptance: a real multipart upload route writes to `/data/webconfig/uploads/` (or equivalent) and the existing install-from-path logic is invoked afterward.
  Verify: an integration test that uploads a bundle via HTTP multipart and confirms it reaches the install path.

- [ ] **1.5 RAUC-unavailable fallback is never silent on a real device**
  Where: `crates/webconfigd/src/update_backend.rs::make_backend`.
  Acceptance: falling back to the mock backend when `is_real_root()` is true and `rauc` is unavailable logs at `warn!` or higher; ideally the API also surfaces a "update backend unavailable" state rather than serving fabricated slot data.
  Verify: test that simulates a real root with `rauc` missing and asserts a warning is logged / a degraded-state flag is set.

- [ ] **1.6 Factory reset surfaces and handles partial failure**
  Where: `crates/webconfigd/src/api/system.rs::factory_reset`.
  Acceptance: SQL errors during reset are logged (`tracing::error!`), and the reboot step is either skipped or the response clearly indicates a partial/failed reset rather than presenting it as a clean success.
  Verify: test with a deliberately locked/corrupt `config.db` and confirm the response and logs reflect the failure, and the device does not silently reboot claiming success.

- [ ] **1.7 First-boot admin audit entry uses the shared ISO-8601 timestamp helper**
  Where: `crates/webconfigd/src/auth.rs`.
  Acceptance: the local stale `now_iso()` is deleted; `first_boot`'s audit call uses `crate::api::now_iso()`.
  Verify: a test asserting the first-boot audit entry's `ts` field matches RFC3339 format, not `t=<epoch>`.

- [ ] **1.8 Request body size limits are enforced**
  Where: `crates/webconfigd/src/routes.rs`.
  Acceptance: `tower_http::limit::RequestBodyLimitLayer` (or equivalent) applied at minimum to `/security/tls`, `/security/certs`, `/updates/install`, `/updates/restore`, `/updates/backup`, sized against the 15 MB RAM budget in spec §2.
  Verify: a test sending an oversized body to at least one of these routes and asserting rejection (413 or equivalent) rather than an OOM/hang.

- [ ] **1.9 Malformed compiled-in board manifest fails loudly**
  Where: `crates/webconfigd/src/bootstrap.rs::load_or_seed_manifest`.
  Acceptance: a parse failure on the compiled-in fallback manifest logs at `error!` level (matching the schema loader's existing `warn!` pattern), and ideally the daemon surfaces a degraded-manifest signal to the UI.
  Verify: test with a deliberately malformed compiled-in manifest asserting a log line is emitted.

- [ ] **1.10 Provisional network-apply failure state is visible to the operator**
  Where: `crates/webconfigd/src/api/network.rs::apply`.
  Acceptance: if `networkd` goes `Provisional` and a later applier (firewall/wifi/ap) fails, the error response includes an indication that a pending rollback timer is active.
  Verify: test that forces a downstream applier to fail after `networkd` succeeds provisionally, and asserts the error response includes provisional-state info.

**Phase 1 gate:** all ten boxes checked, global exit gate passing.

---

## Phase 2 — P1 Frontend items

- [ ] **2.1 Serial config apply is wired**
  Where: `frontend/pages/serial.js`.
  Acceptance: an "Apply serial" action calls `POST /api/v1/io/serial/apply` after save; UI reflects apply success/failure separately from save success/failure.
  Verify: manual/E2E check — change serial config, save, apply, confirm the backend route is actually invoked (network tab / test).

- [ ] **2.2 CAN termination-GPIO control is in the UI**
  Where: `frontend/pages/can.js`.
  Acceptance: when `caps.termination_gpio` is present in the manifest, a toggle for it is rendered and wired to the existing backend support.
  Verify: manual check on a platform manifest with `termination_gpio` set (e.g. arches.json) that the toggle appears and persists.

- [ ] **2.3 MQTT client-cert auth path is wired in the UI**
  Where: `frontend/pages/cloud.js`.
  Acceptance: a cert-store picker (using the existing cert store from `api/security.rs`) and an editable `client_id` field are present in the MQTT cloud settings form.
  Verify: manual check that a cert can be selected and saved as part of the cloud config, and `client_id` round-trips correctly.

- [ ] **2.4 Users → Groups endpoints are resolved**
  Where: `frontend/pages/users.js`, `crates/webconfigd/src/api/system.rs` (or wherever groups should live), `docs/api-reference.md`.
  Acceptance: either the `/api/v1/system/groups...` routes are implemented server-side and documented, or the groups UI is removed/hidden until they exist. No dead calls remain.
  Verify: hit each groups endpoint the frontend calls and confirm none 404; if implemented, confirm `docs/api-reference.md` documents them.

- [ ] **2.5 Dirty-guard covers all navigation paths**
  Where: `frontend/core/router.js`.
  Acceptance: unsaved-changes protection triggers on browser back/forward and manual hash edits, not just the programmatic `go()` path.
  Verify: manually make an unsaved change on a form page, then use browser back — confirm the guard prompts before navigating away.

- [ ] **2.6 Dead page modules removed or finished**
  Where: `frontend/pages/hardware.js`, `frontend/pages/io.js`.
  Acceptance: both are either deleted (if fully superseded by the split gpio/can/serial/leds/camera pages) or restored to the registry with a clear reason for keeping them.
  Verify: confirm `frontend/pages/registry.js` has no orphaned page files remaining in `frontend/pages/`, and the bundle size is checked against the <500 KB budget.

**Phase 2 gate:** all six boxes checked, global exit gate passing.

---

## Phase 3 — P1 Manifest/Yocto items + P2 manifest data-quality items

- [ ] **3.1 MCU companion schemas exist**
  Where: `schemas/app-mcu-stm32.schema.json` (or equivalent), referenced by `board-manifests/arches.json` (`app:mcu-stm32`).
  Acceptance: the schema file exists, validates against `docs/schema-format.md`, and the MCU settings panel actually renders from it.
  Verify: load the MCU settings page for Arches and confirm real fields render (not an empty/error panel).

- [ ] **3.2 Release artifact pipeline produces real checksums**
  Where: `meta-webconfig/recipes-core/webconfig/webconfigd.bb`.
  Acceptance: `TODO_REPLACE_WITH_RELEASED_TARBALL_SHA256` placeholders are replaced with real SHA-256 values from an actual release pipeline (even a minimal script is acceptable), and `do_fetch` succeeds.
  Verify: run `bitbake -c fetch webconfigd` (or equivalent) against a real or staged release artifact and confirm it succeeds.

- [ ] **3.3 Polkit allowlist synced per platform**
  Where: `meta-webconfig/recipes-core/webconfig/files/50-webconfigd.polkit.rules`, cross-referenced against each `board-manifests/*.json`'s `service_allowlist`.
  Acceptance: the polkit rule's allowed-unit list matches (or is generated from) each platform's real `service_allowlist`, including platform-critical units (e.g. Arches's `nvpmodel.service`, `nv_l4t_bootloader.service`, `jetson_clocks.service`, `nvfancontrol.service`, `nvargus-daemon.service`).
  Verify: diff the polkit rule's allowed list against each manifest's `service_allowlist` — no platform-critical unit missing, no manifest-absent unit present without documented justification.

- [ ] **3.4 License metadata is consistent**
  Where: `Cargo.toml`, README.md, `meta-webconfig/recipes-core/webconfig/webconfigd.bb`, a real `LICENSE` file.
  Acceptance: `Cargo.toml`'s `license` field, README.md's license statement, and every `.bb` recipe's `LICENSE`/`LIC_FILES_CHKSUM` agree, and the `LICENSE` file referenced actually exists with a matching checksum.
  Verify: `bitbake -c fetch` or a manual `md5sum` check against `LIC_FILES_CHKSUM`.

- [ ] **3.5 Board-manifest data-quality pass complete**
  Where: `board-manifests/arches.json`.
  Acceptance (verified against real hardware, not just internal consistency):
  - [ ] `thermal-zone.caps.throttle: true` has a corresponding `throttled` metric surfaced in `dashboard_tiles`.
  - [ ] CAN-termination GPIO is modeled where the hardware actually supports it (confirmed with hardware/BSP team, not assumed).
  Verify: a manifest-diff review signed off by whoever owns the Arches hardware BSP, not just an internal-consistency check.

**Phase 3 gate:** all boxes checked, global exit gate passing.

---

## Phase 4 — Test-coverage hardening (P2)

Add real HTTP-level integration tests, in this priority order. Each sub-item is done only when the test exists, passes, AND would have caught the corresponding Phase 0/1 bug if it had still been present (sanity-check by temporarily reverting the fix and confirming the new test fails).

- [ ] **4.1 Network apply/confirm/rollback** — `POST /network/apply`, `POST /network/confirm`, `POST /network/rollback`, `GET /network/scan`, `POST /network/connectivity-test` via real HTTP, with a real (not directly-planted) provisional state.
- [ ] **4.2 Services** — `start`/`stop`/`restart`/`set_enabled` via HTTP, including a forced-failure case (this must have failed before 0.2's fix and pass after).
- [ ] **4.3 Updates** — `install` (including the new multipart upload from 1.4), `ota` get/put/check, `backup` (plaintext + encrypted) get/put/check, `restore`.
- [ ] **4.4 SSE `/events` route** — asserts auth gate, vitals + journal event shapes, and cursor-based follow behavior at the HTTP layer (not just the pure parser).
- [ ] **4.5 Security** — `put` (idle timeout / plain-HTTP opt-in / WireGuard config save), `apply` (WireGuard applier), `exposure`, `trust` (must reflect 0.3's fix).
- [ ] **4.6 Operator role boundary** — at least one test exercising `Role::Operator` against `require_mutate_apps`/`require_admin`-gated routes, distinct from the existing admin-vs-viewer tests.
- [ ] **4.7 Session cookie attributes** — a test that inspects the actual `Set-Cookie` header and asserts `Secure` and `HttpOnly` are present, not just that a session ID round-trips.
- [ ] **4.8 Reboot/shutdown/root-password/last-admin-guard/group management** — HTTP-level tests, with the last-admin-delete guard specifically asserted (cannot delete the last remaining admin).
- [ ] **4.9 Monitoring, logs, io (usb/pci/i2c/spi/can/gpio/mcu/platform-panel)** — at least one HTTP-level happy-path test per route currently at zero coverage.

**Phase 4 gate:** all nine boxes checked, `cargo test --workspace --all-targets` output attached showing the new tests, global exit gate passing.

---

## Phase 5 — Polish (P2 remainder + P3)

- [ ] `std::io::Error` → `ApiError::Internal` conversions log the real underlying error server-side (`error.rs`).
- [ ] Alert engine resolves `service.<unit>` and `mcu.heartbeat` metrics, or the gap is documented in the alert-rule authoring docs so rule packs don't silently reference dead metrics.
- [ ] `read_emmc_wear` implements (or explicitly falls back with a logged reason from) the `mmc extcsd read` path for kernels without the sysfs `life_time_estimate` node.
- [ ] Board-manifest loader supports exactly one format (resolved in 0.6); dead code path removed.
- [ ] Platform-specific monitoring tiles (Arches GPU/fan/power-mode) confirmed to have a real data source, or explicitly scoped as not-yet-implemented in docs.
- [ ] `config_defaults` in `generic.yaml` documented in `docs/board-manifest.md`, or removed if not actually consumed.
- [ ] `/api/v1/io` PUT has a concurrency guard (version/ETag or equivalent) preventing silent clobbering across concurrent editors.
- [ ] Timezone list in `frontend/pages/time.js` sourced from tzdata, not a hardcoded 28-entry US-only list.
- [ ] `remote-logs.js` disable path preserves host/port/protocol instead of nulling the whole config.
- [ ] Command palette uses `modal.js`'s `openModal()` (focus trap + ARIA dialog semantics) instead of a bespoke overlay.
- [ ] `LoginThrottle` has a bounded/evicting map and, ideally, an IP-based component or hard lockout after N attempts.
- [ ] SSE/connection concurrency cap added given the `current_thread` runtime and RAM budget.
- [ ] `meta-webconfig` directory-name-vs-docs mismatch (`meta-soccentric-webconfig` in docs, `meta-webconfig` on disk) fixed everywhere.
- [ ] `webconfigd`'s `Cargo.toml` dependencies (`async-trait`, `futures-util`, `http`) moved onto the shared `workspace = true` mechanism.
- [ ] Dead code removed: `api.upload()` (frontend), `IconButton`, unused `Tabs` component, `ConfirmManager::apply_all()`.
- [ ] `Table` component has `<caption>`/`scope` attributes.
- [ ] Frontend router has a real 404 page instead of silently redirecting unknown routes to Overview.
- [ ] `appliers/wireguard.rs` validates `allowed_ips` CIDR shape and `endpoint` host:port shape before writing `wg0.conf`.
- [ ] `appliers/ap.rs` reads radio interface/hw_mode/channel from the board manifest instead of hardcoding `wlan0`/`g`/`7`.
- [ ] Schema-version migration scaffold exists in `store.rs` (even if a no-op today) ahead of the first `CONFIG_SCHEMA_VERSION` bump.
- [ ] `root_password` rejects embedded control characters/newlines before piping to `chpasswd`.
- [ ] `connectivity_test`'s DNS-resolve step description accurately reflects that it uses system resolver config, not necessarily the operator-configured DNS servers (fix the string, or fix the mechanism to actually target the configured resolver).

---

## Final sign-off

- [ ] Every box above is checked with a verify note (what was run, what the result was).
- [ ] `make build && make test && make lint` passes on a clean checkout.
- [ ] A fresh instance of this review process (re-run the same file-by-file audit that produced `MasterPlan.md`) finds no new P0/P1 items introduced by the fixes themselves.
- [ ] `MasterPlan.md`'s §6 "what's already solid" list is re-verified to still be true (regression check — fixing the gaps above should not have broken the parts that were already correct).

Only when all four final sign-off boxes are checked is this codebase ready to be called production-ready.
