#[cfg(test)]
mod tests {
    use crate::parser::CodeScanner;
    use crate::rules::axiom_sanitization::Allowed_Crates_Axiom_unit::AllowedCratesAxiom;
    use crate::rules::axiom_sanitization::Dependency_Purification_Axiom_unit::DependencyPurificationAxiom;
    use crate::rules::axiom_sanitization::Restricted_Tokens_Axiom_unit::RestrictedTokensAxiom;
    use crate::rules::axiom_sanitization::Token_Sanitization_Axiom_unit::TokenSanitizationAxiom;
    use crate::structs::{AuditConfig, AuditContext, GovernanceAxiom, HierarchyConfig};

    #[test]
    fn allowed_crates_flags_unauthorized_external_dependency() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
[package]
name = "fixture"

[dependencies]
serde = "1"
ac_ant_delirium = "0.1"
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "Cargo.toml",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.dependency_governance.allowed_crates".to_string(),
            violations: &mut violations,
        };

        AllowedCratesAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("serde")));
        assert!(!ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("ac_ant_delirium")));
    }

    #[test]
    fn dependency_purification_flags_optional_dependency_forced_by_default_feature() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
[package]
name = "fixture"

[features]
default = ["dep:serde"]

[dependencies]
serde = { version = "1", optional = true }
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "Cargo.toml",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.dependency_governance.dependency_purification".to_string(),
            violations: &mut violations,
        };

        DependencyPurificationAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("serde")));
    }

    #[test]
    fn restricted_tokens_flags_forbidden_token_and_respects_bypass() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
pub fn sample() {
    let _ = value.unwrap();
    let _ = safe_value.unwrap(); // [TEMP_ALLOWED]
}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/exemples_error/Restricted-Tokens-Error_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        RestrictedTokensAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("unwrap")));
        assert_eq!(ctx.violations.len(), 1);
    }

    #[test]
    fn token_sanitization_flags_forbidden_token_usage() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
pub fn sample() {
    panic!("boom");
}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/exemples_error/Token-Sanitization-Error_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        TokenSanitizationAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("panic")));
    }
}
