# SoCcentric WebConfig — top-level Makefile.
#
# Per SoCcentric global CLAUDE.md §12, the repo-root Makefile is the single
# entry point for build/test/lint, wrapping the underlying tool (cargo). The
# git workflows (§9 step 6) run `make build && make test && make lint`.
#
# Targets:
#   make build      — debug build of the daemon (host target)
#   make release    — release build with the exact spec §2 profile
#   make test       — engine + HTTP integration tests (fake sysroot, no HW)
#   make lint       — clippy + rustfmt --check
#   make fmt        — format in place
#   make vendor     — (re)generate the vendored offline crate tarball
#   make cross      — build all three cross-targets (CI parity)
#   make run        — run the daemon against a tempdir fake sysroot (dev)
#   make clean      — remove target/

CARGO ?= cargo
TARGETS := x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf

.PHONY: build release test lint fmt vendor cross run clean check

build:
	$(CARGO) build --workspace

release:
	$(CARGO) build --workspace --release

test:
	$(CARGO) test --workspace --all-targets

lint:
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(CARGO) fmt --all -- --check

fmt:
	$(CARGO) fmt --all

# Regenerate vendored sources for offline builds (spec §2). Run after any
# dependency change; commit the result.
vendor:
	$(CARGO) vendor --versioned-dirs vendor

# Cross-compile parity check for CI (spec §6).
cross:
	@for t in $(TARGETS); do \
		echo "==> building $$t"; \
		$(CARGO) build --workspace --release --target $$t || exit 1; \
	done

# Dev run against a tempdir fake sysroot so no hardware is required (spec §6).
run:
	WEBCONFIG_SYSROOT=$$(mktemp -d) \
	WEBCONFIG_DATA_DIR=$$WEBCONFIG_SYSROOT/data \
	RUST_LOG=info \
	$(CARGO) run -p webconfigd -- --

clean:
	$(CARGO) clean

# Convenience alias: full validation gate (§9 step 6).
check: build test lint