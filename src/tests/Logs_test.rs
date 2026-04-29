#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::structs::{AuditConfig, AuditResult, AuditViolation, HierarchyConfig};
    use crate::Logs::GovernanceLogs;

    fn sample_result() -> (AuditConfig, AuditResult) {
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config/type_1"),
            ..AuditConfig::default()
        };

        let result = AuditResult {
            violations: vec![AuditViolation {
                file_path: "src/exemples_error/Type-Logic-Error_unitcore.rs".to_string(),
                line: 3,
                rule_id: "hierarchy.structural.type_logic".to_string(),
                message: "Type Logic Violation".to_string(),
                snippet: Some("pub fn leaked() {}".to_string()),
                quick_fix: Some("Move public API to the parent DNA.".to_string()),
                detected_dna: "unitcore".to_string(),
                taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
            }],
            logs: vec!["[TEST] synthetic log bundle".to_string()],
        };

        (config, result)
    }

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("{}_{}", label, nanos));
        fs::create_dir_all(&dir).expect("temporary directory should be created");
        dir
    }

    #[test]
    fn terminal_and_manifest_share_the_same_artifact_model() {
        let (config, result) = sample_result();
        let terminal = GovernanceLogs::format_terminal_report(&result, &config, None);
        let manifest = GovernanceLogs::to_violation_manifest(&result, &config, None);
        let shadow = GovernanceLogs::to_shadow_context(&result, &config, None);

        assert!(terminal.contains("P0: 0 erros"));
        assert!(terminal.contains("P1: 1 erros"));
        assert!(terminal.contains("P2: 0 erros"));
        assert!(terminal.contains("P3: 0 erros"));
        assert!(terminal.contains("P4: 0 erros"));
        assert!(terminal.contains("Total: 1 erros"));
        assert!(terminal.contains("Audited Files: "));
        assert!(terminal.contains(
            "Violations Manifest: AC_Ant-Delirium_props/Logs/artifacts/violations_manifest.json"
        ));
        assert!(terminal.contains(
            "Shadow Context: AC_Ant-Delirium_props/Logs/artifacts/shadow_context.sarif.json"
        ));
        assert!(terminal.contains("API Help: "));
        assert!(terminal.contains("--api hierarchy.help"));
        assert!(!terminal.contains("Comandos uteis:"));
        assert!(!terminal.contains("Focused diagnostics"));
        assert!(terminal.contains("Modo estrito: ativo."));
        assert!(manifest.contains("\"ai_guidance\""));
        assert!(manifest.contains("\"api_help_command\""));
        assert!(manifest.contains("\"log_file\""));
        assert!(manifest.contains("\"p4\": 0"));
        assert!(manifest.contains("hierarchy.structural.type_logic"));
        assert!(manifest.contains("\"rule_doc_id\": \"type_logic\""));
        assert!(manifest.contains("\"api_rule_path\": \"hierarchy.rule.type_logic\""));
        assert!(manifest.contains("\"api_taxonomy_path\": \"hierarchy.files.unit.unitcore\""));
        assert!(manifest.contains("\"feedback_command\": "));
        assert!(manifest.contains("--help.feedback"));
        assert!(manifest.contains("\"taxonomy_guidance\""));
        assert!(manifest.contains("\"rule_guidance\""));
        assert!(manifest.contains("\"remediation_kind\": \"align_declaration_dna\""));
        assert!(manifest.contains("type_logic.log"));
        assert!(shadow.contains("\"results\""));
        assert!(shadow.contains("\"priority_order\": \"P0 → P1 → P2 → P3 → P4\""));
        assert!(shadow.contains("\"p4\": 0"));
        assert!(shadow.contains("\"ruleId\": \"hierarchy.structural.type_logic\""));
        assert!(shadow.contains("\"api_rule_path\": \"hierarchy.rule.type_logic\""));
        assert!(shadow.contains("\"feedback_command\": "));
        assert!(shadow.contains("\"feedback_help_command\": "));
        assert!(shadow.contains("\"taxonomy_guidance\""));
        assert!(shadow.contains("\"rule_guidance\""));
        assert!(shadow.contains("type_logic.log"));
    }

    #[test]
    fn writes_log_artifacts_to_disk() {
        let (config, result) = sample_result();
        let output_root = std::env::temp_dir().join("ac_ant_delirium_logs_test");
        let output_root_str = output_root.to_string_lossy().to_string();
        let _ = fs::remove_dir_all(&output_root);

        let paths = GovernanceLogs::write_artifacts(&result, &config, &output_root_str)
            .expect("log artifacts should be written");

        assert!(Path::new(&paths.terminal_report_path).exists());
        assert!(Path::new(&paths.violations_manifest_path).exists());
        assert!(Path::new(&paths.shadow_context_path).exists());
        assert!(Path::new(&paths.violations_dir).exists());

        let grouped_log = Path::new(&paths.violations_dir).join("05__type_logic.log");
        assert!(grouped_log.exists());

        let grouped_content =
            fs::read_to_string(&grouped_log).expect("type_logic grouped log must be readable");
        assert!(grouped_content.contains("axiom_id=type_logic"));
        assert!(grouped_content.contains("violations=1"));

        let log_count = fs::read_dir(&paths.violations_dir)
            .expect("violations dir should be readable")
            .filter_map(Result::ok)
            .count();
        assert_eq!(log_count, 24, "should keep one grouped log per axiom");

        let stale_log = Path::new(&paths.violations_dir).join("zz__stale.log");
        fs::write(&stale_log, "stale").expect("should create stale log");
        assert!(stale_log.exists());

        GovernanceLogs::write_artifacts(&result, &config, &output_root_str)
            .expect("rewriting log artifacts should clear stale files");

        assert!(
            !stale_log.exists(),
            "violations dir must be recreated before each write"
        );

        let rewritten_count = fs::read_dir(&paths.violations_dir)
            .expect("violations dir should stay readable")
            .filter_map(Result::ok)
            .count();
        assert_eq!(rewritten_count, 24);
    }

    #[test]
    fn terminal_report_stays_compact_and_keeps_diagnostics_in_artifacts() {
        let workspace = unique_temp_dir("ac_ant_delirium_terminal_diagnostics");
        let source_path = workspace.join("src").join("sample.rs");
        fs::create_dir_all(source_path.parent().expect("parent dir should exist"))
            .expect("source dir should exist");
        fs::write(
            &source_path,
            "fn alpha() {\n    let x = 1;\n    let y = 2;\n    if x < y {\n        println!(\"nested\");\n    }\n}\n",
        )
        .expect("source fixture should be written");

        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config/type_1"),
            runtime: crate::structs::RuntimeConfig {
                project_root: workspace.to_string_lossy().to_string(),
                ..crate::structs::RuntimeConfig::default()
            },
            ..AuditConfig::default()
        };

        let result = AuditResult {
            violations: vec![AuditViolation {
                file_path: "src/sample.rs".to_string(),
                line: 4,
                rule_id: "hierarchy.structural.max_nesting.sample".to_string(),
                message: "Nesting Violation".to_string(),
                snippet: Some("if x < y {".to_string()),
                quick_fix: Some("Extract nested branches into helpers.".to_string()),
                detected_dna: "unit".to_string(),
                taxonomy_path: "hierarchy.files.unit.max_nesting".to_string(),
            }],
            logs: vec![],
        };

        let terminal = GovernanceLogs::format_terminal_report(&result, &config, None);
        assert!(terminal.contains("P1: 1 erros"));
        assert!(terminal.contains("Violations Manifest:"));
        assert!(terminal.contains("Shadow Context:"));
        assert!(terminal.contains("API Help:"));
        assert!(!terminal.contains("Focused diagnostics"));
        assert!(!terminal.contains(">  4 |     if x < y {"));
        assert!(!terminal.contains("--api hierarchy.rule.max_nesting"));

        let _ = fs::remove_dir_all(workspace);
    }
}
