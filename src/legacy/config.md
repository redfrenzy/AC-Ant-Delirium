# [ac_ant_delirium] Configuration Infrastructure

This module implements a zero-dependency, brute-force TOML-style parser designed for the Aesthetic Core governance ecosystem.

## Architectural Objectives

- **Dependency Erasure**: Eradicates the need for heavy parsing crates like `serde` or `toml`, significantly improving build times and security auditability.
- **Linear Dispatch**: Uses a shallow-nested,早期リターン (early return) dispatch model to maintain cognitive clarity.
- **Dynamic Rule Mapping**: Supports granular activation and exception handling via `active_to` and `ignore_to` mechanisms.

## Configuration Model

- `AuditConfig`: The primary container for all governance settings.
- `RulesConfig`: Aggregates structural limits, token restrictions, and macro constraints.
- `TagConfig`: Defines pedagogical requirements for documentation artifacts.
- `ManifestConfig`: Controls the generation of AI-consumable semantic metadata.
