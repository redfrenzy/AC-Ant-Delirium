# Ant Delirium
### Aesthetic Core — Industrial Governance Engine

**Ant Delirium** is a Rust-based governance engine designed to provide controlled execution, verifiable authority, protected development workflows, and structured task management.

The project focuses on establishing explicit boundaries between authorized operations, restricted interfaces, administrative actions, and legacy migration workflows.

It combines a native executable, a portable WebAssembly build, structured configuration, protocol contracts, and supporting validation tooling.

**Repository:** [redfrenzy/AC-Ant-Delirium](https://github.com/redfrenzy/AC-Ant-Delirium)

---

## Overview

Ant Delirium is an infrastructure and governance project built around the idea that critical operations should be explicitly authorized, constrained, and verifiable.

Rather than relying exclusively on application-level conventions, the project includes dedicated components for service authority, authentication, task packages, protected Git workflows, migration procedures, and controlled execution.

Its architecture separates these responsibilities into focused Rust modules, with supporting configuration and validation scripts.

The project is organized around several engineering goals:

- **Explicit authority:** Separate operational capabilities from administrative authority.
- **Controlled execution:** Provide dedicated interfaces for agent and reviewer operations.
- **Protected development workflows:** Support controlled Git operations and protected execution gates.
- **Structured task management:** Define task packages and associated contracts.
- **Migration governance:** Organize legacy staging, normalization, conversion, approval, and cutover workflows.
- **Portable builds:** Support native Linux and WebAssembly compilation.
- **Validation and integrity:** Include automated checks for source formatting, protocol contracts, packaging, and operational scripts.

---

## Core Components

### 1. Execution and CLI Interfaces

The project includes dedicated modules for agent and reviewer command-line interfaces.

These components provide distinct entry points for operational and review workflows.

Relevant modules include:

- `Agent-Cli_unit.rs`
- `Reviewer-Cli_unit.rs`
- `Public-Output_unit.rs`
- `Binary-Mode_unit.rs`

The authoritative executable is launched through the binary execution entry point.

### 2. Service Governance

The service-related modules organize authority, authentication, storage, provisioning, communication, and hardening.

Relevant components include:

- `Service-Authority_unit.rs`
- `Service-Authentication_unit.rs`
- `Service-Storage_unit.rs`
- `Service-Provisioning_unit.rs`
- `Service-Channel_unit.rs`
- `Service-Hardening_unit.rs`

These components establish the structural foundation for controlled service operations.

### 3. Task Packages and Contracts

Ant Delirium includes components for working with task packages and their associated contracts.

Relevant modules include:

- `Task-Package-Contracts_unit.rs`
- `Task-Package-Generator_unit.rs`
- `Task-Package-Store_unit.rs`
- `Task-Package-Install_unit.rs`

The repository also contains versioned JSON schemas under `docs/contracts/v1/`.

These schemas define structured interfaces for supported governance operations and associated request, receipt, and evidence formats.

### 4. Protected Development Workflows

The project includes components and validation tooling for protected Git workflows and execution gates.

Relevant components include:

- `Protected-Git-Backup_unit.rs`
- `Protected-Gate_unit.rs`

The supporting tooling includes checks related to protected-branch authority, bypass matrices, final binary parity, and validation of governance gates.

### 5. Legacy Migration and Recovery

Migration-related components organize the handling of legacy states and controlled transitions.

The implementation includes modules for:

- Legacy staging
- Legacy normalization
- Dry-run analysis
- Legacy conversion
- Migration identity
- Identity recovery
- Attestation
- Approval
- Cutover

The repository also contains schemas and supporting tooling for migration requests, reports, identity bindings, approvals, and receipts.

These components provide a structured foundation for validating migration workflows before and during transitions.

---

## Technology Stack

| Technology | Purpose |
|---|---|
| Rust | Core implementation |
| Cargo | Dependency management and compilation |
| WebAssembly / WASI | Portable build target |
| SQLite | Embedded database support through `rusqlite` |
| Serde | Data serialization and deserialization |
| JSON | Structured data and protocol contracts |
| TOML | Configuration parsing and editing |
| SHA-256 | Cryptographic hashing through `sha2` |
| Git | Source control and protected workflow integration |
| Python and Bash | Supporting validation and packaging scripts |

### Rust Requirements

The project declares:

- **Rust edition:** 2021
- **Minimum Rust version:** 1.88

The dependency versions are defined in `Cargo.toml` and `Cargo.lock`.

---

## Repository Structure

```text
AC-Ant-Delirium/
├── .cargo/
│   └── config.toml
├── ant_delirium_config/
├── docs/
│   └── contracts/
│       └── v1/
├── packaging/
├── scratch/
├── src/
│   ├── main.rs
│   ├── Agent-Cli_unit.rs
│   ├── Reviewer-Cli_unit.rs
│   ├── Service-Authority_unit.rs
│   ├── Service-Authentication_unit.rs
│   ├── Protected-Gate_unit.rs
│   ├── Task-Package-Contracts_unit.rs
│   └── ...
├── tests/
├── Cargo.toml
├── Cargo.lock
├── Makefile
├── build.rs
├── compile.sh
├── api_ant_delirium.toml
└── delirium.toml
```

This is a representative overview of the repository. Individual modules and supporting files may evolve as the project develops.

---

## Getting Started

### Prerequisites

Install the following tools:

- Rust toolchain, including `rustc` and `cargo`
- GNU Make
- Git
- Python 3
- Bash

Additional tools may be required by specific validation targets, packaging scripts, or platform-specific build procedures.

For example, the repository's CI checks use `jq` to validate JSON schemas.

### 1. Clone the Repository

```bash
git clone https://github.com/redfrenzy/AC-Ant-Delirium.git
cd AC-Ant-Delirium
```

### 2. Check the Rust Toolchain

```bash
rustc --version
cargo --version
```

Ensure that the installed Rust compiler satisfies the minimum version specified in `Cargo.toml`.

### 3. Check the Project

Run the native Cargo check target:

```bash
make check
```

This invokes Cargo with the locked dependency versions and the native Linux target.

### 4. Build the Native Executable

Build the release executable:

```bash
make binary
```

The build uses the locked dependencies and enables strict compiler warnings.

The resulting executable is located at:

```text
target/x86_64-unknown-linux-gnu/release/antdelirium
```

The build also removes the ELF `.comment` section from the generated executable.

### 5. Build the WebAssembly Artifact

To build the WebAssembly version:

```bash
make wasm
```

The output artifact is:

```text
target/wasm32-wasip1/release/antdelirium.wasm
```

This target uses the `wasm32-wasip1` compilation target.

A successful build does not, by itself, establish compatibility with every WebAssembly runtime. Runtime requirements and supported execution environments should be verified separately.

---

## Build Commands

The repository provides a Makefile for common development and build operations.

| Command | Description |
|---|---|
| `make check` | Check the Rust project for compilation errors |
| `make binary` | Build the native release executable |
| `make wasm` | Build the WebAssembly release artifact |
| `make release-nondisclosure` | Build the native executable and run ELF disclosure validation |
| `make ci` | Run the repository's configured formatting and validation checks |

### Native Build

The native build explicitly targets:

```text
x86_64-unknown-linux-gnu
```

This is important because `.cargo/config.toml` defines a default WebAssembly target. The Makefile explicitly selects the native target for native compilation.

### WebAssembly Build

The WebAssembly build targets:

```text
wasm32-wasip1
```

The generated artifact is intended for a compatible WASI environment.

---

## Validation and Quality Checks

Ant Delirium includes a dedicated validation target:

```bash
make ci
```

The Makefile configures checks covering several areas.

### Rust Formatting

```bash
cargo fmt --all -- --check
```

Checks Rust formatting without automatically modifying source files.

### Script Validation

The validation workflow includes syntax checks for Bash scripts and parsing checks for Python files.

### Contract Validation

The workflow uses `jq` to validate versioned JSON schemas under:

```text
docs/contracts/v1/
```

### Governance and Packaging Checks

The configured checks also cover areas such as:

- Protected execution and branch-authority validation
- Bypass-matrix validation
- Binary parity checks
- Migration and legacy-source validation
- Installation and rollback validation
- Service and identity validation
- Sandbox isolation checks
- Release artifact validation

The exact set of checks is defined by the repository's current Makefile.

**Note:** The presence of a validation target does not imply that every check has passed in every environment. Run the commands locally to verify the current checkout.

---

## Configuration

The repository includes configuration files for Cargo and Ant Delirium.

Relevant files include:

```text
.cargo/config.toml
api_ant_delirium.toml
delirium.toml
```

The Cargo configuration sets a default WebAssembly compilation target and defines a WebAssembly build alias.

The Makefile explicitly selects the native target when performing native builds.

Before running the application or configuring an installation, consult the relevant configuration definitions and documentation in the repository.

Do not assume that development configuration is suitable for a production deployment without reviewing the applicable requirements.

---

## Design Principles

### Explicit Authority Boundaries

Operational interfaces and administrative workflows should have clearly defined responsibilities.

The architecture reflects this separation through dedicated service, authentication, administrative protocol, and execution modules.

### Structured Contracts

Versioned JSON schemas provide machine-readable definitions for supported protocol messages and governance artifacts.

Structured contracts help make interfaces explicit and provide a foundation for automated validation.

### Controlled State Transitions

Migration-related modules separate the stages involved in handling legacy data and transitioning between states.

This structure supports validation and review of migration operations.

### Defense in Depth

The project includes separate components and validation tooling for execution gates, service hardening, identity management, protected workflows, and sandbox isolation.

These mechanisms should be evaluated together with the actual runtime configuration and deployment environment.

### Reproducible Build Inputs

The repository includes `Cargo.lock`, and the native build uses Cargo's locked dependency mode.

The build configuration also applies source-path remapping and strict compiler warnings.

These measures support build consistency and reduce the amount of incidental build information embedded in the release process. They do not independently guarantee bit-for-bit reproducibility across all environments.

---

## Development

When working on the project:

1. Review the relevant module before changing its behavior.
2. Check the associated protocol contracts and configuration.
3. Run the narrowest applicable validation checks.
4. Run `make check` after Rust changes.
5. Run `make ci` when validating the broader repository.
6. Review changes to migration, authority, and protected-workflow components carefully.

Keep documentation, schemas, implementation, and validation tooling consistent when changing a public contract.

---

## Current Scope

Ant Delirium contains components for governance, controlled execution, task-package handling, service operations, protected development workflows, and legacy migration.

The repository also provides native and WebAssembly build paths.

The exact capabilities available at runtime depend on the implemented modules, configuration, and deployment environment. A component's presence in the source tree should not be interpreted as proof that every associated workflow is fully deployed or production-ready.

---

## Contributing

Contributions that improve correctness, maintainability, test coverage, documentation, and validation are welcome.

Before proposing a change:

1. Review the relevant implementation and existing contracts.
2. Keep changes focused on the intended behavior.
3. Add or update tests when appropriate.
4. Run the relevant validation commands.
5. Document any changes to configuration or supported interfaces.

For substantial architectural changes, explain the problem being addressed and the expected impact on existing workflows.

---

## License

Review the repository's license and distribution terms before redistributing or incorporating Ant Delirium into another project.

If no applicable license is present, do not assume that the code is available for unrestricted reuse.

---

## Project

**Ant Delirium — Aesthetic Core Project**

Repository: [https://github.com/redfrenzy/AC-Ant-Delirium](https://github.com/redfrenzy/AC-Ant-Delirium)
