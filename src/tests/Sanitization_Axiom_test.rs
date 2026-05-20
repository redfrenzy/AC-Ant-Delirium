#[cfg(test)]
mod tests {
    use crate::parser::CodeScanner;
    use crate::rules::axiom_sanitization::Allowed_Crates_Axiom_unit::AllowedCratesAxiom;
    use crate::rules::axiom_sanitization::Dependency_Purification_Axiom_unit::DependencyPurificationAxiom;
    use crate::rules::axiom_sanitization::Restricted_Tokens_Axiom_unit::RestrictedTokensAxiom;
    use crate::rules::axiom_sanitization::Strict_Wrapper_Boundary_Axiom_unit::StrictWrapperBoundaryAxiom;
    use crate::rules::axiom_sanitization::Token_Sanitization_Axiom_unit::TokenSanitizationAxiom;
    use crate::structs::{
        AuditConfig, AuditContext, GovernanceAxiom, HierarchyConfig, ProjectContext,
        ProjectFileContext,
    };

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
    fn allowed_crates_blocks_all_external_dependencies_when_libres_is_empty() {
        let mut hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        if let Some(permissions) = hierarchy.nodes.get_mut("hierarchy.permissions") {
            permissions
                .properties
                .insert("libres".to_string(), "[]".to_string());
        }
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
[package]
name = "fixture"

[dependencies]
serde = "1"
rayon = "1"
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
        assert!(ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("rayon")));
    }

    #[test]
    fn allowed_crates_flags_external_usage_outside_src_libres_wrapper_boundary() {
        let hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
use rayon::prelude::*;

pub fn sample() {}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/feature/Feature_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        AllowedCratesAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("rayon")));
        assert!(ctx.violations.iter().any(|violation| {
            violation.message.contains("src/libres")
                && violation.message.contains("rayon")
        }));
    }

    #[test]
    fn allowed_crates_flags_path_usage_outside_src_libres_wrapper_boundary() {
        let mut hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        if let Some(permissions) = hierarchy.nodes.get_mut("hierarchy.permissions") {
            permissions
                .properties
                .insert("libres".to_string(), "[\"wgpu\"]".to_string());
        }
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
pub fn sample() {
    let _ = wgpu::Features::empty();
}
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/feature/Feature_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        AllowedCratesAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("wgpu")));
    }

    #[test]
    fn allowed_crates_flags_missing_wrapper_file_for_allowed_dependency() {
        let mut hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        if let Some(permissions) = hierarchy.nodes.get_mut("hierarchy.permissions") {
            permissions
                .properties
                .insert("libres".to_string(), "[\"serde\"]".to_string());
        }
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
[package]
name = "fixture"

[dependencies]
serde = "1"
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut project = ProjectContext::default();
        project.insert_loaded_file(ProjectFileContext::new(
            "src/lib.rs".to_string(),
            String::new(),
            String::new(),
            String::new(),
        ));
        let mut ctx = AuditContext {
            file_path: "facets/red_core/Cargo.toml",
            source,
            functions: &functions,
            config: &config,
            project: &project,
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.dependency_governance.allowed_crates".to_string(),
            violations: &mut violations,
        };

        AllowedCratesAxiom::default().execute(&mut ctx);

        assert!(ctx.violations.iter().any(|violation| {
            violation.rule_id.contains("serde_wrapper")
                && violation
                    .snippet
                    .as_deref()
                    .unwrap_or_default()
                    .contains("src/libres/serde_unit.rs")
        }));
    }

    #[test]
    fn strict_wrapper_boundary_flags_wildcard_reexport_inside_libres() {
        let mut hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        if let Some(permissions) = hierarchy.nodes.get_mut("hierarchy.permissions") {
            permissions
                .properties
                .insert("wrapper_boundary_contract".to_string(), "explicit_ac_alias".to_string());
        }
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
pub use wgpu::*;
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "facets/red_core/src/libres/Wgpu_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.permissions.wrapper_boundary_contract".to_string(),
            violations: &mut violations,
        };

        StrictWrapperBoundaryAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("wgpu_glob_reexport")));
    }

    #[test]
    fn strict_wrapper_boundary_accepts_explicit_named_export_inside_libres() {
        let mut hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        if let Some(permissions) = hierarchy.nodes.get_mut("hierarchy.permissions") {
            permissions
                .properties
                .insert("wrapper_boundary_contract".to_string(), "explicit_ac_alias".to_string());
        }
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
pub type AC_String = std::string::String;
pub use std::time::Duration as AC_Duration;
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/libres/Std_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.permissions.wrapper_boundary_contract".to_string(),
            violations: &mut violations,
        };

        StrictWrapperBoundaryAxiom::default().execute(&mut ctx);

        assert!(ctx.violations.is_empty());
    }

    #[test]
    fn strict_wrapper_boundary_requires_ac_alias_contract() {
        let mut hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config/type_1");
        if let Some(permissions) = hierarchy.nodes.get_mut("hierarchy.permissions") {
            permissions
                .properties
                .insert("wrapper_boundary_contract".to_string(), "explicit_ac_alias".to_string());
        }
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };
        let source = r#"
extern crate alloc;
pub use alloc::string::String;
"#;
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/libres/Alloc_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.permissions.wrapper_boundary_contract".to_string(),
            violations: &mut violations,
        };

        StrictWrapperBoundaryAxiom::default().execute(&mut ctx);

        assert!(ctx.violations.iter().any(|violation| {
            violation
                .rule_id
                .contains("alloc_ac_alias_boundary")
        }));
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
