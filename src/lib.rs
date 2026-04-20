#![allow(non_snake_case)]

pub mod engine;
#[path = "legacy/parser.rs"]
pub mod parser;
pub mod rules;
pub mod structs;
#[path = "legacy/visualizer.rs"]
pub mod visualizer;
#[allow(non_snake_case)]
pub mod Logs;
#[path = "legacy/rules_logs.rs"]
pub mod rules_logs;
pub mod restricted_tokens;
#[path = "legacy/governance.rs"]
pub mod governance;
pub mod tests;

pub use crate::structs::{AuditConfig, AuditViolation};

impl AuditConfig {
    pub fn audit_geode(
        &self,
        file_path: &str,
        source: &str,
        violations: &mut Vec<AuditViolation>,
        detected_dna: String,
        taxonomy_path: String,
    ) {
        let functions = parser::CodeScanner::scan_blocks(source);

        rules::run_diagnostics(
            self,
            file_path,
            source,
            &functions,
            violations,
            detected_dna,
            taxonomy_path,
        );
    }
}
