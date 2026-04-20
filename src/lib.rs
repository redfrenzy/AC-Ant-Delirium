#![allow(non_snake_case)]

pub mod engine;
pub mod parser;
pub mod rules;
pub mod structs;
pub mod visualizer;
#[allow(non_snake_case)]
pub mod Logs;
pub mod rules_logs;
pub mod restricted_tokens;
pub mod governance;
pub mod tests;

pub use crate::structs::{AuditConfig, AuditViolation};

/// [AESTHETIC CORE - AUDIT GATEKEEPER]
/// Portal soberano que orquestra a auditoria de conformidade de um Geode.
impl AuditConfig {
    pub fn audit_geode(
        &self, 
        file_path: &str, 
        source: &str, 
        violations: &mut Vec<AuditViolation>,
        detected_dna: String,
        taxonomy_path: String,
    ) {
        // [SCAN] Extração industrial de blocos
        let functions = parser::CodeScanner::scan_blocks(source);
        
        // [ORCHESTRATE] Invoca o motor de regras de forma independente do motor de hierarquia
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
