# [ac_ant_delirium] 3-Tier Dependency Classification & Layering

This module implements the architectural "Shield" for the Aesthetic Core ecosystem, enforcing strict dependency purity and hierarchical layering at the build level.

## Tiered Governance Engine

- **TIER 1: LOCAL (Layering)**: Enforces that low-level infrastructure (L0) remains platform-agnostic and does not depend on higher-level feature modules (L1-L6).
- **TIER 2: ALLOWED (Whitelist)**: Validates third-party crates against a strictly governed whitelist of authorized dependencies.
- **TIER 3: OPTIONAL (Sanitization)**: Detects and restricts hidden dependency activations in Cargo features, ensuring that only "Sanitized" (opt-in) features are permitted.
- **TIER 4: IMPURE (Violation)**: Blocks any dependency that fails Tiers 1-3, ensuring the geode remains a high-integrity asset.

## Architectural Objectives

- **Circular Dependency Elimination**: Enforces a directed acyclic graph (DAG) across the workspace through layer-based validation.
- **WASM Footprint Optimization**: Monitors feature activations to prevent "Crate Creep" and maintain minimal binary sizes for decentralized application delivery.
- **Zero-Dependency Auditing**: Provides a pure-Rust, standalone parser for `Cargo.toml` to avoid bootstrap dependencies.
