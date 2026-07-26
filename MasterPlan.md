# WebConfig Master Plan — Path to Production Readiness

**Repo:** Arches-Configuration (SoCcentric WebConfig — Rust daemon + engine + vanilla-JS frontend, shared across Arches, Acadia, Zion, Pinnacle, Joshua, Sequoia)
**Prepared:** 2026-07-25
**Method:** full file-by-file review of `crates/webconfig-engine/`, `crates/webconfigd/`, `frontend/`, `board-manifests/`, `schemas/`, `docs/`, `meta-webconfig/`, and the Makefile/build tooling, checked line-by-line against `docs/webconfig-spec.md`, `docs/api-reference.md`, `docs/board-manifest.md`, `docs/schema-format.md`, and every explicit rule in `CLAUDE.md`.
**Verdict up front:** the Rust core (`webconfig-engine`, most of `webconfigd`) is well-engineered and mostly production-grade. The gaps that make this **not production-ready today** cluster in four places: (1) a handful of handlers that report success without doing the real work (MCU firmware flash, service start/stop/restart, TLS trust status), (2) a secrets-redaction hole that can leak app-level secrets in backups/diagnostics, (3) the Yocto BSP layer cannot currently select a real per-platform board manifest — every image build gets the generic fallback, which defeats the entire "one codebase, six platforms" premise at the point it matters most, and (4) large swaths of the HTTP API (network apply/confirm/rollback, SSE, updates, security, io) have zero test coverage, so regressions in the areas above would ship silently.

This document is a plan, not a patch — no code was changed. It is organized so engineering can turn it directly into a backlog: each item has a severity, the exact file/line evidence, and a description of "done."

---

## 1. Scope and how to read this plan

- **P0 (Blocker):** must be fixed before any production/field deployment. Includes anything that lies about success, leaks a secret, or silently defeats a security control.
- **P1 (High):** must be fixed before general availability; either a real functional gap in a headline feature or a substantial trust/safety issue.
- **P2 (Medium):** should be fixed before GA but is not immediately dangerous; test-coverage gaps, silent-degradation-with-no-signal issues, doc/code drift.
- **P3 (Low):** cleanup, hardening, or nice-to-have; safe to schedule after GA.

Each item lists **Where** (file:line), **What's wrong**, and **Done means**.

---

## 2. P0 — Blockers

### 2.1 MCU firmware "flash" does not flash anything and does not verify signatures
- **Where:** `crates/webconfigd/src/api/io.rs:632-647` (handler `mcu_firmware`); frontend `frontend/pages/mcu.js:24-47`.
- **What's wrong:** The request body is `{version, signed}` — no firmware bytes, no file reference. The server never cryptographically verifies `signed`; it takes the client's boolean at face value, performs no flash operation anywhere in the codebase, and returns `"flashed": true`-shaped success. The frontend has no `<input type=file>` and never calls the (dead) `api.upload()` helper. This is the one MCU-facing counterpart to the OS-update path (`update_backend.rs`), which *does* do real signature verification — the asymmetry means an operator believes MCU firmware has been validated and flashed when nothing happened.
- **Done means:** a real multipart upload path that lands bytes on disk, a signature-verification step using the same trust chain as `update_backend.rs::verify_bundle`, and an actual flash mechanism (or, if out of scope for v1, the route and UI must clearly state "not implemented" rather than reporting success — do not ship a convincing no-op).

### 2.2 Service start/stop/restart/enable report success even when the operation failed
- **Where:** `crates/webconfigd/src/api/services.rs:40-56, 58-74, 76-92, 100-123`.
- **What's wrong:** Each handler calls `app.systemd.<verb>(&unit).await` and discards the `Result` via `.ok()`/`.map_err(...).ok()`, always returning `{"ok": true}` and writing an audit entry that records the *attempt*, not the *outcome*. A restart that fails (bad unit name, permission denied, unit in a broken state) is indistinguishable from one that succeeded, both to the operator and in the audit trail.
- **Done means:** propagate the systemd error to `ApiError` (500 or a specific error code), and audit the real outcome.

