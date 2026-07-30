# CLAUDE.md - Arches-Configuration (WebConfig)

Master SoCcentric CLAUDE.md applies. This file adds local knowledge only. See [project_specs.md](project_specs.md) for what this repo is and its architecture.

## Commands

* Build: `make build` (debug, host target) — wraps `cargo build`
* Release: `make release` — exact spec profile (opt-level=z, lto, codegen-units=1, panic=abort, stripped)
* Test: `make test` — engine unit tests + in-crate HTTP integration tests (`crates/webconfigd/src/tests.rs`) against a tempdir fake sysroot, no hardware
* Lint: `make lint` — `cargo clippy -D warnings` + `cargo fmt --check`
* Validation gate (global section-9 step 6): `make build && make test && make lint`
* Cross: `make cross` — x86_64, aarch64-unknown-linux-gnu, armv7-gnueabihf
* Vendor: `make vendor` — regenerate offline crate tarball; build with `CARGO_NET_OFFLINE=1`

## Do

* Build/test via the root Makefile only (global section-12). Never bypass with ad-hoc cargo commands in CI/PR validation.
* Stop a local daemon and clear its listener with `make stop`; override the default TLS port with `make stop WEBCONFIG_PORT=<port>`.
* Keep the in-crate test pattern (`src/main.rs` → `src/tests.rs`) for the binary crate — external `tests/` integration tests cannot import `crate::routes`/`crate::state`.
* Never silently stub a system interaction with a TODO — pick real vs mock explicitly via `make_backend`/`make_client`.
* When a struct derives `Default`, every field type must impl `Default` (e.g. `IoKind` needed a `Default` derive so `Peripheral: Default` compiles — the derive fails the whole crate otherwise).
* State assumptions in one line where a real interaction is narrowed (e.g. updates install takes a server-staged `bundle_path`, not multipart, in v1).
* Match webconfig-engine handler contracts exactly when wiring the frontend (Vitals/UnitState/NetworkConfig/SystemConfig/AlertRule/SlotStatus/Peripheral shapes — see `docs/api-reference.md`).
* Never let a component class set `display` unconditionally — `base.css` has a global `[hidden] { display: none !important; }` specifically so no component can re-break the hidden-attribute-vs-class-display bug that hit the old `.overlay`/`.banner` rules.
* Native dialogs (`confirm()`/`prompt()`) are banned in the frontend — use `confirmDialog`/`promptDialog`/`formDialog` from `frontend/components/modal.js` instead.
* Register every system applier in `appliers::all()` AND validate + apply it explicitly in `system::apply` / `network::apply` — `apply_all()` is not used on those routes.
* `/system/info` returns per-interface MACs under `macs` plus `uptime_secs` (spec section-4.1).
* For "secondary text" use the `.text-sm` utility class (with `.muted` for color) — never inline `style: 'font-size:...'`.

## Don't

* Don't route app-schema validation by `name.starts_with("app:")` on the validator key — validators are keyed by the schema file stem, while app-ness is decided by `descriptor.id`. Route via `descriptor.id.starts_with("app:")`. (Previously app settings were never server-validated.)
* Don't add a `DELETE /logs/audit` route or an `AuditLog::clear()` method — the audit log is append-only (spec section-4.8). Only a full factory reset wipes it, inlined in `system::factory_reset`.
* Don't set the session cookie `with_secure(false)` — the daemon is TLS-only (`server.rs`), so `with_secure(true)` is correct (spec section-4.8 httpOnly+Secure).
* Don't expect the CSRF guard to return 403. Missing/invalid `X-CSRF-Token` on a mutating protected route returns 400 BadRequest (`csrf.rs`). Tests must assert `StatusCode::BAD_REQUEST`, not FORBIDDEN.
* Don't validate `authorized_keys` by trusting any non-empty line — reject private-key material (`-----BEGIN`) and require each non-comment line to start with a known OpenSSH key-type prefix + a base64 blob (`appliers/ssh.rs`). `SshConfig.authorized_keys` is otherwise saved-but-never-applied if you forget to add the applier to `system::apply`.
* Don't assume `ApiError::NotFound` takes a message — it is a unit variant. Use `ApiError::NotFound` directly.
* Host cargo toolchain IS available (`~/.cargo/bin` on PATH via rustup, channel pinned by `rust-toolchain.toml`) — `make build && make test && make lint` runs and passes locally; no need to skip compile-verification.
* Don't add node/build steps to the frontend. Vanilla JS (native ES modules, no bundler), <500 KB, no build step, no node at build time (spec section-2).
* Don't hardcode I/O or board behavior in code — it lives in the board manifest (`board-manifests/arches.json`) as data (spec section-4.9, section-3).
* Don't reintroduce `meta-soccentric-webconfig` as a path in docs — the Yocto layer directory is `meta-webconfig` on disk. The `soccentric-webconfig` string still appears legitimately as the Yocto collection name in `meta-webconfig/conf/layer.conf`'s `LAYERSERIES_COMPAT_`/`LAYERDEPENDS_`/`LAYERSERIES_COREVERSION_` overrides — a different concept from the directory name and correct as-is.
* `MasterPlan.md` and `Checkpoint.md` live in `docs/planning/`, not repo root — keep the root to files with tooling/fleet contracts (README.md, CLAUDE.md, CHANGELOG.md, REPO_MAP.md). The two files cross-reference each other by bare filename since they're in the same directory — don't split them apart.
* Don't assume CI and `Cargo.toml` agree on the Rust version — `.github/workflows/ci.yml` pins 1.75.0 while `Cargo.toml`/`rust-toolchain.toml` declare `rust-version = "1.85"`. Verify which is authoritative before bumping either in isolation.
* Don't assume this is the only `-Configuration` pattern in the fleet — see [project_specs.md](project_specs.md).

## Validation gate - .claude/validate.sh

```
./.claude/validate.sh quick   # seconds: board-manifest and schema JSON parse, meta-webconfig layer wiring, `clippy -D warnings` + `rustfmt --check`
./.claude/validate.sh full    # + `make check` = build && test && lint
```

Exit codes follow the fleet contract: 0 pass, 1-76 fail, 77 means a check never ran (missing toolchain) - which is not a pass. Runs automatically via the Stop hook in .claude/settings.json, on demand via /validate, and as part of any loop the engine executes here.

Human-owned: the Loop Engine runs this script and never edits it. If a check is wrong, fix it yourself or raise it in the loop's Lessons Learned.

<!-- BEGIN loop-engine -->
## Do's and Don'ts (maintained by the Loop Engine)

Appended by the Loop Engine on loop close; curated by a human when approving a loop out of in-review/. Nothing outside this fence is engine-writable.

<!-- END loop-engine -->
