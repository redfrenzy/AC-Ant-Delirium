# Phase 12: Dependency Governance Plan (ac_ant_delirium)

/// [WHAT] High-performance, brute-force dependency auditing for Elite geodes.
/// [WHY] To enforce architectural layering and purity without external crates.
/// [WHERE] ac_ant_delirium/src/rules/deps.rs

## Objective
Implement a 100% dependency-free version of the legacy `rule_deps` logic.

## Key Logic
1. **3-Tier Classification**: Whitelist check (ALLOWED), Path check (LOCAL), Feature check (OPTIONAL).
2. **Layering Hierarchy**: L0 (Auditor) -> L1 (Codecs) -> L2 (Image) -> L3 (Core) etc.
3. **Brute TOML Parser**: Line-based section and key-value mapping for `Cargo.toml`.

## File Map
- `src/rules/deps.rs`: The auditor code.
- `src/rules/deps.md`: Pedagogical documentation.
- `src/rules/mod.rs`: Integration hub.
