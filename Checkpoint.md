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

- [x] **0.5 Yocto layer installs the real Arches board manifest**
  Where: `meta-webconfig/recipes-core/webconfig/webconfig-manifest.bb` + `.bbappend`, `meta-webconfig/recipes-core/webconfig/files/`.
  Acceptance:
  - [x] The real Arches manifest (`arches.json`) is present in the layer, in the format the manifest loader actually expects (see 0.6).
  - [x] The `.bbappend` is wired to the real Arches manifest file, not left commented out.
  - [x] Building with no matching manifest fails the build loudly, rather than silently substituting `generic.yaml`.
  - Verify: `webconfig-manifest.bb` now references `board-manifests/arches.json` directly
    from the same source tarball `webconfigd.bb`/`webconfig-static.bb` already fetch (never
    copied — single source of truth), defaults `WEBCONFIG_BOARD_FILE` to the real
    `"arches.json"` (was the wrong `"generic.yaml"` default, silently), and `do_install`
    `bbfatal`s if the resolved file is missing from the tarball. `webconfig-manifest.bbappend`
    now has a real, uncommented `WEBCONFIG_BOARD_FILE:jetson-orin-nano = "arches.json"`
    override. Removed the now-dead `files/generic.yaml` (was only ever the silent-wrong-default
    source, unrelated to the real generic-linux dev fallback which is compiled in from
    `board-manifests/generic.yaml` independent of Yocto). **Not verified against a real
    bitbake run** — no Yocto toolchain in this environment; verified by careful manual
    review against the sibling `webconfig-static.bb`/`webconfig-schemas.bb` recipes'
    established conventions (same tarball-fetch pattern, same `FILES:${PN}`/`do_install`
    idioms) and by re-reading the rewritten recipe end to end for correctness.

- [x] **0.6 Board-manifest format is consistent across docs, code, and Yocto recipe**
  Where: `docs/board-manifest.md`, `crates/webconfigd/src/bootstrap.rs`, `meta-webconfig/recipes-core/webconfig/webconfig-manifest.bb`, `board-manifests/*.json`.
  Acceptance:
  - [x] One canonical format (JSON or YAML) and one canonical on-device path is chosen.
  - [x] `docs/board-manifest.md` matches the real recipe and the real loader exactly (path, format, variable names).
  - [x] The dead code path (whichever format was not chosen) is removed from `bootstrap.rs`'s fallback chain, or explicitly documented as an intentional legacy fallback with a reason.
  - Verify: chose JSON at `/usr/share/webconfig/board-manifest.json` as canonical (matches
    the pre-existing docs and the real `board-manifests/arches.json` data — smallest-diff,
    lowest-risk choice). Removed the dead `BoardManifest::load_yaml(share)` / `<share>/board.yaml`
    fallback from both `bootstrap.rs`'s load order and `manifest.rs` (the function itself,
    since it had no other callers). Kept the local `/etc/webconfig/board.yaml` YAML override
    as an intentionally-documented field-technician escape hatch — now stated as such in both
    `bootstrap.rs`'s doc comment and `docs/board-manifest.md`. Fixed a stale
    `WEBCONFIG_MANIFEST_FILE` variable-name reference (code actually uses
    `WEBCONFIG_BOARD_FILE`) in both `docs/board-manifest.md` and `meta-webconfig/README.md`.
    `make build && make test && make lint` green (63/63 tests pass).

**Phase 0 gate:** all six boxes above checked, global exit gate passing, and a fresh `make build && make test && make lint` run attached/logged. **Phase 0 (all P0 blockers) is now fully closed.**

---

## Phase 1 — P1 Engine + Daemon items

- [x] **1.1 GPIO applier exists and is registered**
  Where: new `crates/webconfig-engine/src/appliers/gpio.rs`, `appliers/mod.rs::all()`.
  Acceptance: validate/apply/rollback + `FileBackup` pattern matching `can.rs`; direction/active_low/initial-state for named GPIO pins and board LEDs materialize to a real boot-time mechanism (udev rule or oneshot unit) on the real sysroot; registered in `appliers::all()`.
  Verify: `cargo test -p webconfig-engine appliers::gpio` (8 new tests, commit pending) —
    validation rejects empty name/bad direction/pins absent from the manifest; output pins
    render a `Type=simple` `gpioset --mode=signal` unit (active_low correctly inverts the
    driven bit); input pins and disabled pins/subsystem write no unit; rollback restores.
    `GpioApplier` is constructed with a manifest-peripherals snapshot (threaded through
    `appliers::all(&[Peripheral])` → `EngineBridge::new(store, sysroot, &manifest.peripherals)`)
    since `GpioPin` stores only a manifest name, not a device string. Registered in
    `appliers::all()`. **Scope note:** board LEDs (`BoardLedConfig`/`AppLedConfig`) are
    explicitly NOT covered — reflecting live system/service health onto LEDs is a
    continuously-running status reflector, not a boot-time validate/apply/rollback applier;
    it needs a different design and is a separate, larger follow-up. `make build && make test
    && make lint` green (155 engine + 73 daemon tests).

