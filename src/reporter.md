# [ac_ant_delirium] Governance Reporting Infrastructure

This module provides high-fidelity, corporate-grade reporting utilities for governance audit results, ensuring that both human maintainers and AI agents receive actionable diagnostic feedback.

## Architectural Objectives

- **Strategic AI Direction**: Incorporates mandatory instructions for AI agents regarding the use of semantic manifests and automated Quick-Fixes.
- **Priority-Driven Feedback**: Categorizes violations into industrial priority levels (P0-P3) to guide remediation efforts effectively.
- **Standardized Serialization**: Bridges internal audit results with standardized terminal output and SARIF metadata.

## Reporting Utilities

- `print_elite_report`: Generates a high-level summary of the audit state, including priority breakdowns.
- `format_visual_report`: Produces a detailed, line-by-line list of violations for native terminal consumption.
