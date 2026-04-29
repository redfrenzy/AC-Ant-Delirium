#[cfg(test)]
mod tests {
    use crate::parser::CodeScanner;
    use crate::rules::axiom_identity::DNA_Suffix_Axiom_unit::DnaSuffixAxiom;
    use crate::rules::axiom_identity::Manifest_Anchoring_Axiom_unit::ManifestAnchoringAxiom;
    use crate::rules::axiom_identity::Name_Separator_Axiom_unit::NameSeparatorAxiom;
    use crate::rules::axiom_identity::Pascal_Case_Axiom_unit::PascalCaseAxiom;
    use crate::rules::axiom_identity::Recursion_Axiom_unit::RecursionAxiom;
    use crate::rules::axiom_identity::Repository_Anchor_Axiom_unit::RepositoryAnchorAxiom;
    use crate::rules::axiom_identity::Tag_Separator_Axiom_unit::TagSeparatorAxiom;
    use crate::structs::{
        AuditConfig, AuditContext, GovernanceAxiom, HierarchyConfig, ProjectContext,
    };

    fn config() -> AuditConfig {
        AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config/type_1"),
            ..AuditConfig::default()
        }
    }

    #[test]
    fn tag_separator_flags_wrong_separator_before_dna() {
        let config = config();
        let source = "pub fn demo() {}";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/exemples_error/identity_workspace/Tag-Separator-Error-unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.files".to_string(),
            violations: &mut violations,
        };

        TagSeparatorAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .any(|v| v.rule_id.contains("tag_separator")));
    }

    #[test]
    fn name_separator_flags_tag_separator_in_core_name() {
        let config = config();
        let source = "pub fn demo() {}";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/exemples_error/identity_workspace/Name_Separator_Error_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        NameSeparatorAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .any(|v| v.rule_id.contains("name_separator")));
    }

    #[test]
    fn pascal_case_flags_non_pascal_core_name() {
        let config = config();
        let source = "pub fn demo() {}";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/exemples_error/identity_workspace/pascal_case_error_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        PascalCaseAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .any(|v| v.rule_id.contains("pascal_case")));
    }

    #[test]
    fn dna_suffix_flags_logic_without_dna_tag() {
        let config = config();
        let source = "pub fn dna_suffix_error() -> usize { 4 }";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/exemples_error/identity_workspace/DnaSuffixError.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.files".to_string(),
            violations: &mut violations,
        };

        DnaSuffixAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .any(|v| v.rule_id.contains("dna_suffix")));
    }

    #[test]
    fn dna_suffix_ignores_generic_rust_entrypoints() {
        let config = config();
        let source = "pub fn main() {}";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "build.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.files".to_string(),
            violations: &mut violations,
        };

        DnaSuffixAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .all(|v| !v.rule_id.contains("dna_suffix")));
    }

    #[test]
    fn name_separator_ignores_generic_rust_entrypoints() {
        let config = config();
        let source = "pub fn main() {}";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "main.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.files".to_string(),
            violations: &mut violations,
        };

        NameSeparatorAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .all(|v| !v.rule_id.contains("name_separator")));
    }

    #[test]
    fn pascal_case_accepts_hyphenated_pascal_name_with_dna_suffix() {
        let config = config();
        let source = "pub fn demo() {}";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/exemples_error/identity_workspace/Example-Name_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        PascalCaseAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .all(|v| !v.rule_id.contains("pascal_case")));
    }

    #[test]
    fn manifest_anchoring_flags_child_md_with_tag() {
        let config = config();
        let source = "# Child Doc";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/exemples_error/identity_workspace/Guide_unit.md",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        ManifestAnchoringAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .any(|v| v.rule_id.contains("manifest_anchoring")));
    }

    #[test]
    fn recursion_flags_missing_companion_from_taxonomy_children() {
        let config = config();
        let source = "pub fn recursion_missing_unit() -> usize { 5 }";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "src/exemples_error/identity_workspace/Recursion-Missing_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        RecursionAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .any(|v| v.rule_id.contains("hierarchy.identity.recursion")));
    }

    #[test]
    fn repository_anchor_flags_absolute_path_outside_repository() {
        let config = config();
        let source = "pub fn repo_anchor() {}";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut ctx = AuditContext {
            file_path: "/tmp/repository_anchor_outside.rs",
            source,
            functions: &functions,
            config: &config,
            project: crate::structs::empty_project_context(),
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        RepositoryAnchorAxiom::default().execute(&mut ctx);
        assert!(ctx
            .violations
            .iter()
            .any(|v| v.rule_id.contains("repository_anchor")));
    }

    #[test]
    fn recursion_accepts_companion_present_in_project_index() {
        let config = config();
        let source = "pub fn recursion_ok_unit() -> usize { 5 }";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut project = ProjectContext::default();
        project.register_known_path("src/exemples_error/identity_workspace/Recursion-Ok_unit.rs");
        project
            .register_known_path("src/exemples_error/identity_workspace/Recursion-Ok_unitcore.rs");

        let mut ctx = AuditContext {
            file_path: "src/exemples_error/identity_workspace/Recursion-Ok_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: &project,
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        RecursionAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .all(|violation| !violation.rule_id.contains("hierarchy.identity.recursion")));
    }
}
