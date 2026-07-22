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
- `system.apply` runs three appliers in order: `hostname`, `time`, `ssh` (spec
  §4.1). Register every system applier in `appliers::all()` AND validate +
  apply it explicitly in `system::apply` — `apply_all()` is not used on this
  route (it runs platform appliers like networkd/can/fireguard via their own
  section routes).
- The NTP server list lives at `config.system.ntp_servers` (a `Vec<String>`),
  NOT under `config.system.time`. The time applier writes it to a
  `systemd-timesyncd.conf.d/webconfig.conf` drop-in (`[Time]\nNTP=…`) with a
  `FileBackup` for rollback and restarts `systemd-timesyncd` on the real root
  so it picks the new list up; `timedatectl set-ntp` only flips the unit on/off.
- `/system/info` returns per-interface MACs under `macs` (interface→address),
  read from `<sysroot>/sys/class/net/<iface>/address` (loopback skipped), plus
  `uptime_secs` (spec §4.1: "uptime, MACs").
- `network.apply` runs four appliers: `networkd`, `firewall`, `wifi`, `ap` (spec
  §4.2). Each must be validated + applied explicitly here; `network.rollback`
  rolls all four back. Provisional/confirm-window logic keys off the `networkd`
  outcome only (the severing heuristic lives there).
- `config.network.wifi` (remembered SSIDs) and `regulatory_domain` are applied
  by `appliers/wifi.rs` → `/etc/wpa_supplicant/wpa_supplicant.conf` (priority
  sort, `scan_ssid=1` for hidden, 64-hex raw key unquoted, open nets
  `key_mgmt=NONE`) + `iw reg set`. `config.network.ap_mode` is applied by
  `appliers/ap.rs` → hostapd.conf + dnsmasq.d + a static `20-ap.network`
  (192.168.4.1/24, DHCPServer) and enable/disable of `hostapd`/`dnsmasq`.
- Global DNS `config.network.dns_servers` is applied by the networkd applier to
  `/etc/systemd/resolved.conf.d/webconfig.conf` (`[Resolve]\nDNS=…`), validated
  as IP addresses; per-interface DNS stays in each `.network` file.
  `connectivity_test` pings the configured gateway (first interface with one),
  then a public IP, then resolves via the configured DNS — not hardcoded
  192.168.1.1/1.1.1.1.
- `PUT /apps/:id/settings` fires the app schema's `apply_hook`s after commit
  (spec §3/§4.3): `restart` → `systemd.restart(target)`, `reload` →
  `systemctl reload`, `sighup` → `pkill -HUP -x <proc>` (real root only). Hooks
  are looked up by schema file **stem** (the URL `:id`), deduped across fields,
  target-charset-validated, best-effort (a failed verb is recorded + skipped,
  never failing the save), and the side effect is appended to the audit entry.
- §4.4 updates: `UpdateBackend` has `mark_good()` (RAUC `rauc status mark-good`)
  for the manual "confirm booted image" (cancels the bootloader's failed-boot
  rollback), exposed at `POST /api/v1/updates/confirm` (admin only, audited).
  The `diagnostics` bundle (`GET /api/v1/updates/diagnostics`) must carry
  **journal** (`journalctl -n 200 --no-pager`, real root only) AND **network**
  state (the shared `network::enumerate_live(&Sysroot)`, reused by the network
  page so the two never drift) alongside config/slots/dmesg/alert_history/audit.
  `enumerate_live` is the single source for the `/sys/class/net` live snapshot.
- §4.5 monitoring `Vitals` carries: load[3], `mem_available_kb` (NOT free),
  mounts, `temperatures` (from `/sys/class/thermal/thermal_zone*` AND
  `/sys/class/hwmon/hwmon*/temp*_input`, labeled `hwmonN:<chip>:tempK`), per-iface
  net `rx_bytes`/`tx_bytes`, `uptime_secs`, `boot_count` (`journalctl
  --list-boots` on real root, else None), `last_boot_reason` (from
  `webconfig_dir/last-boot-reason` if an MCU/platform daemon wrote it, else
  None). `TempZone.throttled` is READ, never hardcoded false: any
  `thermal_zone*/cdev*/cur_state > 0` ⇒ throttled (fanless SoCs' cdev0 is the
  freq clamp). `None`/false when not reported is correct behavior, not a stub.

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
- Don't validate `authorized_keys` by trusting any non-empty line — reject
  private-key material (`-----BEGIN`) and require each non-comment line to
  start with a known OpenSSH key-type prefix + a base64 blob. The SSH applier
  (`appliers/ssh.rs`) does this; `SshConfig.authorized_keys` is otherwise
  saved-but-never-applied if you forget to add the applier to `system::apply`.
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