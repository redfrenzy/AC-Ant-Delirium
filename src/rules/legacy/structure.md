# [ac_ant_delirium] Structural Governance Rules

This module enforces the Elite industrial standard for code flow and architectural depth, ensuring that the geode remains maintainable through the elimination of complexity anti-patterns.

## Architectural Objectives

- **Else-Free Logic**: Mandates the use of early returns, guards, and match blocks to ensure a single, linear, and easily auditable execution path.
- **Cognitive Depth Management**: Implements the "Abismo Cognitivo" rule, which constrains logical nesting based on indentation depth to prevent spaghetti code.
- **Interface Decoupling**: Verifies function signatures at the structural level to ensure parameter counts remain within strictly governed limits.

## Structural Rules

- `rule_else_free`: Identifies and flags `else` blocks that lack a reasoned bypass justification.
- `rule_nesting`: Calculates the logical depth of code blocks based on industrial indentation standards.
- `rule_params`: Enforces parameter count limits on all function signatures.
