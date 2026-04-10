# [ac_ant_delirium] SARIF 2.1.0 Serialization Engine

This module provides the industrial serialization logic for transforming raw `AuditViolation` records into the standardized SARIF 2.1.0 JSON format.

## Architectural Objectives

- **Standardization**: Ensures full compatibility with the SARIF 2.1.0 specification for seamless integration with modern static analysis dashboards.
- **Atomicity**: Fragments the complex JSON construction process into small, auditable sub-passes that maintain low logical density.
- **Zero-Dependency Sanitization**: Implements an internal character escaping engine to ensure that generated reports are always syntactically valid JSON.

## Serialization Components

- `push_sarif_header`: Injects the SARIF schema and tool metadata.
- `push_sarif_results`: Iterates and encodes individual violation records.
- `push_sarif_footer`: Finalizes the JSON bracket structure.
