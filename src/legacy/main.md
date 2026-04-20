# [ac_ant_delirium] CLI Command Interface

This module provides the primary user interface for the governance engine, facilitating the transition from a library to a standalone industrial tool.

## Technical Objectives

- **Dual-Mode Execution**: Supports both file-level and directory-level auditing with automatic path resolution.
- **Fail-Safe Defaults**: Implements a transparent configuration discovery mechanism that falls back to default settings if `delirium.toml` is missing.
- **Industrial Reporting**: Bridges the audit engine with the `Elite` visual reporter to provide high-fidelity terminal feedback.

## Commands

- `main`: Orchestrates argument parsing and configuration loading.
- `audit_file`: Executes the governance pipeline on a single Rust source file.
- `audit_directory`: Recursively discovers and audits all `.rs` files within a specified target directory.
