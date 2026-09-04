# SoCcentric WebConfig — top-level Makefile.
#
# Per SoCcentric global CLAUDE.md §12, the repo-root Makefile is the single
# entry point for build/test/lint, wrapping the underlying tool (cargo). The
# git workflows (§9 step 6) run `make build && make test && make lint`.
#
# Targets:
#   make toolchain  — install rustup + the exact pinned toolchain (one-time
#                     host setup; see rust-toolchain.toml)
#   make build      — build the production webconfigd Docker image (fleet
#                     default). Cross builds (arm64/armv7) and container
#                     logs/clean live in docker/Makefile: make -C docker …
#   make run        — run the webconfigd container, publish 8443 on the LAN
#                     (UI at https://<host-ip>:8443; stop with make stop)
#   make test       — engine + HTTP integration tests (fake sysroot, no HW)
#   make lint       — clippy + rustfmt --check
#   make fmt        — format in place
#   make check      — validation gate: build && test && lint
#   make vendor     — (re)generate the vendored offline crate tarball
#   make release-artifacts — package release tarballs + checksums for Yocto
#   make run-mock   — run against a tempdir fake sysroot (mock backends, no HW)
#   make stop       — stop the process listening on the WebConfig TCP port
#   make clean      — remove target/

# Install rustup (if missing) plus the exact pinned toolchain, components,
# and cross targets declared in rust-toolchain.toml (spec §6 host/CI parity).
# Idempotent — safe to re-run. Needs network access to static.rust-lang.org /
# sh.rustup.rs; not usable in network-restricted sandboxes.
.DEFAULT_GOAL := help

toolchain: ## Install rustup + the exact pinned toolchain (one-time host setup)
	@set -e; \
	export PATH="$$HOME/.cargo/bin:$$PATH"; \
	if ! command -v rustup >/dev/null 2>&1; then \
		echo "==> rustup not found; installing"; \
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
			| sh -s -- -y --default-toolchain none --profile minimal; \
	fi; \
	echo "==> installing toolchain $(RUST_CHANNEL) (rustfmt, clippy)"; \
	rustup toolchain install $(RUST_CHANNEL) --profile minimal \
		--component rustfmt --component clippy; \
	echo "==> adding cross targets: $(TARGETS)"; \
	rustup target add --toolchain $(RUST_CHANNEL) $(TARGETS); \
	echo "==> ready: $$(rustup run $(RUST_CHANNEL) rustc --version)"

# Prefer the standard rustup installation even when the caller's shell has not
# sourced ~/.cargo/env; an explicit CARGO override still takes precedence.
CARGO ?= $(if $(wildcard $(HOME)/.cargo/bin/cargo),$(HOME)/.cargo/bin/cargo,cargo)
WEBCONFIG_PORT ?= 8443
export WEBCONFIG_PORT
TARGETS := x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf
# Pinned channel, read straight from rust-toolchain.toml so this target can't
# drift from the pin.
RUST_CHANNEL := $(shell grep -m1 '^channel' rust-toolchain.toml | sed -E 's/.*"([^"]*)".*/\1/')

.PHONY: help build test lint fmt vendor release-artifacts run run-mock stop clean check toolchain

# Bare `make` prints help rather than building, so a stray invocation never
# kicks off a network install or a build by surprise.


test: ## Engine + HTTP integration tests (fake sysroot, no hardware)
	$(CARGO) test --workspace --all-targets

lint: ## clippy -D warnings + rustfmt --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(CARGO) fmt --all -- --check

fmt: ## Format the workspace in place
	$(CARGO) fmt --all

# Regenerate vendored sources for offline builds (spec §2). Run after any
# dependency change; commit the result.
vendor: ## Regenerate the vendored offline crate tarball
	$(CARGO) vendor --versioned-dirs vendor

# Package the release artifacts webconfigd.bb fetches (source + vendor
# tarballs) and print their real SHA-256 checksums (Checkpoint 3.2). Requires
# `make vendor` to have been run at least once.
release-artifacts: ## Package release tarballs + compute real checksums for webconfigd.bb
	@./tools/release-artifacts.sh

# Mock mode: run against a tempdir fake sysroot so no hardware is required and
# no host side effects occur (spec §6). Vitals read as zeros; backends are
# mocks. The tempdir is wiped by the OS on reboot; state does not persist.
run-mock: ## Run against a tempdir fake sysroot (mock backends, no hardware)
	WEBCONFIG_SYSROOT=$$(mktemp -d) \
	WEBCONFIG_DATA_DIR=$$WEBCONFIG_SYSROOT/data \
	RUST_LOG=info \
	$(CARGO) run -p webconfigd -- --

