# [ac_ant_delirium] Authorized Debug Sessions & Tracing

This module facilitates authorized debugging within Elite geodes, providing a governed alternative to raw `println!` macros.

## Architectural Objectives

- **Authorized Liberation**: Permits the use of standard output macros in strictly whitelisted files, enabling localized tracing during complex debugging sessions.
- **Elite Trace Integration**: Demonstrates use of the `ELITE_TRACE` system, which provides zero-cost, corporate-formatted logging for performance-critical audit engines.
- **Project Isolation**: While debug sessions are permitted, they remain subject to global project isolation rules to prevent trace leakage into production environments.

## Tracing Protocols

- `debug_internal_state`: Specialized entry point for localized state inspection.
- `ELITE_TRACE!`: Preferred mechanism for high-fidelity, industrial tracing.
