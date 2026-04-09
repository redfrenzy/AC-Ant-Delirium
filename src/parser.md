# Code Scanner (Brute Parser)

/// [WHAT] Simplified code analysis engine for Elite geodes.
/// [WHY] To isolate functions and metadata without external dependencies.
/// [WHERE] ac_ant_delirium/src/parser.rs

## Scanning Methodology

The `CodeScanner` uses a **Brute-Force Vertical Scan** approach:

1.  **Docstring Accumulation**: It collects consecutive lines starting with `///`.
2.  **Boundary Detection**: It identifies the start of a function block using keyword matching (`fn`, `{`).
3.  **Depth Resolution**: It performs a nested bracket-count scan to find the exact closing `}` of the function block.
4.  **Metadata Packaging**: It combines the accumulated docstring with the discovered line range and function name.

## Constraints

The scanner is designed to be **O(N)** and memory-efficient. It does not perform full syntactic analysis, but provides enough structural context for governance rules (like documentation tag validation and nesting depth checks) to operate reliably.
