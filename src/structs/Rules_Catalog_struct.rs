use super::Global_Params_struct::{AuditConfig, AuditViolation};
use crate::parser::ScannedBlock;

/// [AESTHETIC CORE - GOVERNANCE CONTRACT]
/// Soberania absoluta para execução de regras.
pub trait AuditRule: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn execute(&self, ctx: &mut AuditContext);
}

/// [AESTHETIC CORE - AUDIT CONTEXT]
/// Solo sagrado que contém todo o estado necessário para uma auditoria industrial.
pub struct AuditContext<'a> {
    pub file_path: &'a str,
    pub source: &'a str,
    pub functions: &'a [ScannedBlock<'a>],
    pub config: &'a AuditConfig,
    pub detected_dna: String,
    pub taxonomy_path: String,
    pub violations: &'a mut Vec<AuditViolation>,
}

#[derive(Default)]
pub struct ComplexityRule;

#[derive(Default)]
pub struct MaxParamsRule;
