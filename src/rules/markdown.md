# [ac_ant_delirium] Archival Pairing & Markdown Governance

This module enforces the P3-Archival standard, ensuring that every source module is accompanied by a mandatory markdown companion file that provides high-level architectural context.

## Architectural Objectives

- **Mandatory Pairing**: Verifies that for every `.rs` file in the geode, a corresponding `.md` file exists in the same directory.
- **Pedagogical Substance**: Monitors the content length of markdown companions to ensure they provide meaningful documentation (min 256 characters).
- **Synchronization Monitoring**: Detects temporal drift between source code and documentation, mandating updates when the implementation outpaces the archival record by more than 48 hours.

## Archival Rules

- `rule_md_pairing`: Identifies missing or undersized documentation companions.
- `check_staleness`: Enforces the 48-hour synchronization window between code and narrative.