### 2.3 Security "trust" status is permanently hardcoded to unknown
- **Where:** `crates/webconfigd/src/api/security.rs:206-220` (`GET /security/trust`).
- **What's wrong:** `update_signing_key_id`, `verity_status`, `encryption_status` are hardcoded null/"unknown" regardless of platform; only `secure_boot`/`tpm_present` are wired to real manifest data. Spec §4.8 explicitly requires these be read from a BSP platform-status file — no such reader exists anywhere in the codebase, and there's no TODO marking this as incomplete, so it reads as finished when it isn't.
- **Done means:** either implement the platform-status-file reader (needs a defined file format from the BSP team) or clearly surface "not available on this platform" in both API response and UI rather than a bare "unknown" that looks like a real read.

### 2.4 App-level `secret`-typed fields are never redacted in backups/diagnostics
- **Where:** `crates/webconfigd/src/api/updates.rs:464-492` (`redact_secrets`).
- **What's wrong:** `redact_secrets` is a hand-maintained allowlist covering only top-level `Config` struct fields. Any schema-declared `secret`-type field inside `cfg.app_settings[id]` (the spec's own first-class secret field type, `docs/schema-format.md` / spec §3) is **never redacted**. `GET /updates/backup` (plaintext path) and `GET /updates/diagnostics` can therefore leak app-level plaintext secrets.
- **Done means:** redaction must walk `app_settings` using each app's loaded schema and blank any field typed `secret`, not just a hardcoded top-level list. Add a regression test that adds a `secret`-typed app schema field and asserts it never appears in backup/diagnostics output.

### 2.5 Yocto layer cannot select a real per-platform board manifest
- **Where:** `meta-webconfig/recipes-core/webconfig/webconfig-manifest.bb` + `.bbappend`; `meta-webconfig/recipes-core/webconfig/files/` (only contains `generic.yaml`).
- **What's wrong:** the recipe/bbappend expects per-machine `.yaml` files (`arches.yaml`, `acadia.yaml`, etc.) that were never created — the real per-platform data only exists as the six `.json` files in `board-manifests/` at the repo root, in the wrong format and the wrong location for the Yocto layer to consume. Every machine built from this layer today silently gets the generic fallback manifest, defeating the "one codebase, six platforms, manifest-only difference" architecture at exactly the point it needs to work.
- **Done means:** convert (or directly ship) the six real `board-manifests/*.json` files into the layer in the format `bootstrap.rs`/the manifest loader actually expects (see 2.6), wire each `.bbappend` machine override to its real platform file, and add a build-time check that fails the build if a target machine has no matching manifest rather than silently falling back.

### 2.6 Board-manifest format is inconsistent between docs, code, and Yocto recipe (JSON vs YAML)
- **Where:** `docs/board-manifest.md:3-4,74-76` (documents `/usr/share/webconfig/board-manifest.json`); `crates/webconfigd/src/bootstrap.rs:71-99` (loads local YAML → share YAML → share JSON → compiled-in fallback, in that order); `meta-webconfig/recipes-core/webconfig/webconfig-manifest.bb` (installs `board.yaml` from `generic.yaml`); `board-manifests/*.json` (the six real per-platform files are JSON, not YAML).
- **What's wrong:** three different sources of truth disagree on the on-disk format and path. Anyone building against the documented path/format will fail against the real recipe. Combined with 2.5, this is why no real platform manifest can currently reach a running device through the Yocto path.
- **Done means:** pick one canonical format + path, update the doc, the recipe, and (if needed) `bootstrap.rs`'s fallback order to match, and delete the now-dead code path for the format not chosen.

---

## 3. P1 — High priority

