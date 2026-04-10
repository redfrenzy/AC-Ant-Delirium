# [ac_ant_delirium] Diagnostic & Tracing Infrastructure

This module provides a centralized repository for governance messaging and cross-platform tracing utilities, ensuring consistent diagnostic feedback across both native and WASM execution environments.

## Architectural Objectives

- **WASM Interoperability**: Implements custom logging macros that bypass the limitations of standard `println!` in restricted environments.
- **UI/Logic Separation**: Isolates all human-readable violation messages from the core audit logic to facilitate internationalization and pedagogical consistency.
- **Governed Tracing**: Employs a zero-cost, whitelist-driven tracing system (`ELITE_TRACE`) that allows for surgical debugging in development without polluting production binaries.

## Infrastructure Components

- `messages`: A static collection of standardized governance violation strings.
- `delirium_log!`: A safe, buffer-backed logging macro for WASM compatibility.
- `ELITE_TRACE!`: A governed, conditional debug tracer for Elite geodes.
