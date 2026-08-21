# Arches-Configuration

## Project Summary

Arches-Configuration is the Rust workspace for on-device system configuration.

The reusable `webconfig-engine` crate models manifests and schemas, persists state, audits changes, creates backups, monitors the system, and applies changes through focused backends. Appliers cover access points, CAN, firewall, GPIO, hostname, journald, systemd-networkd, serial, SSH, time, Wi-Fi, and WireGuard.

The `webconfigd` daemon supplies bootstrap and state management, authenticated HTTP routes, CSRF protection, TLS, server-sent events, jobs, systemd integration, alert dispatch, and update handling.

API modules expose system, network, security, services, I/O, logs, monitoring, alerts, and updates. `schemas/` contains example and STM32 application schemas. `board-manifests/` contains Arches and generic target data, and `meta-webconfig` packages the service for Yocto.

The Makefile defines debug build, fake-sysroot engine and HTTP integration tests, Clippy with warnings denied, rustfmt checking, and the combined `check` gate.

Unit tests are embedded in `crates/webconfigd/src/tests.rs`; no separate top-level test directory was found, so live hardware behavior remains target-dependent. Statuses distinguish code presence from test evidence.

The daemon depends on system privileges, systemd services, board-specific paths, and a compatible fake or real sysroot.

Review rollback, privilege boundaries, manifest compatibility, and live service behavior before release. Also check restart recovery, concurrent configuration changes, persistence permissions, and the rendered browser assets on the target.

No manual verification is implied by source presence or by a passing host build.

## Features

| # | Name / Description | Implemented? | Test Coverage? | Manual Test |
|---:|---|---|---|---|
| 1 | Rust workspace and webconfig-engine | Implemented | `make build`, `make lint` | [ ] Manual verification required |
| 2 | Manifest and schema parsing | Implemented | Partial: daemon tests | [ ] Manual verification required |
| 3 | Persistent store, backup, and audit | Implemented | Partial | [ ] Manual verification required |
| 4 | Update workflow | Implemented | Partial | [ ] Manual verification required |
| 5 | Network, Wi-Fi, WireGuard, firewall appliers | Implemented | Fake-sysroot integration gate | [ ] Manual verification required |
| 6 | GPIO, CAN, serial, time, SSH, hostname appliers | Implemented | Partial | [ ] Manual verification required |
| 7 | Authenticated HTTP API | Implemented | `make test` | [ ] Manual verification required |
| 8 | CSRF, TLS, and session security | Implemented | Partial | [ ] Manual verification required |
| 9 | SSE, jobs, monitoring, and alerts | Implemented | Partial: daemon tests | [ ] Manual verification required |
| 10 | Yocto packaging and board manifests | Implemented | Unknown | [ ] Manual verification required |

## Validation Commands

`make check`; `make build`; `make test`; `make lint`.

## Existing Documentation

An **on-device, browser-based configuration web UI** for the Arches
Yocto/embedded-Linux platform (NVIDIA Jetson application processor + STM32
real-time co-processor). One small Rust daemon serves a vanilla-JS
frontend over TLS; board-specific data lives in a single board-manifest file
(`board-manifests/arches.json`).

> The full design specification lives in [`docs/webconfig-spec.md`](docs/webconfig-spec.md).

## What it is

A field engineer or operator points a browser at the device
(`https://<device>:8443`) and configures it: system identity, networking,
services, updates, monitoring, logs, alerts, security, and I/O — all behind
authenticated, role-based access with a commit/confirm rollback safety net for
networking changes.

- **One daemon**, `current_thread` tokio runtime, rustls (no OpenSSL), no
  database (JSON + ring files). Target: <15 MB RAM, 3-6 MB stripped binary.
- **Vanilla JS frontend**, no build step, no node at build time, <500 KB total.
- **Schema-driven rendering**: adding a setting is a schema file — zero UI code.
- **Flagship networking rollback**: network changes are provisional; reconnect
  and confirm within a window or they auto-roll-back.
- **Real + mock system interactions**: RAUC/zbus talk to real backends on-device
  and to mocks on the fake-sysroot test harness (no hardware to test).

## Repository layout

```
Cargo.toml                  # workspace, pinned deps, exact release profile
crates/
  webconfig-engine/         # config store, appliers, schema, manifest, alert/monitoring
  webconfigd/               # axum HTTP daemon (TLS, auth, CSRF, 9 section APIs, SSE)
frontend/                   # vanilla JS: index.html, styles.css, schema-form.js, pages.js, app.js
schemas/                    # *.schema.json — schema-driven rendering + validation
board-manifests/            # arches.json (board-specific data) + generic.yaml fallback
meta-webconfig/             # Yocto layer: daemon, static assets, schemas, manifest, hardened unit
docs/                       # spec, API reference, schema format, board-manifest spec, planning/audits
Makefile                    # build | release | test | lint | fmt | vendor | cross | run | clean
```

## Build & test

The repo-root Makefile is the single entry point (per SoCcentric conventions):

```bash
make build    # debug build (host target)
make release  # exact spec release profile (opt-level=z, lto, panic=abort, stripped)
make test     # engine + HTTP integration tests (fake sysroot, no hardware)
make lint     # clippy -D warnings + rustfmt --check
make vendor   # regenerate the offline crate tarball for reproducible builds
make cross    # build all three cross-targets: x86_64, aarch64, armv7
make run      # run the daemon against a tempdir fake sysroot (dev)
```

Validation gate (used by the git workflows and CI): `make build && make test && make lint`.

## Cross-targets

`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`armv7-unknown-linux-gnueabihf`. Vendored crates + `CARGO_NET_OFFLINE=1` make
builds fully offline and reproducible.

## Deploying (Yocto)

Add the layer and packages to your image:

```bitbake
IMAGE_INSTALL:append = " webconfigd webconfig-static webconfig-schemas webconfig-manifest"
```

Map your BSP `MACHINE` to its board manifest in
`meta-webconfig/recipes-core/webconfig/webconfig-manifest.bbappend`.
See [`meta-webconfig/README.md`](meta-webconfig/README.md).

## Documentation

- [Design specification](docs/webconfig-spec.md)
- [REST API reference](docs/api-reference.md)
- [Schema format](docs/schema-format.md)
- [Board-manifest format](docs/board-manifest.md)
- [Production-readiness audit](docs/planning/MasterPlan.md) and its [acceptance checklist](docs/planning/Checkpoint.md)

## License

CLOSED (SoCcentric internal IV&V tooling).