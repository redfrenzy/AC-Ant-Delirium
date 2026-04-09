# Macro Governance and Isolation

/// [WHAT] Enforcing simplicity and safety in macro definitions.
/// [WHY] To prevent the creation of opaque, un-auditable logic trails.
/// [WHERE] Structural audit phase of the rule engine.

This module limits the length and complexity of `macro_rules!` blocks.
It ensures that macros remain tools for boilerplate reduction, not for hiding logic.
