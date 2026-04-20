use crate::structs::{AuditConfig, AuditResult, AuditViolation};
use crate::rules::get_full_axioms;

#[path = "Swarm_Engine_unitcore.rs"]
mod Swarm_Engine_unitcore;

pub struct SwarmEngine;

impl SwarmEngine {
    /// Dispara a patrulha massiva e devolve um resultado estruturado para o sistema de logs.
    pub fn audit(config: &AuditConfig) -> AuditResult {
        let axioms = get_full_axioms();
        let mut logs = vec![format!(
            "[SWARM] Iniciando patrulha massiva com {} axiomas.",
            axioms.len()
        )];
        let mut all_violations = Vec::new();

        for axiom in axioms {
            let violations = Swarm_Engine_unitcore::execute_axiom(axiom.as_ref(), config);
            if !violations.is_empty() {
                logs.push(format!(
                    "[SWARM][{}] {} violation(s) detectada(s).",
                    axiom.id(),
                    violations.len()
                ));
            }
            all_violations.extend(violations);
        }

        logs.push(format!(
            "[SWARM] Patrulha encerrada com {} violation(s).",
            all_violations.len()
        ));

        AuditResult {
            violations: all_violations,
            logs,
        }
    }

    /// Dispara a patrulha massiva preservando compatibilidade com o fluxo antigo.
    pub fn patrol(config: &AuditConfig) -> Vec<AuditViolation> {
        Self::audit(config).violations
    }

    /// [WHAT] Resolve o DNA detectado e o caminho taxonomico base para um arquivo auditado.
    pub fn resolve_file_taxonomy(config: &AuditConfig, file_path: &str) -> (String, String) {
        let tag_separator = Self::resolve_tag_separator(config);
        Swarm_Engine_unitcore::resolve_file_taxonomy(config, file_path, &tag_separator)
    }

    /// [WHAT] Resolve explicitamente o separador de tags utilizado pelo orquestrador.
    pub fn resolve_tag_separator(config: &AuditConfig) -> String {
        config.hierarchy.resolve_parameter("tag_separator")
            .unwrap_or_else(|| "_".to_string())
    }
}
