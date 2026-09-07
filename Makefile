# ac_ant_delirium - Industrial Governance Engine
# ==================================================
# This Makefile provides automated commands for the
# Aesthetic Core governance plugin lifecycle.

WASM_TARGET := wasm32-wasip1
BIN_NAME := antdelirium
OBJCOPY ?= objcopy

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
	$(OBJCOPY) --remove-section .comment target/$(HOST_TARGET)/release/$(BIN_NAME)
	@echo "[GOVERNANCE] Artifact: target/$(HOST_TARGET)/release/$(BIN_NAME)"

# -----------------------------------------------
# make release-nondisclosure
# -----------------------------------------------
# Builds with reproducible logical source prefixes, removes non-runtime ELF
# metadata and validates symbols, hardening flags and physical locators.
# Administrative names may remain while rollback code is compiled; names do
# not grant authority and are removed only with the legacy implementation.
# -----------------------------------------------
.PHONY: release-nondisclosure
release-nondisclosure: binary
	bash packaging/release/validate-elf-nondisclosure.sh

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
# Runs the ordinary Cargo command used by governed Rust projects.
# -----------------------------------------------
.PHONY: check
check:
	cargo check --locked --target $(HOST_TARGET)

# -----------------------------------------------
# make ci
# -----------------------------------------------
# Reproduces the native quality gate used by automation. Rust warnings from
# this package are errors.
# -----------------------------------------------
.PHONY: ci
ci:
	cargo fmt --all -- --check
	bash -n packaging/cargo/validate-standard-commands.sh
	bash -n packaging/cargo/validate-installed-uninstall-rollback.sh
	bash -n packaging/rollout/validate-fail-open.sh
	python3 -B packaging/rollout/malformed-limited-peer.py --check
	bash -n packaging/ci/validate-bypass-matrix.sh
	bash -n packaging/ci/validate-authoritative-gate.sh
	bash -n packaging/ci/validate-projection-authority.sh
	bash -n packaging/ci/validate-protected-branch-gate.sh
	bash -n packaging/ci/render-protected-branch-authority-bundle.sh
	bash -n packaging/ci/validate-final-binary-parity.sh
	bash packaging/ci/validate-obsolete-build-hook-removal.sh
	python3 -c 'import ast,pathlib; ast.parse(pathlib.Path("packaging/ci/protected-branch-authority.py").read_text(encoding="utf-8"))'
	bash -n packaging/integrity/render-authority-bundle.sh
	bash -n packaging/integrity/validate-signed-checkpoint.sh
	bash -n packaging/systemd/checkpoint-installed-integrity.sh
	bash -n packaging/systemd/install-protected-branch-authority.sh
	python3 -c 'import ast,pathlib; ast.parse(pathlib.Path("packaging/integrity/checkpoint-authority.py").read_text(encoding="utf-8"))'
	bash -n packaging/release/validate-elf-nondisclosure.sh
	bash -n packaging/migration/validate-legacy-source-inventory.sh
	bash -n packaging/sandbox/validate-client-isolation.sh
	bash -n packaging/sandbox/validate-wrapper-syscalls.sh
	bash -n packaging/sandbox/validate-hostile-isolation-matrix.sh
	bash -n packaging/systemd/render-installation-bundle.sh
	bash -n packaging/systemd/validate-installed-service.sh
	bash -n packaging/systemd/validate-installed-generic-project.sh
	bash -n packaging/systemd/validate-installed-identity-matrix.sh
	bash -n packaging/systemd/upgrade-installed-service.sh
	bash -n packaging/systemd/diagnose-capability-upgrade.sh
	bash -n packaging/systemd/diagnose-installed-association-regression.sh
	bash -n packaging/systemd/manage-ci-attestation-capability.sh
	bash -n packaging/systemd/run-agent-submission.sh
	bash -n packaging/systemd/run-reviewer-operation.sh
	bash -n packaging/systemd/validate-installed-task-cycle.sh
	bash -n packaging/systemd/validate-installed-service-rollback.sh
	bash -n packaging/systemd/recover-installed-service-rollback.sh
	grep -Fqx 'limited_socket=/run/antdelirium/agent.sock' packaging/systemd/run-agent-submission.sh
	grep -Fqx 'limited_socket=/run/antdelirium/agent.sock' packaging/systemd/run-reviewer-operation.sh
	jq empty docs/contracts/v1/admin-protocol-v1.schema.json
	jq empty docs/contracts/v1/capability-grant-request-v1.schema.json
	jq empty docs/contracts/v1/capability-grant-receipt-v1.schema.json
	jq empty docs/contracts/v1/capability-revoke-request-v1.schema.json
	jq empty docs/contracts/v1/capability-revoke-receipt-v1.schema.json
	jq empty docs/contracts/v1/legacy-stage-receipt-v1.schema.json
	jq empty docs/contracts/v1/legacy-normalization-request-v1.schema.json
	jq empty docs/contracts/v1/legacy-normalization-receipt-v1.schema.json
	jq empty docs/contracts/v1/physical-reference-bindings-v1.schema.json
	jq empty docs/contracts/v1/legacy-dry-run-request-v1.schema.json
	jq empty docs/contracts/v1/legacy-dry-run-report-v1.schema.json
	jq empty docs/contracts/v1/legacy-conversion-request-v1.schema.json
	jq empty docs/contracts/v1/legacy-identity-bindings-v1.schema.json
	jq empty docs/contracts/v1/legacy-conversion-report-v1.schema.json
	jq empty docs/contracts/v1/migration-attestation-request-v1.schema.json
	jq empty docs/contracts/v1/migration-verifier-attestation-v1.schema.json
	jq empty docs/contracts/v1/migration-attestation-receipt-v1.schema.json
	jq empty docs/contracts/v1/migration-approval-request-v1.schema.json
	jq empty docs/contracts/v1/migration-approval-receipt-v1.schema.json
	jq empty docs/contracts/v1/migration-cutover-request-v1.schema.json
	jq empty docs/contracts/v1/migration-cutover-receipt-v1.schema.json
	jq empty docs/contracts/v1/migration-manifest-v1.schema.json
	jq empty docs/contracts/v1/system-installation-bundle-v1.schema.json
	jq empty docs/contracts/v1/initial-capability-provisioning-v1.schema.json
	jq empty docs/contracts/v1/governance-task-package-v1.schema.json
	jq empty docs/contracts/v1/fail-open-rollout-evidence-v1.schema.json
	jq empty docs/contracts/v1/bypass-matrix-evidence-v1.schema.json
	jq empty docs/contracts/v1/release-elf-audit-v1.schema.json
	jq empty docs/contracts/v1/hostile-isolation-evidence-v1.schema.json
	jq empty docs/contracts/v1/projection-authority-evidence-v1.schema.json
	jq empty docs/contracts/v1/signed-integrity-checkpoint-v1.schema.json
	jq empty docs/contracts/v1/integrity-authority-bundle-v1.schema.json
	jq empty docs/contracts/v1/protected-branch-policy-v1.json
	jq empty docs/contracts/v1/protected-branch-policy-v1.schema.json
	jq empty docs/contracts/v1/protected-branch-attestation-v1.schema.json
	jq empty docs/contracts/v1/protected-branch-authority-bundle-v1.schema.json
	RUSTFLAGS="-D warnings" cargo check --locked --all-targets --target $(HOST_TARGET)
	RUSTFLAGS="-D warnings" cargo test --locked --all-targets --target $(HOST_TARGET) --no-fail-fast
	$(MAKE) release-nondisclosure