# Stop the process holding the default daemon port, or an overridden
# WEBCONFIG_PORT. A clear port is already in the desired state.
stop: ## Stop the container (make run) or bare daemon on WEBCONFIG_PORT
	@if docker ps --format '{{.Names}}' 2>/dev/null | grep -qx webconfigd; then \
		echo '==> stopping webconfigd container'; \
		$(MAKE) -C docker stop; \
		exit 0; \
	fi; \
	case "$$WEBCONFIG_PORT" in \
		[1-9]|[1-9][0-9]|[1-9][0-9][0-9]|[1-9][0-9][0-9][0-9]|[1-9][0-9][0-9][0-9][0-9]) ;; \
		*) echo "WEBCONFIG_PORT must be an integer between 1 and 65535" >&2; exit 2 ;; \
	esac; \
	if [ "$$WEBCONFIG_PORT" -lt 1 ] || [ "$$WEBCONFIG_PORT" -gt 65535 ]; then \
		echo "WEBCONFIG_PORT must be an integer between 1 and 65535" >&2; exit 2; \
	fi; \
	fuser "$$WEBCONFIG_PORT/tcp" >/dev/null 2>&1; status=$$?; \
	if [ $$status -eq 0 ]; then \
		fuser -k -TERM "$$WEBCONFIG_PORT/tcp" >/dev/null || exit $$?; \
		for attempt in 1 2 3 4 5; do \
			if ! fuser "$$WEBCONFIG_PORT/tcp" >/dev/null 2>&1; then \
				echo "Stopped process listening on port $$WEBCONFIG_PORT"; exit 0; \
			fi; \
			sleep 1; \
		done; \
		echo "Port $$WEBCONFIG_PORT is still in use after SIGTERM" >&2; exit 1; \
	fi; \
	if [ $$status -eq 1 ]; then \
		echo "No process is listening on port $$WEBCONFIG_PORT"; \
	else \
		echo "Unable to inspect port $$WEBCONFIG_PORT" >&2; exit $$status; \
	fi

clean: ## Remove target/
	$(CARGO) clean

# ── Docker (fleet-standard build) ────────────────────────────────────────────
# The default build is the production container image (daemon + frontend +
# schemas + board manifest) — see docker/Dockerfile. `make build` = Docker.
# Cross builds (arm64/armv7) and container logs/clean stay in
# docker/Makefile: `make -C docker build-arm64`, `make -C docker logs`, …

build: ## Build the production webconfigd image in Docker (fleet default)
	$(MAKE) -C docker build IMAGE_NAME=arches-webconfigd

# Publish 8443 on every host interface so the UI is reachable from other
# machines on the LAN (https://<host-ip>:8443). First boot generates a
# self-signed TLS cert into the `webconfig-data` volume — accept the browser
# warning. Stop it with `make stop`.
run: ## Run the webconfigd container, publish port 8443 on the LAN
	@$(MAKE) -C docker run IMAGE_NAME=arches-webconfigd
	@sleep 1; \
	lan_ip=$$(hostname -I 2>/dev/null | awk '{print $$1}'); \
	if [ -z "$$lan_ip" ]; then \
		lan_ip=$$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<NF;i++) if($$i=="src") print $$(i+1)}' | head -1); \
	fi; \
	echo; \
	echo '  ==> webconfig UI:  https://localhost:8443'; \
	if [ -n "$$lan_ip" ]; then \
		echo "                     https://$$lan_ip:8443   (local network)"; \
	fi; \
	echo '  ==> self-signed TLS on first boot — accept the browser warning'; \
	echo '  ==> stop it with: make stop'

# Convenience alias: full validation gate (§9 step 6).
check: build test lint ## Full validation gate: build (Docker) && test && lint

# ── Help ──────────────────────────────────────────────────────────────────────
# Targets are self-documenting: text after `## ` on a target line is picked up
# by `make help` automatically, so the listing can never drift from the rules.
.PHONY: help
help: ## Show this help
	@printf '\n  \033[1m%s\033[0m\n' 'Arches-Configuration'
	@printf '  Usage: make <target> [VAR=value ...]\n'
	@printf '\n'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z0-9_.\/-]+:.*?## / { printf "    \033[36m%-24s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@printf '\n'