### Engine (`webconfig-engine`)
1. **No GPIO applier exists.** `store.rs:916-927` (`GpioPin`) and `store.rs:851-873` (`BoardLedConfig`/`AppLedConfig`) are stored and API-exposed via `PUT /io` / `POST /io/gpio/:name/toggle`, but nothing materializes GPIO direction/active_low/initial-state to the system at boot — unlike CAN/serial, which generate real bring-up units. Contradicts CLAUDE.md's explicit "register every system applier" rule. **Done means:** an `appliers/gpio.rs` following the same validate/apply/rollback + `FileBackup` pattern as `can.rs`, registered in `appliers::all()`.
2. **RAUC variant compatibility check is dead on real hardware.** `update.rs:303-323` (`parse_manifest_from_rauc_info`) hardcodes `variant: None` — it never parses a `Variant:` line from `rauc info` output, so the fully-implemented, fully-tested variant-mismatch guard (an Arches `agx` bundle must not flash onto an Arches `orin-nano`) can never fire outside the mock backend. **Done means:** parse the real variant field (from RAUC bundle metadata) and add a test against real `rauc info`-shaped text, not just the mock.
3. **`network.cellular_5g` has no applier.** `store.rs:459-477` defines a typed `FiveGConfig`, distinct from the spec's explicitly-deferred `serde_json::Value` v2 placeholder slots (vlan/bridges/cellular) — meaning it reads as intended for v1 but has no ModemManager/mmcli integration anywhere. **Done means:** either demote it to the same v2 `Value`-placeholder treatment, or implement a real applier.

### Daemon (`webconfigd`)
4. **No multipart/byte-upload route for OS update bundles.** `api/updates.rs:26-65` — `install` only accepts a pre-staged `bundle_path`; the browser-upload half of spec §4.4 ("Browser upload of a signed bundle") has no implementation anywhere in the router. **Done means:** a real multipart handler that writes to `/data/webconfig/uploads/` then calls the existing install logic.
5. **RAUC backend silently falls back to a fabricated mock on a real device.** `update_backend.rs:17-39` — if `rauc` isn't on PATH for any reason on real hardware, the daemon silently presents fake "Slot A v1.0.0 / Slot B v0.9.0" data with no warning logged, indistinguishable from the intentional dev/CI fake-sysroot case. **Done means:** `tracing::warn!` (or harder failure) when falling back to mock on a real root.
6. **`factory_reset` swallows every SQL error and still reboots.** `api/system.rs:273-334` — `let _ =` on every `DELETE FROM` call means a locked/corrupt `config.db` produces a device that looks reset but isn't, with zero operator-visible signal, and it still reboots. **Done means:** log and, for destructive/security-relevant operations, abort the reboot on any failure.
7. **First-boot admin audit entry uses a stale non-ISO timestamp.** `auth.rs:168-175,278-282` — a local `now_iso()` that still emits `t=<epoch>` instead of the RFC3339 format used everywhere else (`api/mod.rs::now_iso`, already fixed and tested elsewhere). This is the *first* audit entry any device ever produces. **Done means:** call the shared `api::now_iso()`; delete the stale local copy.
8. **No request body size limit anywhere in the router**, despite the spec's explicit 15 MB RAM budget (§2) — upload-heavy routes (`/security/tls`, `/security/certs`, `/updates/install`, `/updates/restore`, `/updates/backup`) can buffer arbitrarily large bodies. **Done means:** add `tower_http::limit::RequestBodyLimitLayer` sized to the actual largest legitimate payload.
9. **Compiled-in board manifest fallback fails silently.** `bootstrap.rs:71-99` — a malformed baked-in manifest for a real platform silently becomes `BoardManifest::default()` with no log line at all (contrast: the schema loader does `tracing::warn!` on the same class of failure). Given the manifest drives the entire I/O/dashboard/alerts UI, this needs the same visibility. **Done means:** log at `error!` level and consider surfacing a "degraded manifest" flag to the UI.
10. **Provisional network-apply failure leaves no trace for the operator.** `api/network.rs:99-127` — if `firewall`/`wifi`/`ap` apply fails after `networkd` already went `Provisional`, the HTTP error gives no indication a rollback timer is still ticking in the background. **Done means:** include pending-provisional state in the error response.

