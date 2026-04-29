#[cfg(test)]
mod tests {
    use crate::parser::CodeScanner;
    use crate::rules::axiom_structural::Else_Free_Axiom_unit::ElseFreeAxiom;
    use crate::structs::{AuditConfig, AuditContext, GovernanceAxiom, HierarchyConfig};

    #[test]
    fn test_else_free_reports_plain_else_in_governed_function() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
fn branchy_unitcore(value: bool) -> usize {
    if value {
        return 1;
    } else {
        0
    }
}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/example/Else-Free-Error_unitcore.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unitcore".to_string(),
            taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
            violations: &mut violations,
        };

        ElseFreeAxiom::default().execute(&mut ctx);

        assert_eq!(ctx.violations.len(), 1);
        assert_eq!(
            ctx.violations[0].taxonomy_path,
            "hierarchy.files.unit.unitcore.else_free"
        );
        assert!(ctx.violations[0].message.contains("Else-Free violation"));
        assert!(ctx.violations[0]
            .snippet
            .as_ref()
            .is_some_and(|s| s.contains("else")));
    }

    #[test]
    fn test_else_free_accepts_bypass_token() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
fn justified_else(value: bool) -> usize {
    if value {
        return 1;
    } else { // [REASON] symmetry with the generated caller contract
        0
    }
}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/example/Justified_unitcore.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unitcore".to_string(),
            taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
            violations: &mut violations,
        };

        ElseFreeAxiom::default().execute(&mut ctx);

        assert!(ctx.violations.is_empty());
    }

    #[test]
    fn test_else_free_can_ignore_else_if_when_taxonomy_allows_it() {
        let mut hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        hierarchy
            .nodes
            .get_mut("hierarchy.files.unit.unitcore")
            .expect("unitcore branch should exist")
            .properties
            .insert("else_free_allow_else_if".to_string(), "true".to_string());
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
fn chained_branch(value: usize) -> usize {
    if value == 0 {
        return 0;
    } else if value == 1 {
        return 1;
    }
    2
}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/example/Else-If-Allowed_unitcore.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unitcore".to_string(),
            taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
            violations: &mut violations,
        };

        ElseFreeAxiom::default().execute(&mut ctx);

        assert!(ctx.violations.is_empty());
    }
}