# -----------------------------------------------
# make signed-integrity-checkpoint
# -----------------------------------------------
# Exercises the external Ed25519 authority against synthetic protected state.
# It signs journal/snapshot commitments and rejects gaps, prefix rewrites,
# rollback and signature mutation without requiring sudo or an external project.
# -----------------------------------------------
.PHONY: signed-integrity-checkpoint
signed-integrity-checkpoint:
	bash packaging/integrity/validate-signed-checkpoint.sh

# -----------------------------------------------
# make integrity-authority-bundle
# -----------------------------------------------
# Stages the path-free checkpoint authority for a separately reviewed root
# installation. The bundle never includes a signing key or activates the host.
# -----------------------------------------------
.PHONY: integrity-authority-bundle
integrity-authority-bundle:
	bash packaging/integrity/render-authority-bundle.sh

# -----------------------------------------------
# make legacy-example-validation
# -----------------------------------------------
# Optional evidence for a concrete legacy-project example. It is deliberately
# excluded from the AntDelirium product gate: no external example project may
# block the generic binary, service, Cargo integration or projection contract.
# -----------------------------------------------
.PHONY: legacy-example-validation
legacy-example-validation:
	bash -n packaging/migration/verify-legacy-source-inventory.sh
	bash -n packaging/systemd/prepare-installed-autobrowser-candidate.sh
	bash packaging/migration/verify-legacy-source-inventory.sh

# -----------------------------------------------
# make installed-generic-acceptance
# -----------------------------------------------
# Proves the real installed release using only a maintained synthetic Rust
# fixture. This gate never reads, migrates or modifies an external example.
# It requires the already-installed non-root wrapper and active systemd service.
# -----------------------------------------------
.PHONY: installed-generic-acceptance
installed-generic-acceptance:
	bash packaging/systemd/validate-installed-generic-project.sh

# -----------------------------------------------
# make installed-identity-matrix
# -----------------------------------------------
# Exercises installed project identity with two same-name projects, linked
# worktrees, a cache-complete check and parallel builds. Every fixture lives
# under target/ in this repository and no external project is inspected.
# -----------------------------------------------
.PHONY: installed-identity-matrix
installed-identity-matrix:
	bash packaging/systemd/validate-installed-identity-matrix.sh

