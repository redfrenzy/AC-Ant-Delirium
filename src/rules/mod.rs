//@ [WHAT] Orchestration hub for all governance rules.
//@ [WHY] To centralize and modularize the audit engine logic.
//@ [WHERE] ac_ant_delirium/src/rules/mod.rs

pub mod metrics;
pub mod docs;
pub mod structure;
pub mod macros;
pub mod tokens;
pub mod manifest;
pub mod deps;
pub mod markdown;
pub mod quality;
