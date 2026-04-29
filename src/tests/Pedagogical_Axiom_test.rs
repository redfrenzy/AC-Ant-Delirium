#[cfg(test)]
mod tests {
    use crate::parser::CodeScanner;
    use crate::rules::axiom_pedagogical::Doc_Tags_Axiom_unit::DocTagsAxiom;
    use crate::rules::axiom_pedagogical::Root_MD_Pairing_Axiom_unit::RootMdPairingAxiom;
    use crate::rules::axiom_pedagogical::Test_Case_Mapping_Axiom_unit::TestCaseMappingAxiom;
    use crate::rules::axiom_pedagogical::What_Mission_Axiom_unit::WhatMissionAxiom;
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
    fn root_md_pairing_flags_missing_manifest_from_project_index() {
        let config = config();
        let source = "# Root Guide\n";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut project = ProjectContext::default();
        project.register_known_path("src/exemples_error/pedagogical_workspace/RootGuide.md");

        let mut ctx = AuditContext {
            file_path: "src/exemples_error/pedagogical_workspace/RootGuide.md",
            source,
            functions: &functions,
            config: &config,
            project: &project,
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.files".to_string(),
            violations: &mut violations,
        };

        RootMdPairingAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains("root_md_pairing")));
    }

    #[test]
    fn root_md_pairing_accepts_manifest_present_in_project_index() {
        let config = config();
        let source = "# Root Guide\n";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let mut project = ProjectContext::default();
        project.register_known_path("src/exemples_error/pedagogical_workspace/RootGuide.md");
        project.register_known_path(
            "src/exemples_error/pedagogical_workspace/RootGuide_manifest.json",
        );

        let mut ctx = AuditContext {
            file_path: "src/exemples_error/pedagogical_workspace/RootGuide.md",
            source,
            functions: &functions,
            config: &config,
            project: &project,
            detected_dna: String::new(),
            taxonomy_path: "hierarchy.files".to_string(),
            violations: &mut violations,
        };

        RootMdPairingAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .all(|violation| !violation.rule_id.contains("root_md_pairing")));
    }

    #[test]
    fn test_case_mapping_stays_inactive_when_local_documentation_contract_omits_test() {
        let config = config();
        let source = "fn internal_unitcore_worker() {}\n";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let project = ProjectContext::default();

        let mut ctx = AuditContext {
            file_path: "src/Internal-Worker_unitcore.rs",
            source,
            functions: &functions,
            config: &config,
            project: &project,
            detected_dna: "unitcore".to_string(),
            taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
            violations: &mut violations,
        };

        TestCaseMappingAxiom::default().execute(&mut ctx);

        assert!(ctx
            .violations
            .iter()
            .all(|violation| !violation.rule_id.contains("test_case_mapping")));
    }

    #[test]
    fn what_mission_uses_taxonomy_min_and_info_from_tag_node() {
        let mut config = config();
        if let Some(node) = config.hierarchy.nodes.get_mut("hierarchy") {
            node.properties
                .insert("doc_tags".to_string(), "false".to_string());
        }
        let source = "/// [WHAT] tiny\nfn internal_unitcore_worker() {}\n";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let project = ProjectContext::default();

        let mut ctx = AuditContext {
            file_path: "src/Internal-Worker_unitcore.rs",
            source,
            functions: &functions,
            config: &config,
            project: &project,
            detected_dna: "unitcore".to_string(),
            taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
            violations: &mut violations,
        };

        WhatMissionAxiom::default().execute(&mut ctx);

        assert!(ctx.violations.iter().any(|violation| {
            violation.rule_id.contains("what_mission")
                && violation.message.contains("too short")
                && violation
                    .quick_fix
                    .as_deref()
                    .unwrap_or("")
                    .contains("Describe what this internal implementation symbol does.")
        }));
    }

    #[test]
    fn doc_tags_aggregates_missing_and_short_contract_into_single_violation() {
        let config = config();
        let source = "/// [WHAT] tiny\nfn public_unit_worker() {}\n";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let project = ProjectContext::default();

        let mut ctx = AuditContext {
            file_path: "src/Public-Worker_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: &project,
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        DocTagsAxiom::default().execute(&mut ctx);

        assert_eq!(ctx.violations.len(), 1);
        let violation = &ctx.violations[0];
        assert!(violation.rule_id.contains("doc_tags"));
        assert!(violation.message.contains("[WHY]"));
        assert!(violation.message.contains("[USAGE]"));
        assert!(violation.message.contains("[TEST]"));
        assert!(violation.message.contains("[WHAT] is too short (4/64)"));
        let quick_fix = violation.quick_fix.as_deref().unwrap_or("");
        assert!(quick_fix.contains("/// [WHAT]"));
        assert!(quick_fix.contains("/// [WHY]"));
        assert!(quick_fix.contains("/// [USAGE]"));
        assert!(quick_fix.contains("/// [TEST]"));
    }

    #[test]
    fn doc_tags_detects_word_salad_inside_present_tag() {
        let config = config();
        let source = "/// [WHAT] parser parse parser data parse parser data parser parse parser data.\n/// [WHY] Explains the real architectural reason for this behavior in the system.\n/// [USAGE] Call this from the public unit layer when normalization is required.\n/// [TEST] Covered by the parser contract validation suite.\nfn parse_parser_data() {}\n";
        let functions = CodeScanner::scan_blocks(source);
        let mut violations = Vec::new();
        let project = ProjectContext::default();

        let mut ctx = AuditContext {
            file_path: "src/Parse-Parser-Data_unit.rs",
            source,
            functions: &functions,
            config: &config,
            project: &project,
            detected_dna: "unit".to_string(),
            taxonomy_path: "hierarchy.files.unit".to_string(),
            violations: &mut violations,
        };

        DocTagsAxiom::default().execute(&mut ctx);

        assert_eq!(ctx.violations.len(), 1);
        let violation = &ctx.violations[0];
        assert!(violation.rule_id.contains("doc_tags"));
        assert!(violation.message.contains("[WHAT] looks like word_salad"));
        assert!(violation.message.contains("ratio"));
    }
}
