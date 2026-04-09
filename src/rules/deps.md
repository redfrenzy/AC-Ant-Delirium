# Dependency Governance (rule_deps)

/// [WHAT] Advanced auditor for `Cargo.toml` dependency manifests.
/// [WHY] To ensure that geodes remain isolated, pure, and strictly layered.
/// [WHERE] ac_ant_delirium/src/rules/deps.rs

## 3-Tier Classification System

This rule classifies every dependency into one of four tiers to determine its impact on project purity:

1.  **TIER 1 (LOCAL)**: Internal geode references via relative paths. Triggers a **Layering Check**.
2.  **TIER 2 (ALLOWED)**: Whitelisted external crates (standard libraries like `std`, `core`, `alloc`).
3.  **TIER 3 (OPTIONAL)**: Crates marked `optional = true`. These are permitted ONLY if not activated by the `default` feature chain.
4.  **TIER 4 (IMPURE)**: Any external, non-whitelisted, non-optional crate. Triggers a **P0-CRITICAL** violation.

## Layering Hierarchy

The geode enforces a strict architectural hierarchy. A geode at a lower layer (e.g., L0 - Auditor) is forbidden from depending on a higher layer (e.g., L2 - Image Processing) to prevent cyclic complexity and maintain trust boundaries.

- **Layer 0**: Governance & Core Auditors (`ac_ant_delirium`).
- **Layer 2**: Low-level formats and processing (`ac_image`).
- **Layer 3**: High-level application logic (`ac_core`).

## Implementation Logic

Since the geode is dependency-free, the rule uses a **Brute-Force TOML Parser**. It scans the file line-by-line, tracks the current section, and resolves feature chains manually using string manipulation. This ensures that the auditing logic itself is not compromised by the vulnerabilities it seeks to detect.
