#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;

    use crate::structs::{
        AtomicProjectProjectionWriter, AuditConfig, AuditResult, AuditViolation,
        CanonicalStateStore, GovernanceErrorsRefreshRequest, HierarchyConfig, ProjectDatabase,
        ProjectStateRoot, StorageProject, StorageWorkspace,
    };
    use crate::Logs::{GovernanceErrorsProjectionTarget, GovernanceLogs};

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
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/logs-tests")
            .join(format!("{}_{}", label, nanos));
        fs::create_dir_all(&dir).expect("temporary directory should be created");
        dir
    }

    #[test]
    fn terminal_and_manifest_share_the_same_artifact_model() {
        let (config, result) = sample_result();
        let terminal = GovernanceLogs::format_terminal_report(&result, &config, None);
        let manifest = GovernanceLogs::to_violation_manifest(&result, &config, None);
        let shadow = GovernanceLogs::to_shadow_context(&result, &config, None);

        assert!(terminal.contains("P0: 1 erros"));
        assert!(terminal.contains("P1: 0 erros"));
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

        let paths = GovernanceLogs::write_artifacts(&result, &config, &output_root_str, None)
            .expect("log artifacts should be written");

        assert!(Path::new(&paths.terminal_report_path).exists());
        assert!(Path::new(&paths.violations_manifest_path).exists());
        assert!(Path::new(&paths.shadow_context_path).exists());
        assert!(Path::new(&paths.violations_dir).exists());

        let grouped_log = Path::new(&paths.violations_dir).join("04__type_logic.log");
        assert!(grouped_log.exists());

        let grouped_content =
            fs::read_to_string(&grouped_log).expect("type_logic grouped log must be readable");
        assert!(grouped_content.contains("axiom_id=type_logic"));
        assert!(grouped_content.contains("violations=1"));

        let log_count = fs::read_dir(&paths.violations_dir)
            .expect("violations dir should be readable")
            .filter_map(Result::ok)
            .count();
        assert_eq!(log_count, 23, "should keep one grouped log per axiom");

        let stale_log = Path::new(&paths.violations_dir).join("zz__stale.log");
        fs::write(&stale_log, "stale").expect("should create stale log");
        assert!(stale_log.exists());

        GovernanceLogs::write_artifacts(&result, &config, &output_root_str, None)
            .expect("rewriting log artifacts should clear stale files");

        assert!(
            !stale_log.exists(),
            "violations dir must be recreated before each write"
        );

        let rewritten_count = fs::read_dir(&paths.violations_dir)
            .expect("violations dir should stay readable")
            .filter_map(Result::ok)
            .count();
        assert_eq!(rewritten_count, 23);
    }

    #[test]
    fn logical_density_metadata_is_serialized_in_artifacts() {
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config/type_1"),
            ..AuditConfig::default()
        };

        let result = AuditResult {
            violations: vec![AuditViolation {
                file_path: "src/translator/Wgsl-Material-Translator_unit.rs".to_string(),
                line: 77,
                rule_id: "hierarchy.structural.logical_density.WGSLMaterialTranslator".to_string(),
                message: "logical density overflow - 185/56 statements. Fragment this logic."
                    .to_string(),
                snippet: None,
                quick_fix: None,
                detected_dna: "unit".to_string(),
                taxonomy_path: "hierarchy.files.unit.logical_density".to_string(),
            }],
            logs: vec![],
        };

        let manifest = GovernanceLogs::to_violation_manifest(&result, &config, None);
        let shadow = GovernanceLogs::to_shadow_context(&result, &config, None);

        assert!(manifest.contains("\"measurement_scope\": \"function\""));
        assert!(manifest.contains("\"metric_name\": \"logical_density\""));
        assert!(manifest.contains("\"metric_unit\": \"statements\""));
        assert!(manifest.contains("\"measured_value\": 185"));
        assert!(manifest.contains("\"threshold_value\": 56"));
        assert!(manifest.contains("\"measured_target_name\": \"WGSLMaterialTranslator\""));
        assert!(manifest.contains("\"remediation_hints\""));

        assert!(shadow.contains("\"measurement_scope\": \"function\""));
        assert!(shadow.contains("\"metric_name\": \"logical_density\""));
        assert!(shadow.contains("\"measured_value\": 185"));
        assert!(shadow.contains("\"threshold_value\": 56"));
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

    #[test]
    fn recursion_jurisdiction_is_reported_as_p0() {
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config/type_1"),
            ..AuditConfig::default()
        };

        let result = AuditResult {
            violations: vec![AuditViolation {
                file_path: "facets/red_core/src/Redcore_struct.rs".to_string(),
                line: 1,
                rule_id: "hierarchy.identity.recursion.jurisdiction".to_string(),
                message: "Recursion violation: 'hierarchy.files.struct' must stay inside jurisdiction 'src/structs', but observed 'facets/red_core/src/Redcore_struct.rs'.".to_string(),
                snippet: Some("src/structs".to_string()),
                quick_fix: Some("// [TODO] Move this file into the resolved jurisdiction or revise the hierarchy if the topology intentionally changed.".to_string()),
                detected_dna: "struct".to_string(),
                taxonomy_path: "hierarchy.identity.recursion".to_string(),
            }],
            logs: vec![],
        };

        let terminal = GovernanceLogs::format_terminal_report(&result, &config, None);
        let manifest = GovernanceLogs::to_violation_manifest(&result, &config, None);

        assert!(terminal.contains("P0: 1 erros"));
        assert!(terminal.contains("P1: 0 erros"));
        assert!(terminal.contains("P2: 0 erros"));
        assert!(manifest.contains("\"rule_id\": \"hierarchy.identity.recursion.jurisdiction\""));
        assert!(manifest.contains("\"priority\": \"P-0."));
        assert!(manifest.contains("\"level\": \"error\""));
    }

    #[test]
    fn writes_the_same_audit_violation_set_as_a_sanitized_project_projection() {
        let root = unique_temp_dir("ac_ant_delirium_governance_projection");
        let project_root = root.join("source-project");
        let state_root = ProjectStateRoot::with_default_config(
            root.join("protected/project"),
            project_root.clone(),
            root.join("source-worktree"),
        );
        fs::create_dir_all(&project_root).expect("project root should exist");
        fs::create_dir_all(state_root.workspace_root()).expect("workspace root should exist");
        let database =
            ProjectDatabase::initialize(&state_root).expect("database should initialize");
        CanonicalStateStore::register_project(
            &database,
            &StorageProject {
                project_id: "prj_00000000000000000000000000".to_string(),
                project_key: "governance-projection-project".to_string(),
                created_at: "2026-08-11T00:00:00Z".to_string(),
            },
        )
        .expect("project should register");
        CanonicalStateStore::register_workspace(
            &database,
            &StorageWorkspace {
                workspace_id: "wsp_00000000000000000000000000".to_string(),
                project_id: "prj_00000000000000000000000000".to_string(),
                workspace_key: "governance-projection-workspace".to_string(),
                created_at: "2026-08-11T00:00:00Z".to_string(),
            },
        )
        .expect("workspace should register");
        let writer = AtomicProjectProjectionWriter::new(database.clone(), state_root.clone())
            .expect("writer should bind to the state root");
        let request = GovernanceErrorsRefreshRequest {
            project_id: "prj_00000000000000000000000000".to_string(),
            workspace_id: "wsp_00000000000000000000000000".to_string(),
            rendered_at: "2026-08-11T00:00:00Z".to_string(),
        };
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config/type_1"),
            ..AuditConfig::default()
        };
        let result = AuditResult {
            violations: vec![
                AuditViolation {
                    file_path: "src/private_adapter.rs".to_string(),
                    line: 23,
                    rule_id: "hierarchy.structural.type_logic".to_string(),
                    message: "Move the API from /home/operator/private/source.rs.".to_string(),
                    snippet: Some("pub fn leaked() {}".to_string()),
                    quick_fix: Some("Do not inspect C:\\private\\plans; move the API.".to_string()),
                    detected_dna: "unitcore".to_string(),
                    taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
                },
                AuditViolation {
                    file_path: "src/manual_fix.rs".to_string(),
                    line: 9,
                    rule_id: "hierarchy.structural.max_nesting".to_string(),
                    message: "Refactor the nested branch manually.".to_string(),
                    snippet: Some("if nested {".to_string()),
                    quick_fix: None,
                    detected_dna: "unitcore".to_string(),
                    taxonomy_path: "hierarchy.files.unit.unitcore".to_string(),
                },
                AuditViolation {
                    file_path: "plan/private_task.json".to_string(),
                    line: 1,
                    rule_id: "hierarchy.structural.type_logic".to_string(),
                    message: "Administrative plan violation must stay private.".to_string(),
                    snippet: Some("\"completed\": true".to_string()),
                    quick_fix: None,
                    detected_dna: "administrative".to_string(),
                    taxonomy_path: "private.plan".to_string(),
                },
            ],
            logs: vec!["private administrative log".to_string()],
        };

        let paths = GovernanceLogs::write_artifacts(
            &result,
            &config,
            &root.join("legacy-artifacts").to_string_lossy(),
            Some(GovernanceErrorsProjectionTarget {
                writer: &writer,
                request: &request,
            }),
        )
        .expect("legacy artifacts and typed projection should write");

        let legacy = fs::read_to_string(paths.violations_manifest_path)
            .expect("legacy manifest should remain available");
        let projection = fs::read_to_string(state_root.governance_error_projection_path())
            .expect("governance projection should read");
        assert!(legacy.contains("hierarchy.structural.type_logic"));
        assert!(legacy.contains("src/private_adapter.rs"));
        assert!(legacy.contains("plan/private_task.json"));
        for required in [
            "\"artifact_kind\": \"governance_errors\"",
            "\"violation_id\": \"vio_",
            "\"rule_id\": \"hierarchy.structural.type_logic\"",
            "\"path\": \"src/private_adapter.rs\"",
            "\"path\": \"src/manual_fix.rs\"",
            "\"line\": 23",
            "\"public_rule_reference\": \"hierarchy.rule.type_logic\"",
            "[project-root]",
        ] {
            assert!(
                projection.contains(required),
                "missing projected value: {required}"
            );
        }
        for forbidden in [
            "/home/",
            "C:\\\\private",
            "\"snippet\"",
            "\"taxonomy_path\"",
            "\"detected_dna\"",
            "\"log_file\"",
            "\"installation_command\"",
            "\"api_rule_command\"",
            "\"audited_files\"",
            "\"priority\"",
            "private administrative log",
            "plan/private_task.json",
            "Administrative plan violation",
        ] {
            assert!(
                !projection.contains(forbidden),
                "forbidden projection content leaked: {forbidden}"
            );
        }
        assert_eq!(database.event_count(&request.project_id).unwrap(), 2);

        #[cfg(unix)]
        let projection_inode = fs::metadata(state_root.governance_error_projection_path())
            .unwrap()
            .ino();
        let later_request = GovernanceErrorsRefreshRequest {
            rendered_at: "2026-08-11T00:05:00Z".to_string(),
            ..request.clone()
        };
        GovernanceLogs::write_artifacts(
            &result,
            &config,
            &root.join("legacy-artifacts").to_string_lossy(),
            Some(GovernanceErrorsProjectionTarget {
                writer: &writer,
                request: &later_request,
            }),
        )
        .expect("identical governance refresh should be deduplicated");
        assert_eq!(
            fs::read_to_string(state_root.governance_error_projection_path()).unwrap(),
            projection
        );
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(state_root.governance_error_projection_path())
                .unwrap()
                .ino(),
            projection_inode
        );
        assert_eq!(
            database.event_count(&request.project_id).unwrap(),
            2,
            "identical audit and projection content must not create new events"
        );

        let _ = fs::remove_dir_all(root);
    }
}
