# [ac_ant_delirium] Core Engine Architecture

This module constitutes the primary orchestrator for the governance engine, providing the high-level coordination between the parser, the rule set, and the reporting system.

## Architectural Objectives

- **Reflexive Governance**: The engine is capable of auditing its own source code to ensure absolute adherence to Elite industrial standards.
- **Zero-Dependency Interoperability**: Implements a clean WASM/FFI boundary for integration into any CI/CD pipeline without external runtime requirements.
- **Pedagogical Transparency**: Every architectural decision is documented using mandatory [WHAT], [WHY], and [WHERE] tags to maintain long-term archival health.

## Main Components

- `AuditResult`: Standardized container for all governance violations and execution logs.
- `audit_code_to_result`: The central native entry point for the auditing pipeline.
- `to_sarif`: Standardized serialization layer for VS Code and SARIF integration.
