# Arches-Configuration Build and Deployment

## 1. Purpose and scope

Arches-Configuration is a Rust workspace containing the web configuration daemon and related Yocto integration. The top-level Makefile is the entry point for host builds, tests, linting, cross-target builds, local execution, and release artifact generation.

## 2. Prerequisites

Install `make`, `rustup`, and the pinned Rust toolchain from `rust-toolchain.toml`, including `rustfmt` and `clippy`. The workspace uses Cargo and includes a `.cargo/config.toml`; cross compilation also requires the corresponding target toolchains. Yocto packaging additionally requires the surrounding Arches-OS checkout and its layer/tooling.

## 3. Setup

Run `make toolchain` once to install rustup and the exact pinned channel with the required components. Review `Cargo.toml` for workspace members and `board-manifests/generic.yaml` for the generic board configuration. Do not run the release or deployment procedures with unreviewed credentials or board settings.

## 4. Build and compile

- `make build` runs `cargo build --workspace` for a host debug build.
- `make release` runs `cargo build --workspace --release` using the repository release profile.
- `make cross` builds the configured x86_64, aarch64, and armv7 release targets; this is conditional on installed Rust target support.
- `make vendor` regenerates the offline vendored crate tarball.
- `make release-artifacts` invokes `tools/release-artifacts.sh` to package release tarballs and calculate checksums for the Yocto recipe.

## 5. Test and lint

- `make test` runs `cargo test --workspace --all-targets`; the tests use a fake sysroot and do not require hardware.
- `make lint` runs `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`.
- `make fmt` formats the workspace in place.
- `make check` runs `build`, `test`, and `lint`.

## 6. Packaging and artifacts

Cargo debug and release outputs are written below `target/`, with target-specific cross outputs below the relevant `target/<triple>/` directories. `make release-artifacts` creates the source/vendor archives and checksum data consumed by `meta-webconfig`/`webconfigd.bb`.

## 7. Deployment, install, and run

- `make run-mock` runs against a temporary fake sysroot and is the hardware-free smoke path.
- `make run` starts the production container and publishes port 8443 on all host interfaces, so the UI is reachable from the local network at `https://<host-ip>:8443`; a self-signed TLS certificate is generated on first boot.
- `make stop` stops the webconfigd container when one is running, otherwise the process listening on `WEBCONFIG_PORT` (default `8443`).
- Yocto installation is performed by the Arches-OS image recipe, not by a standalone installer in this repository. Use the release artifact workflow and the matching OS image build for target deployment.

There is no cloud deployment target in this Makefile. Target installation requires the corresponding Yocto image, board configuration, and target access.

## 8. Cleanup

`make clean` runs `cargo clean` and removes Cargo build outputs. Generated/vendor changes should be reviewed before committing; cleanup does not replace the release artifact workflow.

## 9. Hardware, cloud, and manual verification caveats

The automated test target explicitly uses a fake sysroot, so it cannot validate board permissions, real backends, or service integration. Verify real-host runs only on an approved host/target. Cross builds and Yocto packaging are conditional on toolchains, network access for dependencies, and matching board manifests.