# -----------------------------------------------
# make cargo-command-matrix
# -----------------------------------------------
# Installs the release in a disposable Cargo home under target/ and runs the
# five ordinary Cargo commands without wrapper flags or alternate syntax. It
# is explicit rather than part of portable CI because the installer correctly
# refuses root and the host must provide the clippy component.
# -----------------------------------------------
.PHONY: cargo-command-matrix
cargo-command-matrix: binary
	bash packaging/cargo/validate-standard-commands.sh

# -----------------------------------------------
# make authoritative-ci-gate
# -----------------------------------------------
# Uses command-line Cargo policy and a fresh target to ignore user-local and
# workspace-local wrapper choices, then records path-free mechanical evidence.
# Remote protection/signing belongs to the CI platform, not this local script.
# -----------------------------------------------
.PHONY: authoritative-ci-gate
authoritative-ci-gate: binary
	bash packaging/ci/validate-authoritative-gate.sh

# -----------------------------------------------
# make fail-open-rollout
# -----------------------------------------------
# Exercises only maintained synthetic fixtures against the audit-only wrapper
# and installed limited service. It records classifications and false-positive
# counts without enabling enforcement or touching an external project.
# -----------------------------------------------
.PHONY: fail-open-rollout
fail-open-rollout: binary
	bash packaging/rollout/validate-fail-open.sh

# -----------------------------------------------
# make bypass-matrix
# -----------------------------------------------
# Demonstrates five same-user bypasses, then requires a clean canonical
# revalidation through the trusted wrapper selected by CI/supervisor policy.
# No external project or protected state is used by this mechanical gate.
# -----------------------------------------------
.PHONY: bypass-matrix
bypass-matrix: binary
	bash packaging/ci/validate-bypass-matrix.sh

# -----------------------------------------------
# make adversarial-client-isolation
# -----------------------------------------------
# Exercises the T4.8 client boundary in disposable kernel namespaces. This is
# intentionally separate from the portable CI gate because it requires Linux,
# Bubblewrap and permission to create user/mount/PID namespaces.
# -----------------------------------------------
.PHONY: adversarial-client-isolation
adversarial-client-isolation:
	bash packaging/sandbox/validate-client-isolation.sh

# -----------------------------------------------
# make syscall-client-nondisclosure
# -----------------------------------------------
# Traces every file syscall made by Cargo, the wrapper, its audit-only child
# and rustc, then rejects any reference to known protected state roots. This
# requires ptrace permission and is intentionally separate from portable CI.
# -----------------------------------------------
.PHONY: syscall-client-nondisclosure
syscall-client-nondisclosure: binary
	bash packaging/sandbox/validate-wrapper-syscalls.sh

# -----------------------------------------------
# make hostile-isolation-matrix
# -----------------------------------------------
# Consolidates the disposable client boundary, full wrapper syscall trace,
# installed service /proc and ptrace denial, and inherited-FD exec proof. Run
# as the ordinary operator; this target must never be executed with sudo.
# -----------------------------------------------
.PHONY: hostile-isolation-matrix
hostile-isolation-matrix: binary
	bash packaging/sandbox/validate-hostile-isolation-matrix.sh

# -----------------------------------------------
# make projection-authority-matrix
# -----------------------------------------------
# Proves that project-local JSON files are disposable projections and that
# agent attempts cannot install or advance canonical tasks. Synthetic only;
# no installed service, sudo or external project is used.
# -----------------------------------------------
.PHONY: projection-authority-matrix
projection-authority-matrix: binary
	bash packaging/ci/validate-projection-authority.sh

# -----------------------------------------------
# make protected-branch-gate
# -----------------------------------------------
# Exercises the provider-neutral signed fail-closed decision. It proves the
# closed violation policy but cannot configure a hosted branch without a real
# remote and CI provider.
# -----------------------------------------------
.PHONY: protected-branch-gate
protected-branch-gate:
	bash packaging/ci/validate-protected-branch-gate.sh

# -----------------------------------------------
# make protected-branch-authority-bundle
# -----------------------------------------------
# Stages the evaluator and policy for root-owned installation. No key is
# included and no hosted provider is configured by this target.
# -----------------------------------------------
.PHONY: protected-branch-authority-bundle
protected-branch-authority-bundle:
	bash packaging/ci/render-protected-branch-authority-bundle.sh

# -----------------------------------------------
# make final-binary-parity
# -----------------------------------------------
# Proves that the single executable covers the seven approved legacy
# capabilities. Explicit auditing uses ordinary Cargo commands, not a second
# command that agents could bypass.
# -----------------------------------------------
.PHONY: final-binary-parity
final-binary-parity: binary
	bash packaging/ci/validate-final-binary-parity.sh

# -----------------------------------------------
# make system-installation-bundle
# -----------------------------------------------
# Renders a root-owned deployment tree under target/ and validates the
# systemd/sysusers syntax without creating accounts, installing files or
# activating units on the host.
# -----------------------------------------------
.PHONY: system-installation-bundle
system-installation-bundle: binary
	bash packaging/systemd/render-installation-bundle.sh

# -----------------------------------------------
# make clean
# -----------------------------------------------
.PHONY: clean
clean:
	cargo clean
