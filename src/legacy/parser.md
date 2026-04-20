# [ac_ant_delirium] Structural Scanner Infrastructure

This module implements a Zero-Copy structural scanner for Rust source code, designed to identify function boundaries and associated pedagogical documentation without external dependencies.

## Architectural Objectives

- **Zero-Copy Performance**: Minimizes memory pressure by storing references to the original source string rather than duplicating function bodies in the heap.
- **Structural Immunity**: Employs string-literal scrubbing via `clean_line` to ensure that keywords like `fn` inside strings do not trigger false positives.
- **Recursive Brace Tracking**: Implements a robust vertical scope analyzer to accurately calculate function line ranges even in deeply nested logic.

## Core Utilities

- `CodeScanner`: The primary entry point for function discovery.
- `find_closing_brace`: A specialist scope analyzer that respects nesting.
- `clean_line`: A surgical sanitizer that removes strings and comments for structural analysis.
