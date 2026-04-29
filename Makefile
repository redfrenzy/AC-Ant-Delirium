# ac_ant_delirium - Industrial Governance Engine
# ==================================================
# This Makefile provides automated commands for the
# Aesthetic Core governance plugin lifecycle.

PLUGIN_DIR := ../plugins
RAYON_PLUGIN_DIR := ../plugins/AC_Rayon-Genetics
WASM_TARGET := wasm32-wasip1
BIN_NAME := delirium_cli

# The native plugin binary. This is the AUTHORITATIVE governance
# checkpoint. It lives OUTSIDE the project tree (in ../plugins/)
# so that `cargo build` cannot modify it during compilation.
# This prevents circular dependency: the code being audited
# cannot influence the auditor.
NATIVE_PLUGIN := $(PLUGIN_DIR)/ac_ant_delirium
RAYON_NATIVE_PLUGIN := $(RAYON_PLUGIN_DIR)/ac_ant_delirium
WASM_PLUGIN := $(PLUGIN_DIR)/ac_ant_delirium.wasm

# The host triple. Required because .cargo/config.toml sets
# a default target of wasm32-unknown-unknown, which would
# otherwise prevent native builds.
HOST_TARGET := x86_64-unknown-linux-gnu

# -----------------------------------------------
# make plugin
# -----------------------------------------------
# Compiles the governance CLI as a native Linux binary
# and deploys it to ../plugins/ac_ant_delirium.
#
# Why --target $(HOST_TARGET)?
# Because .cargo/config.toml sets target=wasm32-unknown-unknown
# by default. Without an explicit --target, cargo would try to
# build a native binary for wasm32, which is not what we want.
# -----------------------------------------------
.PHONY: plugin
plugin:
	@echo "[GOVERNANCE] Building native governance plugin..."
	cargo build --release --target $(HOST_TARGET) --bin $(BIN_NAME)
	@mkdir -p $(PLUGIN_DIR)
	cp target/$(HOST_TARGET)/release/$(BIN_NAME) $(NATIVE_PLUGIN)
	@chmod +x $(NATIVE_PLUGIN)
	@echo "[GOVERNANCE] Native plugin deployed: $(NATIVE_PLUGIN)"
	@ls -lh $(NATIVE_PLUGIN)

# -----------------------------------------------
# make plugin-rayon
# -----------------------------------------------
# Compiles the governance CLI as a native Linux binary
# and deploys it to ../plugins/AC_Rayon-Genetics/ac_ant_delirium,
# which is the binary consumed by AC_Rayon-Genetics/build.rs.
# -----------------------------------------------
.PHONY: plugin-rayon
plugin-rayon:
	@echo "[GOVERNANCE] Building native governance plugin for AC_Rayon-Genetics..."
	cargo build --release --target $(HOST_TARGET) --bin $(BIN_NAME)
	@mkdir -p $(RAYON_PLUGIN_DIR)
	cp target/$(HOST_TARGET)/release/$(BIN_NAME) $(RAYON_NATIVE_PLUGIN)
	@chmod +x $(RAYON_NATIVE_PLUGIN)
	@echo "[GOVERNANCE] Native Rayon plugin deployed: $(RAYON_NATIVE_PLUGIN)"
	@ls -lh $(RAYON_NATIVE_PLUGIN)

# -----------------------------------------------
# make wasm
# -----------------------------------------------
# Compiles the governance CLI as a portable WASI binary.
# This is for DISTRIBUTION only — when you want to share
# the plugin with other projects or platforms.
# Requires a WASI runtime (wasmtime) to execute.
# -----------------------------------------------
.PHONY: wasm
wasm:
	@echo "[GOVERNANCE] Building WASI portable plugin..."
	cargo build --target $(WASM_TARGET) --release --bin $(BIN_NAME)
	@mkdir -p $(PLUGIN_DIR)
	cp target/$(WASM_TARGET)/release/$(BIN_NAME).wasm $(WASM_PLUGIN)
	@echo "[GOVERNANCE] WASM plugin deployed: $(WASM_PLUGIN)"
	@ls -lh $(WASM_PLUGIN)

# -----------------------------------------------
# make check
# -----------------------------------------------
# Runs governance audit using the native binary directly.
# -----------------------------------------------
.PHONY: check
check:
	cargo run --bin $(BIN_NAME) --target $(HOST_TARGET) -- --api ant_delirium_config

# -----------------------------------------------
# make clean
# -----------------------------------------------
.PHONY: clean
clean:
	cargo clean
