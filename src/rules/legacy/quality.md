# [ac_ant_delirium] Pedagogical Quality & Word Salad Auditing

This module implements the "Circular Definition" detector, which ensures that documentation provides meaningful architectural context rather than simply repeating the function name.

## Architectural Objectives

- **Reflexive Precision**: Prevents the use of "Word Salads"—circular descriptions that add no pedagogical value to the geode.
- **English Mandate**: Enforces the use of English for all documented justifications to ensure global auditability by AI agents.
- **Semantic Density**: Validates that descriptions contain a sufficient diversity of tokens to represent a reasoned architectural intent.

## Quality Rules

- `rule_word_salad`: Analyzes docstrings for circularity against the function identifier tokens.
- `extract_tag_content`: Surgical scraper for pedagogical tag evaluation.
- `is_circular_definition`: Algorithmic threshold for identifying superficial documentation.