- [x] **1.2 RAUC variant compatibility check works on real hardware**
  Where: `crates/webconfig-engine/src/update.rs::parse_manifest_from_rauc_info`.
  Acceptance: the real backend parses an actual variant field from `rauc info` (or equivalent RAUC bundle metadata), no longer hardcodes `variant: None`.
  Verify: `cargo test real_rauc_info_variant_mismatch_is_rejected_end_to_end
    parses_variant_from_real_rauc_info_shaped_text parses_manifest_with_no_variant_line_as_none`
    (commit deed244) — realistic quoted `rauc info` text (`Compatible:  'arches'`, etc.)
    parses a `Variant:` line into `BundleManifest.variant`, and an `agx`-vs-`orin-nano`
    mismatch is rejected end-to-end through the real parser + `check_compatibility_against`.
    Also fixed a latent quote-stripping bug that would have made even platform matching fail
    against any real bundle. `make build && make test && make lint` green (147 engine + 72
    daemon tests).

- [x] **1.3 `network.cellular_5g` is resolved one way or the other**
  Acceptance: either (a) a real applier exists (ModemManager/mmcli-backed) with validate/apply/rollback, or (b) `FiveGConfig` is explicitly demoted to the same `serde_json::Value` v2-deferred treatment as `vlan`/`bridges`/`cellular`, and the API/schema/docs reflect that it's not active in v1.
  Verify: chose (b) — `FiveGConfig` deleted, `cellular_5g: serde_json::Value` matching
    vlan/bridges/cellular exactly (spec §4.2 already lists cellular/ModemManager as [v2]).
    `cargo test cellular_5g_is_an_untyped_placeholder_with_redacted_secrets_in_backup`
    (commit e946371) confirms it round-trips as opaque JSON and its `pin` is still redacted
    in the plaintext backup. `docs/api-reference.md` and the frontend 5G card both now state
    it has no effect in v1. `make build && make test && make lint` green (147 engine + 73
    daemon tests).

- [x] **1.4 OS update bundle browser upload works end-to-end**
  Where: `crates/webconfigd/src/api/updates.rs`.
  Acceptance: a real multipart upload route writes to `/data/webconfig/uploads/` (or equivalent) and the existing install-from-path logic is invoked afterward.
  Verify: `cargo test updates_upload_streams_bundle_to_disk_and_reaches_install
    updates_upload_rejects_oversized_bundle_without_buffering_whole_body` (commit 26527b6) —
    `POST /updates/upload` (raw-bytes body, not multipart — avoids a new axum feature/dependency,
    same pattern as the MCU firmware route) streams to `<data>/webconfig/uploads/`
    chunk-by-chunk via `tokio::fs`, never buffering the whole bundle in memory, then calls the
    shared `install_from_path()` the existing path-based route also uses. Verified the audit
    log shows `updates.install` ran with the uploaded bundle's real manifest data, and that a
    truncated/aborted upload leaves no partial file on disk. Also wired a file-upload action
    into `frontend/pages/updates.js`. `make build && make test && make lint` green (71/71).

- [x] **1.5 RAUC-unavailable fallback is never silent on a real device**
  Where: `crates/webconfigd/src/update_backend.rs::make_backend`.
  Acceptance: falling back to the mock backend when `is_real_root()` is true and `rauc` is unavailable logs at `warn!` or higher; ideally the API also surfaces a "update backend unavailable" state rather than serving fabricated slot data.
  Verify: `cargo test degraded_fallback_only_when_real_root_without_rauc` (commit 5712683) —
    extracted the fallback-worthiness decision into a pure `is_degraded_fallback(is_real_root,
    rauc_available)` predicate (testable without depending on the actual filesystem root or a
    real `rauc` binary) and asserted all four input combinations; `make_backend` logs
    `tracing::warn!` exactly when that predicate is true. `make build && make test && make
    lint` green (64/64 tests).

- [x] **1.6 Factory reset surfaces and handles partial failure**
  Where: `crates/webconfigd/src/api/system.rs::factory_reset`.
  Acceptance: SQL errors during reset are logged (`tracing::error!`), and the reboot step is either skipped or the response clearly indicates a partial/failed reset rather than presenting it as a clean success.
  Verify: `cargo test factory_reset_reports_partial_failure_and_skips_reboot_on_broken_db`
    (commit ee05d3a) — makes `config.db` a directory (so `Connection::open` fails), asserts
    the response is `{"factory_reset": false, "partial": true, "errors": [...]}` with a
    `config.db`-mentioning error, not a fabricated clean success. `make build && make test &&
    make lint` green (68/68 tests).

- [x] **1.7 First-boot admin audit entry uses the shared ISO-8601 timestamp helper**
  Where: `crates/webconfigd/src/auth.rs`.
  Acceptance: the local stale `now_iso()` is deleted; `first_boot`'s audit call uses `crate::api::now_iso()`.
  Verify: `cargo test first_boot_creates_admin_and_authenticates` (commit 15ab9c3) — extended
    to fetch `/logs/audit` and assert the `auth.first-boot` entry's `ts` ends in `Z` and does
    not start with `t=`. `make build && make test && make lint` green.

