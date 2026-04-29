#![allow(non_snake_case)]

#[allow(non_snake_case)]
pub mod Logs;
pub mod api;
pub mod engine;
#[path = "Governance_unit.rs"]
pub mod governance;
#[path = "Parser_unit.rs"]
pub mod parser;
pub mod restricted_tokens;
pub mod rules;
#[path = "Rules-Logs_unit.rs"]
pub mod rules_logs;
pub mod structs;
pub mod tests;
#[path = "Visualizer_unit.rs"]
pub mod visualizer;

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