### Frontend
11. **Serial config can be saved but is never applied.** `frontend/pages/serial.js` has no "Apply serial" action wired, despite a real, working backend route `POST /api/v1/io/serial/apply` (`routes.rs:149,494-504`). **Done means:** wire the apply button; config saved via `PUT /io` should not silently sit unapplied.
12. **CAN termination-GPIO control is missing from the UI** despite `caps.termination_gpio` being a documented, manifest-level, safety-relevant field (`docs/board-manifest.md:36`). **Done means:** add the toggle to `frontend/pages/can.js`.
13. **MQTT client-cert auth path is entirely unimplemented in the UI.** `frontend/pages/cloud.js` has no cert-store picker and no editable `client_id`, despite both being documented/expected. **Done means:** wire cert selection from the cert store (already implemented server-side in `api/security.rs`) into the cloud page form.
14. **Users → Groups UI may be calling dead endpoints.** `frontend/pages/users.js:15,116,128,136,150` calls `/api/v1/system/groups...` (GET/POST/DELETE/members) which are **not documented in `docs/api-reference.md`** and not mentioned in the spec. This needs an explicit cross-check against `crates/webconfigd/src/api/system.rs`'s actual route table — if the routes truly don't exist, roughly half the Users page 404s in production. **Done means:** either implement+document the groups routes, or remove the dead UI calls.
15. **Dirty-guard (unsaved-changes protection) is bypassed by browser back/forward.** `frontend/core/router.js:23` vs `32-55` — the guard is checked on the programmatic `go()` path but not on the `hashchange` listener, so back/forward navigation or manual hash edits silently discard unsaved form changes on every page. **Done means:** route all navigation, not just programmatic, through the same dirty-check gate.
16. **Two dead, unreachable page modules ship in the bundle.** `frontend/pages/hardware.js` and `frontend/pages/io.js` are not referenced anywhere in `frontend/pages/registry.js`; `io.js` is ironically the most complete monolithic implementation, suggesting an incomplete migration to the split gpio/can/serial/leds/camera pages. **Done means:** delete both, or finish the migration and remove whichever is truly superseded — matters for the spec's <500 KB frontend budget.

### Manifests / Schemas / Yocto
17. **MCU companion schemas referenced but do not exist.** `board-manifests/arches.json` declares `settings_schema: "app:mcu-stm32"`, `acadia.json` declares `"app:mcu-picow"` — neither schema file exists anywhere in `schemas/` or the Yocto layer's files. The MCU settings panel (spec §4.9) has no backing data to render against, independent of the MCU-flash gap in 2.1. **Done means:** author the two real MCU schema files.
18. **`webconfigd.bb` cannot currently fetch a real release artifact.** Both the source and vendor tarball SHA-256 checksums are literal placeholder strings (`TODO_REPLACE_WITH_RELEASED_TARBALL_SHA256`) with no release script in-repo to fill them. **Done means:** stand up the release-artifact pipeline (even a minimal one) that produces real tarballs + checksums.
19. **Polkit allowlist is a generic placeholder, not synced per-platform** as `docs/board-manifest.md` instructs. It matches none of the six manifests' `service_allowlist` arrays — includes `rauc.service`/`mosquitto.service` that aren't allowlisted anywhere, omits Arches-critical units (`nvpmodel.service`, `nv_l4t_bootloader.service`, `jetson_clocks.service`, `nvfancontrol.service`, `nvargus-daemon.service`). **Done means:** generate the polkit rule per-platform from each manifest's `service_allowlist`, or accept a broader rule and document why.
20. **License metadata is self-contradictory and points at a missing file.** `Cargo.toml` declares MIT; README.md and every Yocto `.bb` recipe say CLOSED; `webconfigd.bb`'s `LIC_FILES_CHKSUM` points at a `LICENSE` file that doesn't exist in the repo. **Done means:** pick the real license, fix `Cargo.toml`, and add the `LICENSE` file the recipe already expects.

---

## 4. P2 — Medium priority

### Test coverage (cuts across the whole daemon — call out as its own workstream)
The single largest structural risk in this codebase is that its best-engineered, most security-relevant surfaces have **no HTTP-level test coverage**, meaning the P0/P1 bugs above (and future regressions in the same areas) are invisible to CI:
- Zero HTTP-route-level test for `/network/apply`, `/network/confirm`, `/network/rollback`, `/network/scan`, `/network/connectivity-test` — the flagship feature (spec §4.2).
- Zero test for the `/events` SSE route.
- Zero test for `reboot`, `shutdown`, `root_password`, user delete, **last-admin-delete guard** (a critical safety property), group management.
- Zero test for services start/stop/restart/enabled — exactly where the "always returns ok:true" bug (3.2 / P0-adjacent) lives undetected.
- Zero test for updates `install`/`ota`/`backup` (plaintext+encrypted)/`restore`.
- Zero test for monitoring, logs (except audit indirectly), security exposure/trust/wireguard, io usb/pci/i2c/spi/can/gpio/mcu/platform-panel.
- No test anywhere exercises the `Role::Operator` boundary — only admin vs. viewer is tested.
- No test asserts the session cookie is actually `Secure`+`HttpOnly` on the wire.
- **Done means:** a dedicated test-coverage pass, prioritized in this order: network apply/confirm/rollback → services start/stop/restart → updates install/backup/restore → SSE → security/trust/wireguard → operator-role boundary → the rest.

