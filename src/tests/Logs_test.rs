#[cfg(test)]
mod tests {
    use crate::Logs::GovernanceLogs;
    use crate::structs::{AuditConfig, AuditResult, AuditViolation, HierarchyConfig};

    fn sample_result() -> (AuditConfig, AuditResult) {
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config"),
            ..AuditConfig::default()
        };

        let result = AuditResult {
            violations: vec![
                AuditViolation {
                    file_path: "src/exemples_error/Type-Logic-Error_unitcore.rs".to_string(),
                    line: 3,
                    rule_id: "hierarchy.structural.type_logic".to_string(),
                    message: "Type Logic Violation".to_string(),
                    snippet: Some("pub fn leaked() {}".to_string()),
                    quick_fix: Some("Move public API to the parent DNA.".to_string()),
                    detected_dna: "unitcore".to_string(),
                    taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
                }
            ],
            logs: vec!["[TEST] synthetic log bundle".to_string()],
        };

        (config, result)
    }

    #[test]
    fn terminal_and_manifest_share_the_same_artifact_model() {
        let (config, result) = sample_result();
        let terminal = GovernanceLogs::format_terminal_report(&result, &config, None);
        let manifest = GovernanceLogs::to_violation_manifest(&result, &config, None);
        let shadow = GovernanceLogs::to_shadow_context(&result, &config, None);

        assert!(terminal.contains("src/Logs/artifacts/violations/0001__hierarchy_structural_type_logic__src_exemples_error_Type_Logic_Error_unitcore_rs.log"));
        assert!(manifest.contains("\"log_file\""));
        assert!(manifest.contains("hierarchy.structural.type_logic"));
        assert!(shadow.contains("\"results\""));
        assert!(shadow.contains("\"ruleId\": \"hierarchy.structural.type_logic\""));
    }

    #[test]
    fn writes_log_artifacts_to_disk() {
        let (config, result) = sample_result();
        let output_root = std::env::temp_dir().join("ac_ant_delirium_logs_test");
        let output_root_str = output_root.to_string_lossy().to_string();

        let paths = GovernanceLogs::write_artifacts(&result, &config, &output_root_str)
            .expect("log artifacts should be written");

        assert!(std::path::Path::new(&paths.terminal_report_path).exists());
        assert!(std::path::Path::new(&paths.violations_manifest_path).exists());
        assert!(std::path::Path::new(&paths.shadow_context_path).exists());
        assert!(std::path::Path::new(&paths.violations_dir).exists());
    }
}
