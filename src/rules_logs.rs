use crate::structs::AuditConfig;
use crate::visualizer::TaxonomyVisualizer;

/// [WHAT] Módulo de mensagens pedagógicas para diagnósticos de conformidade.
pub mod messages {
    pub const LOGICAL_DENSITY: &str = "Logical Density Violation: function exceeds authorized atomic complexity.";
    pub const PARAMS_TOO_MANY: &str = "Parameter Coupling Violation: excessive number of parameters in declaration.";
    pub const NESTING_TOO_DEEP: &str = "Nesting Violation: logic tree depth exceed sovereign limits.";
}

/// [WHAT] Roteador dinâmico de documentação industrial.
pub fn get_api_entry(path: &str, config: &AuditConfig) -> Option<String> {
    if path.is_empty() {
        return Some(get_welcome_portal(config));
    }

    TaxonomyVisualizer::generate_node_view(&config.hierarchy, path)
}

/// [WHAT] Portal Industrial Elite que serve como entrada para o motor de descoberta.
fn get_welcome_portal(config: &AuditConfig) -> String {
    let mut out = String::new();
    out.push_str("Welcome to the Ant Delirium Governance Portal\n");
    out.push_str("=============================================\n\n");
    out.push_str("Operational Hive Status: ACTIVE (Agnostic Mode)\n");
    out.push_str("Knowledge Access: GRANTED\n\n");
    out.push_str("You are connected to the discovery engine of Aesthetic Core.\n");
    out.push_str("Showing the established industrial taxonomy discovered in your TOML:\n\n");
    
    // [INJEÇÃO DA ÁRVORE VIA VISUALIZER]
    out.push_str(&TaxonomyVisualizer::generate_tree(&config.hierarchy));
    
    out.push_str("\n\n▸ To understand a specific node, try: '--api <path>'\n");
    out.push_str("[AESTHETIC CORE - INDUSTRIAL SWARM ORCHESTRATION]\n");
    out
}

pub fn get_node_status(_path: &str, _config: &AuditConfig) -> String {
    "[DISCOVERED]".to_string()
}

pub fn format_audit_message(_path: &str) -> String {
    format!(". Run --api for methodology.")
}