### Other medium items
- **`std::io::Error` collapses to `ApiError::Internal` everywhere**, discarding the underlying message even from server-side logs (`error.rs:76-80`) — makes field debugging materially harder across the whole daemon. Add `tracing::error!` with the real error at each conversion site.
- **Alert engine metrics `service.<unit>` and `mcu.heartbeat` are unresolvable** (`alert.rs` — self-documented gap) — any alert rule pack referencing them silently never fires.
- **`read_emmc_wear` doesn't implement the spec's `mmc extcsd read` shell-out**, only a sysfs node that may not exist on all kernels — will silently report `None` on hardware the spec says should work.
- **Two coexisting board-manifest loader formats (JSON + YAML) in `manifest.rs`**, only JSON documented — resolve alongside the P0 format decision (2.6).
- **Platform-specific monitoring tiles (Arches GPU/fan/power-mode, Zion bitstream, Joshua PRU, Sequoia sensors) have no confirmed data-source implementation** — needs explicit confirmation of where (if anywhere) this lives, or it's an unfilled spec §4.5 requirement with only manifest-declared tile labels and nothing feeding them.
- **`config_defaults` in `board-manifests/generic.yaml` is undocumented** and not present in the six real per-platform manifests — resolve whether the doc or the manifests are incomplete.
- **Stale-read-then-full-object-write race on every `/api/v1/io` PUT** (frontend `hardware-utils.js:7-21` and page-local equivalents) — no version/ETag guard, so two admins editing different peripherals concurrently clobber each other.
- **Timezone list hardcoded to 28 US-only entries** (`frontend/pages/time.js:11-41`) instead of sourced from tzdata as spec §4.1 requires.
- **`remote-logs.js:27-32` loses remembered host/port/protocol on disable** by sending `remote_syslog: null` instead of `{enabled:false, ...}`.
- **Command palette lacks focus trap / ARIA dialog semantics** (`frontend/components/command-palette.js`) — reimplements its own overlay instead of reusing `modal.js`'s `openModal()`.
- **`LoginThrottle` has unbounded per-username memory growth** with no eviction and no IP-based component or hard lockout (`auth.rs:33-60`) — soft hardening gap.
- **No SSE/connection caps** — unbounded concurrent SSE and TCP connections given the `current_thread` runtime and strict RAM budget; worth a connection-count limit.
- **Documentation/build drift:** Yocto layer directory is `meta-webconfig` on disk but called `meta-soccentric-webconfig` everywhere in the docs; every bitbake path example in the docs is therefore wrong. Fix alongside 2.5/2.6.
- **`Cargo.toml` (webconfigd) pins `async-trait`/`futures-util`/`http` outside the shared `workspace = true` mechanism** used by everything else — small reproducibility drift risk given the vendored offline-build story.

---

## 5. P3 — Low priority / cleanup

