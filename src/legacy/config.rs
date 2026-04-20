use crate::structs::{HierarchyConfig, AuditConfig};

impl AuditConfig {
    /// [WHAT] Ponto de entrada universal que delega o parse ao motor agnóstico via Scanner (Unit).
    pub fn parse_brute(input: &str) -> Self {
        let mut config = AuditConfig::default();
        config.hierarchy = HierarchyConfig::parse_brute(input);
        config
    }
}
