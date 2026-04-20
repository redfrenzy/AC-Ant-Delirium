# [ac_ant_delirium] Code Metrics & Complexity Auditing

This module implements the primary structural metrics for the Aesthetic Core ecosystem, ensuring that files and functions remain within strictly governed cognitive limits.

## Architectural Objectives

- **Logical Statement Density**: Moves beyond physical line counts to analyze the true structural weight of a function by tracking semicolons and nested scopes.
- **Normalized Coupling**: Verifies function signatures to prevent excessive parameter coupling, encouraging the use of state and configuration structures.
- **Physical Atomicity**: Enforces crate-wide clean line limits to ensure that the codebase remains modular and easily auditable by AI agents.

## Metrics Rules

- `rule_line_limit`: Monitors total clean lines per module.
- `rule_logical_density`: Analyzes the statement-to-line ratio within function bodies.
- `rule_params`: Constrains the number of parameters in function declarations.
