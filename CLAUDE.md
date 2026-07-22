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

- Backup secrets via `POST /updates/backup` with a JSON body `{ "passphrase" }`
  → returns a passphrase-encrypted envelope (Argon2id key derivation +
  ChaCha20-Poly1305 AEAD via `ring`, base64 fields). `GET /updates/backup`
  always redacts secrets (no plaintext secret ever leaves the device). Restore
  detects `.encrypted` in the uploaded file and prompts for the passphrase;
  `/updates/restore` decrypts then commits. Passphrase travels in the POST
  body, never a query string.
- `ring` is already in the vendored supply chain (rustls/rcgen/tokio-rustls all
  depend on it) — adding it as a direct `webconfig-engine` dep introduces no
  new external crate. The `ring::aead` API: `UnboundKey::new(alg, &key)`,
  `LessSafeKey::new(unbound)`, `Nonce::assume_unique_for_key([u8;12])`,
  `seal_in_place_append_tag(nonce, Aad::empty(), &mut Vec)`; RNG via
  `ring::rand::SystemRandom` + `SecureRandom::fill(&mut [u8])`.
- Keep a config backup snapshot: `config_state_backup` table holds the previous
  committed row; `commit()` snapshots then upserts inside one `BEGIN/COMMIT`
  (spec §3 "atomic write + backup copy"). `ConfigStore::load_backup()` reads it.

- Build/test via the root Makefile only (global §12). Never bypass with ad-hoc
  `cargo` commands in CI/PR validation.
- Stop a local daemon and clear its listener with `make stop`; override the
  default TLS port with `make stop WEBCONFIG_PORT=<port>`.
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

- Frontend is native ES modules (`<script type="module" src="/core/app.js">`),
  not classic `defer` scripts — one file per component/page under
  `frontend/{core,components,pages}/`, wired with real `import`/`export`, no
  bundler (browsers resolve the graph directly from static files). Add a page
  by dropping a module in `frontend/pages/` and one line in
  `frontend/pages/registry.js`; add a widget under `frontend/components/`.
  Styles are split under `frontend/styles/*.css`, composed by `main.css` via
  plain `@import` (no preprocessor).
- Never let a component class set `display` unconditionally — `base.css` has
  a global `[hidden] { display: none !important; }` specifically so no
  component can ever re-break the hidden-attribute-vs-class-display bug that
  hit the old `.overlay`/`.banner` rules.
- Native dialogs (`confirm()`/`prompt()`) are banned in the frontend — they
  block the main thread and look dated. Use `confirmDialog`/`promptDialog`/
  `formDialog` from `frontend/components/modal.js` instead.

## Don't

- Don't route app-schema validation by `name.starts_with("app:")` on the
  validator key — validators are keyed by the schema file **stem**
  (`app-example`), while app-ness is decided by `descriptor.id` (`app:example`).
  Route via `descriptor.id.starts_with("app:")` and target
  `config.app_settings[<stem>]`; top-level schemas target `config[<stem>]`.
  (Previously app settings were never server-validated.)
- Don't add a `DELETE /logs/audit` route or an `AuditLog::clear()` method — the
  audit log is append-only (spec §4.8). Only a full factory reset wipes it, and
  that wipe is inlined in `system::factory_reset` (it does more than audit:
  config_state, users, groups). `AuditLog` has no public clear path.
- Don't set the session cookie `with_secure(false)` — the daemon is TLS-only
  (server.rs), so `with_secure(true)` is correct (spec §4.8 httpOnly+Secure).
  First-boot admin creation happens over the same TLS listener.
- Don't expect the CSRF guard to return 403. Missing/invalid `X-CSRF-Token` on
  a mutating protected route returns **400 BadRequest** (`csrf.rs`). Tests must
  assert `StatusCode::BAD_REQUEST`, not `FORBIDDEN`.
- Don't assume `ApiError::NotFound` takes a message — it is a unit variant.
  Use `ApiError::NotFound` directly.
- Host cargo toolchain IS available (`~/.cargo/bin` on PATH via rustup,
  channel pinned by `rust-toolchain.toml`) — `make build && make test &&
  make lint` runs and passes locally; no need to skip compile-verification.
- Don't add node/build steps to the frontend. Vanilla JS (native ES modules
  are fine — browsers run them with no bundler), <500 KB, no build step, no
  node at build time (spec §2).
- Don't hardcode I/O or per-platform behavior in code — it lives in the board
  manifest (`board-manifests/<platform>.json`) as data (spec §4.9, §3).