- `appliers/wireguard.rs` doesn't validate `allowed_ips` CIDR shape or `endpoint` host:port shape before writing `wg0.conf` — fails safely but late, with a worse error message than the rest of the crate.
- `appliers/ap.rs` hardcodes `wlan0`/`hw_mode=g`/`channel=7` instead of reading from the board manifest — wrong for boards whose AP-capable radio isn't `wlan0` or is 5 GHz-only.
- No schema-version migration scaffold in `store.rs` — fine while `CONFIG_SCHEMA_VERSION` is still 1, will become a real risk the day it bumps.
- `root_password` handler doesn't reject embedded newlines before piping into `chpasswd` stdin — subtle correctness issue, not a security hole.
- `api/network.rs` connectivity test's DNS-resolve step uses `getent hosts` against whatever `/etc/resolv.conf` currently has, not necessarily the operator-configured DNS servers the result string implies was tested.
- Dead code: `api.upload()` (frontend, never called), `IconButton`, `Tabs` component (unused, no ARIA tab semantics if ever adopted), `ConfirmManager::apply_all()` (daemon, unreachable from any route).
- `Table` component lacks `<caption>`/`scope` attributes — minor a11y nit on usb/pci/serial pages.
- No real 404 page in the frontend router — unknown routes silently redirect to Overview.
- Board-manifest field-level asymmetries worth a data-quality pass: Joshua's `pru1` entry is missing the `cores` cap array that `pru0` has (likely copy-paste gap); several platforms declare `thermal-zone.caps.throttle: true` but their dashboard tiles never surface the `throttled` metric (only Sequoia and Arches do); no non-Arches manifest models CAN-termination GPIO even though `docs/board-manifest.md` calls it out as the canonical `caps` example — needs a hardware-truth check per platform, not just a data-consistency fix.
- Pinnacle's `display-lvds0.caps.v2: "display-hmi"` is a forward-reference to a deferred [v2] feature embedded in manifest data with nothing in the engine consuming a `v2` cap key — harmless but should be removed until the feature exists.

---

## 6. What's already solid (do not re-litigate)

To keep the backlog honest and prevent churn on things that work: `webconfig-engine`'s eleven appliers (hostname/time/ssh/networkd/firewall/wifi/ap/can/serial/journald/wireguard) are consistently validate-before-write, rollback-capable, sysroot-gated, and well-tested. `backup.rs`'s Argon2id + ChaCha20-Poly1305 encrypted-backup implementation is correct. `audit.rs`'s append-only guarantee is properly enforced by the absence of a public clear method, matching CLAUDE.md exactly. `tls.rs`, `csrf.rs`, `error.rs`'s `ApiError::NotFound` shape, the CSRF-returns-400 behavior, the session-cookie `Secure` flag, and the app-schema routing-by-`descriptor.id` fix are all confirmed correct against every specific CLAUDE.md rule checked. The frontend has zero violations of the `[hidden]`-vs-`display` rule, zero native `confirm()`/`prompt()` usage, zero hardcoded typography, and a genuinely complete `schema-form.js` implementation of the documented schema format. The Makefile has every target CLAUDE.md describes and none are stubs. Most pages (overview, monitoring, alerts, logs, network/wired/wireless, firewall, ntp, ssh, system-control, backup, updates, services, faults, gpio, usb, pci, camera, gps, ptp, fiveg, power, telemetry, secure-boot) are solid, complete, and correctly wired to the real API.

---

## 7. Recommended phased roadmap

**Phase 0 — Stop the bleeding (P0 items, 2.1–2.6).** Nothing that reports false success or leaks a secret should exist in a codebase heading to production. This phase also unblocks real multi-platform builds (2.5/2.6), which is a prerequisite for testing anything else on real hardware.

**Phase 1 — Close the trust gaps (P1 engine + daemon items, 1–10).** GPIO applier, RAUC variant check, upload route, factory-reset error handling, audit timestamp fix, body-size limits, manifest-load visibility.

**Phase 2 — Frontend parity pass (P1 frontend items, 11–16).** Wire the already-working backend routes (serial apply, CAN termination) into the UI, resolve the Users/Groups endpoint question, fix the dirty-guard gap, remove dead pages.

**Phase 3 — Platform data correctness (P1 manifest/Yocto items, 17–20; P2 manifest items in §4).** Author missing MCU schemas, fix the release pipeline, sync the polkit allowlist per platform, resolve the license contradiction, and do the field-level hardware-truth pass on the five non-reference board manifests.

**Phase 4 — Test-coverage hardening (P2, §4 "Test coverage").** Build out HTTP-level tests for network apply/confirm/rollback, services, updates, SSE, security, and the operator role — in that priority order, since these are exactly the surfaces where Phase 0/1 bugs hid.

**Phase 5 — Polish (P2 remainder + P3).** Error-message quality, accessibility, dead-code removal, hardening (login throttle, connection caps), documentation drift cleanup.

Each phase should close with the existing validation gate (`make build && make test && make lint`) plus, starting in Phase 4, an explicit review of new/changed test coverage against the gap list in §4 — do not consider a phase done until the specific routes/files named above have real assertions, not just "the build passes."