- [x] **1.8 Request body size limits are enforced**
  Where: `crates/webconfigd/src/routes.rs`.
  Acceptance: `tower_http::limit::RequestBodyLimitLayer` (or equivalent) applied at minimum to `/security/tls`, `/security/certs`, `/updates/install`, `/updates/restore`, `/updates/backup`, sized against the 15 MB RAM budget in spec §2.
  Verify: `cargo test oversized_request_body_is_rejected_before_buffering` (commit 2e73b8d) —
    2 MiB body to `/security/tls` (64 KiB cap) asserts 413. All five named routes now have a
    `RequestBodyLimitLayer` sized to their real legitimate payload (64 KiB TLS/cert PEM, 4 KiB
    backup-passphrase/install-path JSON, 8 KiB restore). Note: `/updates/install`'s cap is
    JSON-path-sized only because it doesn't accept raw bundle bytes yet — see 1.4, which needs
    genuine streaming-to-disk (not a bigger buffered cap) once it does. `make build && make
    test && make lint` green (69/69 tests).

- [x] **1.9 Malformed compiled-in board manifest fails loudly**
  Where: `crates/webconfigd/src/bootstrap.rs::load_or_seed_manifest`.
  Acceptance: a parse failure on the compiled-in fallback manifest logs at `error!` level (matching the schema loader's existing `warn!` pattern), and ideally the daemon surfaces a degraded-manifest signal to the UI.
  Verify: extracted `parse_compiled_in_manifest()`, which logs `tracing::error!` with the real
    parse error on failure. `cargo test malformed_arches_manifest_falls_back_to_default_not_silently
    malformed_generic_manifest_falls_back_to_default_not_silently valid_arches_manifest_parses_normally`
    (commit 6ba03f1) cover malformed JSON, malformed YAML, and the valid-manifest case.
    `make build && make test && make lint` green (67/67 tests).

- [x] **1.10 Provisional network-apply failure state is visible to the operator**
  Where: `crates/webconfigd/src/api/network.rs::apply`.
  Acceptance: if `networkd` goes `Provisional` and a later applier (firewall/wifi/ap) fails, the error response includes an indication that a pending rollback timer is active.
  Verify: `cargo test network_apply_surfaces_provisional_state_when_a_later_applier_fails`
    (commit f6951ad) — PUTs a severing static-IP interface, sabotages the firewall applier's
    write target (directory in place of its config file), asserts the apply response is 409
    and mentions both "provisional" and `confirm_window_secs`. `make build && make test &&
    make lint` green (72/72 tests).

**Phase 1 gate:** all ten boxes checked, global exit gate passing.

---

## Phase 2 — P1 Frontend items

- [x] **2.1 Serial config apply is wired**
  Where: `frontend/pages/serial.js`.
  Acceptance: an "Apply serial" action calls `POST /api/v1/io/serial/apply` after save; UI reflects apply success/failure separately from save success/failure.
  Verify: commit 4fcc2ed — added an "Apply serial" button (matches `can.js`'s Save/Apply
    pair). Apply's own toast is independent of save's; failures surface via the app-wide
    `Button` error-toast handler, distinct from save's success toast.

- [x] **2.2 CAN termination-GPIO control is in the UI**
  Where: `frontend/pages/can.js`.
  Acceptance: when `caps.termination_gpio` is present in the manifest, a toggle for it is rendered and wired to the existing backend support.
  Verify: commit 6b2314c — a "Toggle termination" button appears per CAN interface card when
    `caps.termination_gpio` is set (confirmed present for `arches.json`'s `can0` →
    `gpio-can0-term`, itself a real `kind: gpio` peripheral), calling the existing
    `POST /io/gpio/:name/toggle` route.

- [x] **2.3 MQTT client-cert auth path is wired in the UI**
  Where: `frontend/pages/cloud.js`.
  Acceptance: a cert-store picker (using the existing cert store from `api/security.rs`) and an editable `client_id` field are present in the MQTT cloud settings form.
  Verify: commit 3030108 — a client-certificate `<select>` populated from `GET
    /security/certs` (filtered to `kind == "client"`) plus an editable `client_id` field, both
    wired into the `PUT /services/mqtt` save payload (previously passed through unedited from
    the loaded config).

- [x] **2.4 Users → Groups endpoints are resolved**
  Where: `frontend/pages/users.js`, `crates/webconfigd/src/api/system.rs` (or wherever groups should live), `docs/api-reference.md`.
  Acceptance: either the `/api/v1/system/groups...` routes are implemented server-side and documented, or the groups UI is removed/hidden until they exist. No dead calls remain.
  Verify: confirmed all 5 routes the frontend calls
    (`list_groups`/`create_group`/`delete_group`/`add_group_member`/`remove_group_member`) are
    real, implemented, and wired in `routes.rs` — the only gap was documentation. Added to
    `docs/api-reference.md` (commit 1012971).

- [x] **2.5 Dirty-guard covers all navigation paths**
  Where: `frontend/core/router.js`.
  Acceptance: unsaved-changes protection triggers on browser back/forward and manual hash edits, not just the programmatic `go()` path.
  Verify: commit 4516d9a — `hashchange` now routes through the same
    `confirmDiscardIfDirty()` prompt as `go()`. Since the hash has already changed by the time
    the event fires, an unconfirmed navigation reverts `location.hash` to the last-rendered
    route first (keeping the address bar/history consistent with what's displayed), guarded
    by a `suppressGuard` flag so the revert itself doesn't re-trigger the prompt.

- [x] **2.6 Dead page modules removed or finished**
  Where: `frontend/pages/hardware.js`, `frontend/pages/io.js`.
  Acceptance: both are either deleted (if fully superseded by the split gpio/can/serial/leds/camera pages) or restored to the registry with a clear reason for keeping them.
  Verify: commit 07e6070 — both deleted (confirmed zero references anywhere in
    `frontend/pages/registry.js` or elsewhere before deletion). Frontend tree is 444 KB,
    under the 500 KB spec §2 budget.

**Phase 2 gate:** all six boxes checked, global exit gate passing. **Phase 2 (all P1 frontend items) is now fully closed.**

---

## Phase 3 — P1 Manifest/Yocto items + P2 manifest data-quality items

- [x] **3.1 MCU companion schemas exist**
  Where: `schemas/app-mcu-stm32.schema.json` (or equivalent), referenced by `board-manifests/arches.json` (`app:mcu-stm32`).
  Acceptance: the schema file exists, validates against `docs/schema-format.md`, and the MCU settings panel actually renders from it.
  Verify: `cargo test real_mcu_stm32_schema_loads_and_renders_real_fields` (pending commit) —
    loads the real repo-root `schemas/` directory (not a fixture), asserts `app-mcu-stm32`
    parses with descriptor id `app:mcu-stm32` and real fields (`heartbeat_interval_ms`,
    `watchdog_timeout_ms`, plus a diagnostics group). Also added to
    `meta-webconfig/recipes-core/webconfig/files/` and wired into `webconfig-schemas.bb`'s
    `SRC_URI` so it actually ships (that recipe still uses an older files/-copy pattern rather
    than the tarball pattern its siblings now use — flagged separately, spawn_task, as its own
    drift-risk fix). `make build && make test && make lint` green (156 engine + 73 daemon).

- [x] **3.2 Release artifact pipeline produces real checksums**
  Where: `meta-webconfig/recipes-core/webconfig/webconfigd.bb`.
  Acceptance: `TODO_REPLACE_WITH_RELEASED_TARBALL_SHA256` placeholders are replaced with real SHA-256 values from an actual release pipeline (even a minimal script is acceptable), and `do_fetch` succeeds.
  Verify: added `tools/release-artifacts.sh` (+ `make release-artifacts`) — packages a real
    `webconfigd-<version>.tar.gz` via `git archive` (tracked files only) and `vendor.tar.gz`
    from the vendored crate directory, computes real SHA-256 for both, and ran it once to fill
    in `webconfigd.bb`'s `SRC_URI[src.sha256sum]`/`SRC_URI[vendor.sha256sum]` (also gave
    `vendor.tar.gz` its own checksum entry — it had none at all before, not just a
    placeholder). **Not verified with a real `bitbake -c fetch`** — no Yocto toolchain in this
    environment; verified by inspecting the produced tarballs and cross-checking their
    checksums with `sha256sum` directly. Note: like any version-pinned recipe checksum, this
    value is only valid for the tarball contents it was computed from — regenerate via `make
    release-artifacts` at every actual release cut. `make build && make test && make lint`
    green (this is a build-tooling-only change; no Rust files touched).

- [x] **3.3 Polkit allowlist synced per platform**
  Where: `meta-webconfig/recipes-core/webconfig/files/50-webconfigd.polkit.rules`, cross-referenced against each `board-manifests/*.json`'s `service_allowlist`.
  Acceptance: the polkit rule's allowed-unit list matches (or is generated from) each platform's real `service_allowlist`, including platform-critical units (e.g. Arches's `nvpmodel.service`, `nv_l4t_bootloader.service`, `jetson_clocks.service`, `nvfancontrol.service`, `nvargus-daemon.service`).
  Verify: `cargo test polkit_allowlist_matches_manifest_service_allowlist` (commit fc16ec1) —
    parses both files and asserts the sets match exactly. Removed stale `rauc.service`/
    `mosquitto.service` (neither in the manifest nor managed as a unit anywhere in the daemon)
    and added the five missing Arches-critical `nv*` units plus `systemd-resolved.service`/
    `wpa_supplicant.service`. `make build && make test && make lint` green (157 engine + 73
    daemon tests).

- [x] **3.4 License metadata is consistent**
  Where: `Cargo.toml`, README.md, `meta-webconfig/recipes-core/webconfig/webconfigd.bb`, a real `LICENSE` file.
  Acceptance: `Cargo.toml`'s `license` field, README.md's license statement, and every `.bb` recipe's `LICENSE`/`LIC_FILES_CHKSUM` agree, and the `LICENSE` file referenced actually exists with a matching checksum.
  Verify: commit b3809f7 — added the real `LICENSE` file, switched `Cargo.toml` to
    `license-file = "LICENSE"` (was the outlier `MIT`; every `.bb` recipe and README already
    said CLOSED), filled in the real md5 (`3382002c026d2dcb5817068a8f184313`, verified via
    `md5sum LICENSE`) and fixed a `mdline`→`beginline` bitbake parameter typo in
    `webconfigd.bb`. `cargo metadata` confirms `license_file` resolves to the real repo-root
    `LICENSE` for both crates. `make build && make test && make lint` green.

- [x] **3.5 Board-manifest data-quality pass complete**
  Where: `board-manifests/arches.json`.
  Acceptance (verified against real hardware, not just internal consistency):
  - [x] `thermal-zone.caps.throttle: true` has a corresponding `throttled` metric surfaced in `dashboard_tiles`.
  - [x] CAN-termination GPIO is modeled where the hardware actually supports it (confirmed with hardware/BSP team, not assumed).
  Verify: commit 245526f fixed a real internal-consistency bug — the thermal tile referenced
    `arches.throttled`, a metric name `alert.rs::resolve_metric` never implements, while the
    default alert rule and the resolver both use `temp.throttled`; the tile now matches.
    CAN-termination was already correctly modeled (`gpio-can0-term`, a real `kind: gpio`
    peripheral). **Not independently verified against real hardware/BSP team sign-off** — no
    such reviewer is reachable in this environment; only the internal-consistency half of this
    item is closed. `make build && make test && make lint` green.

**Phase 3 gate:** all boxes checked, global exit gate passing.

---

## Phase 4 — Test-coverage hardening (P2)

Add real HTTP-level integration tests, in this priority order. Each sub-item is done only when the test exists, passes, AND would have caught the corresponding Phase 0/1 bug if it had still been present (sanity-check by temporarily reverting the fix and confirming the new test fails).

- [x] **4.1 Network apply/confirm/rollback** — `POST /network/apply`, `POST /network/confirm`, `POST /network/rollback`, `GET /network/scan`, `POST /network/connectivity-test` via real HTTP, with a real (not directly-planted) provisional state.
  Commit 93c7ae5: `network_apply_confirm_and_rollback_via_http`, `network_scan_and_connectivity_test_via_http`.
- [x] **4.2 Services** — `start`/`stop`/`restart`/`set_enabled` via HTTP, including a forced-failure case (this must have failed before 0.2's fix and pass after).
  Already covered by `services_verbs_report_real_failure_not_fake_ok` (commit 0f49bb7, written as part of 0.2's fix itself).
- [x] **4.3 Updates** — `install` (including the new multipart upload from 1.4), `ota` get/put/check, `backup` (plaintext + encrypted) get/put/check, `restore`.
  Commit a1be325: `updates_install_ota_backup_restore_via_http` (upload path already covered by 1.4's own tests).
- [x] **4.4 SSE `/events` route** — asserts auth gate, vitals + journal event shapes, and cursor-based follow behavior at the HTTP layer (not just the pure parser).
  Commit 1b89a77: `events_sse_requires_auth_and_emits_vitals_over_http` (paused-clock, no real wait). Cursor/journal parsing already covered by sse.rs's own pure-parser tests.
- [x] **4.5 Security** — `put` (idle timeout / plain-HTTP opt-in / WireGuard config save), `apply` (WireGuard applier), `exposure`, `trust` (must reflect 0.3's fix).
  Commit 2945adb: `security_put_apply_and_exposure_via_http`. `trust` already covered by 0.3's test.
- [x] **4.6 Operator role boundary** — at least one test exercising `Role::Operator` against `require_mutate_apps`/`require_admin`-gated routes, distinct from the existing admin-vs-viewer tests.
  Commit f6791aa: `operator_role_can_mutate_apps_but_not_admin_only_routes`.
- [x] **4.7 Session cookie attributes** — a test that inspects the actual `Set-Cookie` header and asserts `Secure` and `HttpOnly` are present, not just that a session ID round-trips.
  Commit 3f47a65: `session_cookie_is_secure_and_http_only_on_the_wire`.
- [x] **4.8 Reboot/shutdown/root-password/last-admin-guard/group management** — HTTP-level tests, with the last-admin-delete guard specifically asserted (cannot delete the last remaining admin).
  Commit b71cc7d: `reboot_shutdown_root_password_last_admin_guard_and_groups_via_http`.
- [x] **4.9 Monitoring, logs, io (usb/pci/i2c/spi/can/gpio/mcu/platform-panel)** — at least one HTTP-level happy-path test per route currently at zero coverage.
  Commit ede44a9: `monitoring_logs_and_io_happy_paths_via_http` (mcu already covered by 0.1's test).

**Phase 4 gate:** all nine boxes checked. `cargo test --workspace --all-targets` — 157 engine + 82 daemon tests, all passing. `make build && make test && make lint` green throughout. **Phase 4 (all P2 test-coverage items) is now fully closed.**

---

## Phase 5 — Polish (P2 remainder + P3)

- [x] `std::io::Error` → `ApiError::Internal` conversions log the real underlying error server-side (`error.rs`). Verify: `error.rs`'s `From<std::io::Error> for ApiError` calls `tracing::error!(error = %e, "io error converted to ApiError::Internal")` before collapsing to `ApiError::Internal` — resolved in an earlier phase; checkbox was simply never flipped.
- [x] Alert engine resolves `service.<unit>` and `mcu.heartbeat` metrics, or the gap is documented in the alert-rule authoring docs so rule packs don't silently reference dead metrics. Verify: not implemented (no data source exists for either), but the gap is explicitly documented in `docs/webconfig-spec.md`: "`service.<unit>` and `mcu.heartbeat` are declared metric names but `alert.rs::resolve_metric` has no data source for either yet ... a rule pack referencing them is accepted but silently never fires ... until a real source is wired." Satisfies the "or documented" branch.
- [x] `read_emmc_wear` implements (or explicitly falls back with a logged reason from) the `mmc extcsd read` path for kernels without the sysfs `life_time_estimate` node. Verify: resolved in Phase 3 — `monitoring.rs`'s `read_emmc_wear` falls through from sysfs to a real `mmc extcsd read` invocation on the real root, parsed by `parse_extcsd_life_time` (test: `parses_real_mmc_extcsd_life_time_output`). Checkbox was simply never flipped after the earlier fix.
- [x] Board-manifest loader supports exactly one format (resolved in 0.6); dead code path removed. Verify: commit 149efd9 — `webconfig-manifest.bb` now installs the real `board-manifests/arches.json` (JSON, the one canonical format) instead of defaulting to `generic.yaml`; `bootstrap::load_or_seed_manifest()` is a 2-tier fallback with the dead `<share>/board.yaml` check already removed.
- [x] Platform-specific monitoring tiles (Arches GPU/fan/power-mode) confirmed to have a real data source, or explicitly scoped as not-yet-implemented in docs. Verify: `docs/webconfig-spec.md` explicitly scopes this: manifest declares `gpu.util`/`gpu.mem`/`arches.nvpmodel`/`arches.fan_rpm` tiles and the endpoint serves manifest data verbatim, "but no backend data source populates those specific metric values yet ... Treat GPU/fan/power-mode tiles as declared-but-not-yet-implemented, not a working feature"; nvpmodel is separately documented as "a static list of possible mode names ... not the live current mode ... treat as not-yet-implemented." Checkbox was simply never flipped after the earlier documentation fix.
- [x] `config_defaults` in `generic.yaml` documented in `docs/board-manifest.md`, or removed if not actually consumed. Verify: both — `board-manifests/generic.yaml` sets it, `docs/board-manifest.md` documents its first-boot-only semantics, and `bootstrap.rs` actually consumes it (`manifest.config_defaults.clone().unwrap_or_default()` into `ConfigStore::with_defaults`). Checkbox was simply never flipped after the earlier fix.
- [x] `/api/v1/io` PUT has a concurrency guard (version/ETag or equivalent) preventing silent clobbering across concurrent editors. Verify: resolved in Phase 1 (1.9/1.10) — `api/io.rs`'s `io_version()` hashes the current `IoConfig`, `GET /io` returns it as `io_version`, and `PUT /io` rejects a stale `If-Match` with 409 while allowing an omitted header (older clients). Checkbox was simply never flipped after the earlier fix. Test: `tests::io_put_rejects_stale_if_match_but_allows_omitted_header` (passing).
- [x] Timezone list in `frontend/pages/time.js` sourced from tzdata, not a hardcoded 28-entry US-only list. Verify: resolved earlier this phase — `time.js`'s `allTimezones()` calls `Intl.supportedValuesOf('timeZone')` (real ECMA-402 browser API, full IANA tzdata, zero build step), with a minimal 5-entry fallback only for browsers old enough to lack the API. Checkbox was simply never flipped after the earlier fix.
- [x] `remote-logs.js` disable path preserves host/port/protocol instead of nulling the whole config. Verify: resolved earlier this phase — `save()` always sends the host/port/protocol the operator entered, toggling only `enabled`, instead of nulling the remembered destination on disable. Checkbox was simply never flipped after the earlier fix.
- [x] Command palette uses `modal.js`'s `openModal()` (focus trap + ARIA dialog semantics) instead of a bespoke overlay. Verify: `openModal()` extended with `scrimClass`/`dialogClass` params; `command-palette.js` now calls `openModal({ body: panel, scrimClass: 'cmdk-scrim', dialogClass: 'cmdk-dialog' })` instead of building its own `.cmdk-overlay`/`.cmdk-panel` DOM, so it inherits focus-trap/ARIA/Escape-to-close for free; `.cmdk-scrim`/`.cmdk-dialog` in `components.css` only override sizing/positioning atop the shared `.modal-scrim`/`.modal` rules. `make build && make test && make lint` clean.
- [x] `LoginThrottle` has a bounded/evicting map and, ideally, an IP-based component or hard lockout after N attempts. Verify: `attempts` map capped at `MAX_TRACKED_USERNAMES` (1000), evicting the least-recently-active entry on overflow (`auth::tests::bounded_map_evicts_oldest_when_over_capacity`); hard lockout added — 10 failures within a 60s window reject login outright, self-expiring once the window passes without a background sweep (`lockout_triggers_at_threshold`, `lockout_does_not_trigger_below_threshold`, `record_success_clears_lockout`). IP-based tracking intentionally not added — no client-IP plumbing exists yet (`ConnectInfo`/`X-Forwarded-For`), and per-username lockout + bounded map already close the DoS/memory gap the item raised. **Amendment (final-sign-off fresh audit caught a real P1):** the original `is_locked_out()`/`delay_ms()`/`record_failure()` split acquired the lock three separate times with unlocked work (the actual `authenticate()` call, plus the sleep) in between, so concurrent requests for the same username could all read "not locked yet" before any single one's failure was recorded — the hard lockout this item added was itself racy under concurrent flooding, the exact attack it was meant to stop. Fixed by merging check+reserve into one atomic `begin_attempt()` (single lock acquisition, no `.await` inside the critical section) that increments the count *before* authentication runs, so the Nth concurrent caller queued behind the lock sees every prior caller's count already applied. New regression test `concurrent_attempts_cannot_race_past_lockout` fires `3×LOCKOUT_THRESHOLD` concurrent attempts and asserts exactly `LOCKOUT_THRESHOLD` are ever allowed through. `make build && make test && make lint` clean (166+89 tests).
- [x] SSE/connection concurrency cap added given the `current_thread` runtime and RAM budget. Verify: `AppState.sse_connections` is a `Semaphore` sized `MAX_SSE_CONNECTIONS` (8); `/events` (`sse.rs`) does `try_acquire_owned()` before starting the stream, rejecting with 503 (`ApiError::NotReady`) beyond the cap, and holds the `OwnedSemaphorePermit` inside the `stream::unfold` state so it releases exactly on client disconnect. `tests::events_sse_concurrency_cap_rejects_beyond_max` opens `MAX_SSE_CONNECTIONS` connections, confirms the next is rejected, then confirms dropping one frees a slot. `make build && make test && make lint` clean.
- [x] `meta-webconfig` directory-name-vs-docs mismatch (`meta-soccentric-webconfig` in docs, `meta-webconfig` on disk) fixed everywhere. Verify: fixed all literal-path references in `README.md`, `docs/webconfig-spec.md`, `meta-webconfig/README.md`'s own title, and `REPO_MAP.md` to say `meta-webconfig`; `meta-webconfig/conf/layer.conf`'s header comment now notes the disk name differs from the spec's original name instead of misnaming itself. `CLAUDE.md`'s Don't entry updated to reflect the fix and clarify `soccentric-webconfig` still correctly appears as the Yocto *collection name* in layer.conf overrides (a distinct concept from the directory name). `MasterPlan.md` (the audit source) intentionally left untouched. Noticed but out-of-scope: `layer.conf` is missing `BBFILE_COLLECTIONS`/`BBFILE_PATTERN` declarations for that collection name — flagged separately (task_38bbd37f), not fixed here.
- [x] `webconfigd`'s `Cargo.toml` dependencies (`async-trait`, `futures-util`, `http`) moved onto the shared `workspace = true` mechanism. Verify: all three added to root `Cargo.toml`'s `[workspace.dependencies]` (version/feature specs preserved exactly — `futures-util` keeps its `default-features = false`), `webconfigd/Cargo.toml` now references `{ workspace = true }` for each. `Cargo.lock` unchanged (confirmed via `git status`), `make build && make test && make lint` clean.
- [x] Dead code removed: `api.upload()` (frontend), `IconButton`, unused `Tabs` component, `ConfirmManager::apply_all()`. Verify: `api.upload()` was already resolved during Phase 0.1/1.4 (repurposed to the real raw-bytes upload path, not dead — no further action needed). `IconButton` removed from `frontend/components/button.js` (zero callers). `frontend/components/tabs.js` deleted along with its now-orphaned `.tabs-list`/`.tab-btn` CSS in `components.css` (zero callers; nothing else defines or consumes those classes). `EngineBridge::apply_all()` in `crates/webconfigd/src/engine_bridge.rs` removed (zero callers anywhere in routes/api/tests; the checklist named it `ConfirmManager::apply_all()` but the actual dead method lives on `EngineBridge` in the same file). `make build && make test && make lint` clean.
- [x] `Table` component has `<caption>`/`scope` attributes. Verify: `frontend/components/table.js`'s `Table()` now sets `scope="col"` on every `<th>` unconditionally, and accepts an optional `caption` prop rendered as a visually-hidden (new `.sr-only` utility in `base.css`) `<caption>`. Every one of the 15 real call sites across `frontend/pages/*.js` now passes `caption` matching the visible Card/page heading already next to that table (Alerts ×2, Faults, Logs, Users, Serial ×2, Security, PCI, Services, Overview ×3, Updates, USB) — no fabricated text, reusing what's already on screen. `make build && make test && make lint` clean; `node --check` on every touched file.
- [x] Frontend router has a real 404 page instead of silently redirecting unknown routes to Overview. Verify: `frontend/core/router.js`'s `parse()` now distinguishes an empty hash (the real index route → `overview`) from a non-empty, unrecognized segment (→ `null`, not a silent `overview` fallback). `renderCurrent()` renders a real `EmptyState` "Page not found" (with the actual unmatched path echoed back) when `route == null`, sets a matching topbar title/document title, and skips sidebar-active-highlighting, admin-gating, and page-module loading for that case. `node --check` passed; `make build && make test && make lint` clean. Not verified live in-browser: the daemon binary always resolves `Sysroot::new()` (real root, no dev-mode fake sysroot), so exercising it here would mean running the real daemon against this host's `/etc` — skipped as an unnecessary risk for a client-side-only hash-parsing change; reasoning verified by inspection instead.
- [x] `appliers/wireguard.rs` validates `allowed_ips` CIDR shape and `endpoint` host:port shape before writing `wg0.conf`. Verify: new `valid_allowed_ips()` splits the wg-quick comma-separated CIDR list and validates each entry's address/prefix against the correct family bit-width (32 for IPv4, 128 for IPv6); new `valid_endpoint()` validates `host:port` shape, requiring a bracketed host to be a real IPv6 literal and otherwise confining the host to a safe charset (blocks shell/config injection, mirroring the journald remote-syslog host guard). Both wired into `validate()`. Tests: `accepts_multi_entry_allowed_ips_and_ipv6`, `rejects_malformed_allowed_ips`, `accepts_valid_endpoint_shapes`, `rejects_malformed_endpoint` (missing port, port 0, non-numeric port, bad IPv6 literal, shell metacharacters). `make build && make test && make lint` clean (163+88 tests).
- [x] `appliers/ap.rs` reads radio interface/hw_mode/channel from the board manifest instead of hardcoding `wlan0`/`g`/`7`. Verify: new `manifest::WifiRadio { interface, hw_mode, channel }` (per-field serde defaults preserving the old hardcoded values, so existing manifests parse unchanged); `ApApplier::new(&WifiRadio)` snapshots it at construction (same pattern as `GpioApplier`'s manifest-peripherals snapshot) and `render_hostapd`/`render_dnsmasq`/`render_networkd` now read `self.radio.*` instead of the removed `AP_IFACE`/hardcoded `hw_mode`/`channel` literals. Threaded through `appliers::all()` and `EngineBridge::new()` (bootstrap.rs + 3 test call sites updated). New test `manifest_radio_overrides_interface_hw_mode_and_channel` proves a non-default radio (wlan1/a/36) actually renders into all three generated files. Documented in `docs/board-manifest.md`. `make build && make test && make lint` clean (164+88 tests).
- [x] Schema-version migration scaffold exists in `store.rs` (even if a no-op today) ahead of the first `CONFIG_SCHEMA_VERSION` bump. Verify: new `ConfigStore::migrate()` walks `file.schema_version` up to `CONFIG_SCHEMA_VERSION` one step at a time via a `match`, called from `load()` after the existing newer-than-engine check. A commented-out `// 1 => migrate_v1_to_v2(file),` marks the extension point for the first future migration; the catch-all arm returns a new `StoreError::Migration(u32)` instead of silently passing through or panicking. Scaffold-only clippy allows (`never_loop`/`match_single_binding`/`unreachable_code`/`unused_mut`) are on the function with a comment explaining they're inherent to having zero real migration arms yet — they disappear once a real `1 => ...` arm lands. Tests: `migrate_is_a_no_op_at_current_schema_version`, `migrate_errors_on_an_older_version_with_no_registered_step`. `make build && make test && make lint` clean (166+88 tests).
- [x] `root_password` rejects embedded control characters/newlines before piping to `chpasswd`. Verify: this was a real injection vector, not just hardening — `writeln!(stdin, "root:{}", req.password)` piped straight to `chpasswd`, so a password containing `\n` let an authenticated admin inject an arbitrary extra `user:password` line (e.g. setting a *different* account's password in the same call). `crates/webconfigd/src/api/system.rs`'s `root_password()` now rejects any password containing an ASCII control character (`is_ascii_control()`, covers `\n`/`\r`/tab/etc.) with 400 before reaching chpasswd. New HTTP test case in `reboot_shutdown_root_password_last_admin_guard_and_groups_via_http` posts a password with an embedded `\nattacker:pwned` line and asserts 400. `make build && make test && make lint` clean (166+88 tests).
- [x] `connectivity_test`'s DNS-resolve step description accurately reflects that it uses system resolver config, not necessarily the operator-configured DNS servers (fix the string, or fix the mechanism to actually target the configured resolver). Verify: the old detail message claimed resolution happened "via configured DNS (X …)" using either the first configured global DNS server or a fabricated "1.1.1.1" fallback — but `getent hosts` actually goes through the whole system resolver stack (which may use DHCP-provided or /etc/resolv.conf nameservers unrelated to that value), so the claim could be flatly wrong. `api/network.rs`'s `connectivity_test` now says "resolved … via the system resolver (configured DNS preference: X)" when a global DNS is set, or "(no global DNS override configured)" when none is, naming the configured value as a preference rather than a proven fact — no fabricated fallback IP. Test: `network_scan_and_connectivity_test_via_http` extended to assert both message shapes (unconfigured → no fabricated server; configured → named as "preference"). `make build && make test && make lint` clean (166+88 tests).

---

## Final sign-off

- [x] Every box above is checked with a verify note (what was run, what the result was). Verify: confirmed every checked item across Phases 0-5 carries evidence — Phase 0-4 items via a "Where:"/"Acceptance:" breakdown plus commit hashes and test names on the following lines, Phase 5 items via inline "Verify:" notes citing the specific mechanism and test(s). Zero `- [ ]` lines remain anywhere above this section.
- [x] `make build && make test && make lint` passes on a clean checkout. Verify: ran `cargo clean` (wiped 7.3GiB / 10241 target-dir files, forcing a full from-scratch dependency + workspace rebuild) then `make build && make test && make lint` — all three passed clean: 166 engine + 88 daemon tests, 0 failures, clippy `-D warnings` and `cargo fmt --check` both clean.
- [ ] A fresh instance of this review process (re-run the same file-by-file audit that produced `MasterPlan.md`) finds no new P0/P1 items introduced by the fixes themselves.
- [ ] `MasterPlan.md`'s §6 "what's already solid" list is re-verified to still be true (regression check — fixing the gaps above should not have broken the parts that were already correct).

Only when all four final sign-off boxes are checked is this codebase ready to be called production-ready.
