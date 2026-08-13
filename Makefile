# ac_ant_delirium - Industrial Governance Engine
# ==================================================
# This Makefile provides automated commands for the
# Aesthetic Core governance plugin lifecycle.

WASM_TARGET := wasm32-wasip1
BIN_NAME := antdelirium

# The host triple. Required because .cargo/config.toml sets
# a default target of wasm32-unknown-unknown, which would
# otherwise prevent native builds.
HOST_TARGET := x86_64-unknown-linux-gnu
RUSTC_SYSROOT := $(shell rustc --print sysroot)
CARGO_HOME_DIR := $(shell if [ -n "$$CARGO_HOME" ]; then printf '%s' "$$CARGO_HOME"; else printf '%s/.cargo' "$$HOME"; fi)
RELEASE_RUSTFLAGS := -D warnings --remap-path-prefix=$(CURDIR)=antdelirium-src --remap-path-prefix=$(RUSTC_SYSROOT)=rust-toolchain --remap-path-prefix=$(CARGO_HOME_DIR)=cargo-home

# -----------------------------------------------
# make binary
# -----------------------------------------------
# Compiles the sole authoritative native executable. Installation and Cargo
# integration are separate, controlled phases and never copy into a project.
#
# Why --target $(HOST_TARGET)?
# Because .cargo/config.toml sets target=wasm32-unknown-unknown
# by default. Without an explicit --target, cargo would try to
# build a native binary for wasm32, which is not what we want.
# -----------------------------------------------
.PHONY: binary
binary:
	@echo "[GOVERNANCE] Building authoritative antdelirium binary..."
	RUSTFLAGS="$(RELEASE_RUSTFLAGS)" cargo build --release --locked --target $(HOST_TARGET) --bin $(BIN_NAME)
	@echo "[GOVERNANCE] Artifact: target/$(HOST_TARGET)/release/$(BIN_NAME)"

# -----------------------------------------------
# make release-nondisclosure
# -----------------------------------------------
# Builds with reproducible logical source prefixes, then rejects physical
# maintainer, service-state, or service-socket paths embedded in the ELF.
# Administrative names may remain while rollback code is compiled; names do
# not grant authority and are removed only with the legacy implementation.
# -----------------------------------------------
.PHONY: release-nondisclosure
release-nondisclosure: binary
	@if strings -a target/$(HOST_TARGET)/release/$(BIN_NAME) | rg -q '/home/|/Users/|[A-Za-z]:\\\\Users\\\\|/var/lib/antdelirium|/run/antdelirium'; then \
		strings -a target/$(HOST_TARGET)/release/$(BIN_NAME) | rg -m 20 '/home/|/Users/|[A-Za-z]:\\\\Users\\\\|/var/lib/antdelirium|/run/antdelirium'; \
		exit 1; \
	fi
	@echo "[GOVERNANCE] Release contains no protected or maintainer physical path."

# -----------------------------------------------
# make wasm
# -----------------------------------------------
# Compiles the same authoritative source as a portable WASI artifact.
# -----------------------------------------------
.PHONY: wasm
wasm:
	@echo "[GOVERNANCE] Building WASI portable plugin..."
	cargo build --target $(WASM_TARGET) --release --bin $(BIN_NAME)
	@echo "[GOVERNANCE] Artifact: target/$(WASM_TARGET)/release/$(BIN_NAME).wasm"

# -----------------------------------------------
# make check
# -----------------------------------------------
# Runs governance audit using the native binary directly.
# -----------------------------------------------
.PHONY: check
check:
	cargo run --bin $(BIN_NAME) --target $(HOST_TARGET) -- --api ant_delirium_config

# -----------------------------------------------
# make ci
# -----------------------------------------------
# Reproduces the native quality gate used by automation. Rust warnings from
# this package are errors; the known advisory build-hook diagnostic remains a
# Cargo warning until the legacy hook is removed in the global migration.
# -----------------------------------------------
.PHONY: ci
ci:
	cargo fmt --all -- --check
	RUSTFLAGS="-D warnings" cargo check --locked --all-targets --target $(HOST_TARGET)
	RUSTFLAGS="-D warnings" cargo test --locked --all-targets --target $(HOST_TARGET) --no-fail-fast
	$(MAKE) release-nondisclosure

# -----------------------------------------------
# make clean
# -----------------------------------------------
.PHONY: clean
clean:
	cargo clean
