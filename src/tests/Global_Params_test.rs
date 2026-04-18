#[cfg(test)]
mod tests {
    use crate::parser::CodeScanner;
    use crate::structs::{AuditConfig, AuditContext};

    #[test]
    fn test_axiom_parameter_resolution_contract() {
        let hierarchy = crate::structs::HierarchyConfig::load_from_folder("ant_delirium_config");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = "pub fn sample() {}\n";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let ctx = AuditContext {
            file_path: "src/rules/axiom_structural/Logical_Density_Axiom_unit.rs",
            source,
            functions: &functions,
            config: &config,
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        let identity = ctx.resolve_axiom_identity_path("logical_density")
            .expect("logical_density deveria ter identidade unica");
        assert_eq!(identity, "hierarchy.structural.logical_density");

        let requirements = ctx.resolve_axiom_requirements("logical_density")
            .expect("logical_density deveria declarar requirements");
        assert_eq!(requirements, vec!["logical_density".to_string()]);

        let resolution = ctx.require_axiom_parameter("logical_density", "logical_density")
            .expect("logical_density deveria resolver seu valor concreto");
        assert_eq!(resolution.identity_path, "hierarchy.structural.logical_density");
        assert_eq!(resolution.requirement_id, "logical_density");
        assert_eq!(resolution.value, "40");
    }

    #[test]
    fn test_axiom_parameter_resolution_errors_when_requirement_is_missing() {
        let hierarchy = crate::structs::HierarchyConfig::load_from_folder("ant_delirium_config");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = "pub fn sample() {}\n";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let ctx = AuditContext {
            file_path: "src/rules/axiom_structural/Logical_Density_Axiom_unit.rs",
            source,
            functions: &functions,
            config: &config,
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        let error = ctx.require_axiom_parameter("logical_density", "max_params")
            .expect_err("logical_density nao deveria declarar max_params");
        assert!(error.contains("nao declarou o requirement 'max_params'"));
    }
}
