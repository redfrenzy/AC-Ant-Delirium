use crate::structs::{AuditConfig, AuditViolation};
use crate::rules::get_full_axioms;

#[path = "Swarm_Engine_unitcore.rs"]
mod Swarm_Engine_unitcore;

pub struct SwarmEngine;

impl SwarmEngine {
    /// Dispara a patrulha massiva utilizando o enxame de 24 axiomas contra a configuração fornecida.
    pub fn patrol(config: &AuditConfig) -> Vec<AuditViolation> {
        let mut all_violations = Vec::new();
        let axioms = get_full_axioms();
        
        println!("[SWARM] Iniciando patrulha massiva com {} axiomas...", axioms.len());
        
        for axiom in axioms {
            let violations = Swarm_Engine_unitcore::execute_axiom(axiom.as_ref(), config);
            all_violations.extend(violations);
        }
        
        all_violations
    }

    /// [WHAT] Resolve o DNA detectado e o caminho taxonomico base para um arquivo auditado.
    pub fn resolve_file_taxonomy(config: &AuditConfig, file_path: &str) -> (String, String) {
        Swarm_Engine_unitcore::resolve_file_taxonomy(config, file_path)
    }
}
