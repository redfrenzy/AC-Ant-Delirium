# [ac_ant_delirium] Macro Isolation & Complexity Rules

This module enforces structural constraints on Rust macros, ensuring they remain transparent and do not become "Cognitive Abysses" of un-auditable logic.

## Architectural Objectives

- **Logic Erasure**: Restricts the use of complex flow control (if, match, loops) within macro definitions to prevent hidden business logic.
- **Physical Atomicity**: Enforces line limits on macro bodies to keep them concise and purpose-driven.
- **Auditable Substitutions**: Ensures that macros are used only for structural boilerplate and decorative patterns, not as a replacement for pure functions.

## Macro Rules

- `rule_macro_isolation`: Monitors macro bodies for complex tokens and excessive line counts.
- `allow_complex_logic`: Configuration gate that toggles the strictness of macro-layer governance.
