# [ac_ant_delirium] Poison Token & Safety Governance

This module implements the "Poison Token" detection engine, designed to identify and flag prohibited or high-risk code patterns that could compromise the safety and integrity of the geode.

## Architectural Objectives

- **Unhandled Panic Prevention**: Detects and restricts the use of crash-prone tokens like `unwrap()` and `panic!`, enforcing explicit result handling.
- **Log Pollution Control**: Monitors and restricts raw printing and logging tokens, ensuring that all communications are routed through authorized reporters.
- **Surgical Quarantine**: Implements a "Protocol Safe Harbor" that allows for isolated, authorized usage of restricted logic in dedicated quarantine modules.

## Safety Rules

- `rule_poison_tokens`: Scans source code for banned patterns defined in the governance policy.
- `rule_no_raw_print`: Specifically targets unauthorized output tokens to maintain geode purity.
- `Safety Required`: Supports inline bypass comments for explicitly reasoned and risk-assessed logic.
