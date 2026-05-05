#[cfg(test)]
mod tests {
    use crate::parser::CodeScanner;
    use crate::rules::axiom_structural::Type_Logic_Axiom_unit::TypeLogicAxiom;
    use crate::structs::{AuditConfig, AuditContext, GovernanceAxiom, HierarchyConfig};

    #[test]
    fn test_type_logic_accepts_public_api_in_unit_files() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
pub fn exposed_api() {}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/example/Allowed_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        TypeLogicAxiom::default().execute(&mut ctx);

        assert!(ctx.violations.is_empty());
    }

    #[test]
    fn test_type_logic_rejects_public_api_in_unitcore_files() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
pub fn leaked_api() {}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/example/Leaked_unitcore.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unitcore".to_string(),
            taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
            violations: &mut violations,
        };

        TypeLogicAxiom::default().execute(&mut ctx);

        assert_eq!(ctx.violations.len(), 1);
        assert_eq!(
            ctx.violations[0].taxonomy_path,
            "hierarchy.files.unit.unitcore.type_logic"
        );
        assert!(ctx.violations[0].message.contains("unitcore"));
        assert!(ctx.violations[0].message.contains("public_fn"));
    }

    #[test]
    fn test_type_logic_accepts_impl_blocks_in_structcore_files() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
impl WorkerState {
    fn helper() {}
}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/struct/Worker_structcore.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "structcore".to_string(),
            taxonomy_path: "hierarchy.files.struct.structcore".to_string(),
            violations: &mut violations,
        };

        TypeLogicAxiom::default().execute(&mut ctx);

        assert!(ctx.violations.is_empty());
    }

    #[test]
    fn test_type_logic_skips_files_without_detected_dna() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
fn neutral_file() {}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/neutral.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.files".to_string(),
            violations: &mut violations,
        };

        TypeLogicAxiom::default().execute(&mut ctx);

        assert!(ctx.violations.is_empty());
    }
}
