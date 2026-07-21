# CLAUDE.md — SoCcentric WebConfig (Arches-Configuration)

Master SoCcentric CLAUDE.md applies. This file adds local knowledge only.

This repo is the home of the cross-platform **WebConfig** on-device UI: one Rust
daemon + vanilla-JS frontend + engine, with the six platforms (Arches, Acadia,
Zion, Pinnacle, Joshua, Sequoia) differing only in a board-manifest data file.
See `docs/webconfig-spec.md` for the full spec.

- Build: `make build` (debug, host target) — wraps `cargo build`
- Release: `make release` — exact spec profile (opt-level=z, lto, codegen-units=1,
  panic=abort, stripped)
- Test: `make test` — engine unit tests + in-crate HTTP integration tests
  (`crates/webconfigd/src/tests.rs`) against a tempdir fake sysroot, no hardware
- Lint: `make lint` — `cargo clippy -D warnings` + `cargo fmt --check`
- Validation gate (global §9 step 6): `make build && make test && make lint`
- Cross: `make cross` — x86_64, aarch64-unknown-linux-gnu, armv7-gnueabihf
- Vendor: `make vendor` — regenerate offline crate tarball; build with
  `CARGO_NET_OFFLINE=1`

## Do

- Build/test via the root Makefile only (global §12). Never bypass with ad-hoc
  `cargo` commands in CI/PR validation.
- Integration tests for the binary crate `webconfigd` live as
  `#[cfg(test)] mod tests;` in `src/main.rs` → `src/tests.rs`. Binary crates do
  not expose a lib target to the `tests/` directory, so external integration
  tests cannot import `crate::routes`/`crate::state`. Keep the in-crate pattern.
- Use `tower::ServiceExt::oneshot` + `http_body_util::BodyExt` to drive the axum
  router in tests against a tempdir `Sysroot` (fake sysroot, parallel-safe).
- Keep system interactions behind narrow traits with BOTH real and mock impls
  (`UpdateBackend`, `SystemdClient`). `make_backend(&Sysroot)` /
  `make_client(&Sysroot)` pick real vs mock from `Sysroot::is_real_root()`.
  Never silently stub with a TODO.
- When a struct derives `Default`, every field type must impl `Default`. (Fixed
  `IoKind` to derive `Default` so `Peripheral: Default` compiles — the derive
  fails the whole crate otherwise.)
- State assumptions in one line where a real interaction is narrowed (e.g.
  updates install takes a server-staged `bundle_path`, not multipart, in v1).
- Match `webconfig-engine` handler contracts exactly when wiring the frontend
  (Vitals/UnitState/NetworkConfig/SystemConfig/AlertRule/SlotStatus/Peripheral
  shapes — see `docs/api-reference.md`).

## Don't

- Don't use top-level `await` in classic scripts. `frontend/*.js` load with
  `defer` (not `type="module"`); wrap async boot logic in an IIFE or call a
  non-async entry directly. (`bootstrap()` is a sync call that does its own
  async work internally.)
- Don't expect the CSRF guard to return 403. Missing/invalid `X-CSRF-Token` on
  a mutating protected route returns **400 BadRequest** (`csrf.rs`). Tests must
  assert `StatusCode::BAD_REQUEST`, not `FORBIDDEN`.
- Don't assume `ApiError::NotFound` takes a message — it is a unit variant.
  Use `ApiError::NotFound` directly.
- Don't compile-verify on this host without checking first — no host cargo
  toolchain was available when this code was written, so it was authored
  carefully against the real handler contracts but NOT compile-verified here.
  The gate is `make build && make test && make lint` in CI. Run that before any
  push (global §9 step 6).
- Don't add node/build steps to the frontend. Vanilla JS, <500 KB, no build
  step, no node at build time (spec §2).
- Don't hardcode I/O or per-platform behavior in code — it lives in the board
  manifest (`board-manifests/<platform>.json`) as data (spec §4.9, §3).