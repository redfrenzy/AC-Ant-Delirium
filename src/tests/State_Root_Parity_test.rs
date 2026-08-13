#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::structs::{
        build_current_task_projection, build_governance_errors_projection,
        AtomicProjectProjectionWriter, AuditStore, CanonicalStateSnapshot, CanonicalStateStore,
        CurrentTaskProjectionDocument, GovernanceErrorsProjectionDocument,
        LegacyProjectStateRootAdapter, NewAuditEvent, ProjectDatabase, ProjectProjectionWriter,
        ProjectStateRoot, ProjectedCurrentTask, ProjectedGovernanceViolation, ProjectedQueueStatus,
        ProjectedTaskAction, ProjectionMetadata, StorageHealth, StorageProject, StorageWorkspace,
        AUDIT_EVENT_CONTRACT_VERSION,
    };

    const PROJECT_ID: &str = "prj_00000000000000000000000000";
    const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";

    #[derive(Debug, PartialEq, Eq)]
    struct ParitySnapshot {
        canonical: CanonicalStateSnapshot,
        health: StorageHealth,
        current_task: Vec<u8>,
        governance_errors: Vec<u8>,
        audit_events: i64,
        guard_entries: Vec<String>,
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "antdelirium-state-root-parity-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn create_project(root: &Path) -> (Vec<u8>, Vec<u8>) {
        fs::create_dir_all(root.join("src")).expect("project source directory should exist");
        let manifest = b"[package]\nname = \"parity-fixture\"\nversion = \"0.1.0\"\n".to_vec();
        let source = b"pub fn parity_fixture() -> bool { true }\n".to_vec();
        fs::write(root.join("Cargo.toml"), &manifest).expect("manifest should write");
        fs::write(root.join("src/lib.rs"), &source).expect("source should write");
        (manifest, source)
    }

    fn relative_files(root: &Path) -> Vec<String> {
        fn visit(root: &Path, current: &Path, files: &mut Vec<String>) {
            for entry in fs::read_dir(current)
                .expect("project directory should be readable")
                .filter_map(Result::ok)
            {
                let path = entry.path();
                if path.is_dir() {
                    visit(root, &path, files);
                } else {
                    files.push(
                        path.strip_prefix(root)
                            .expect("project entry should remain below root")
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }

        let mut files = Vec::new();
        visit(root, root, &mut files);
        files.sort();
        files
    }

    fn metadata(generation: i64, fingerprint_index: usize) -> ProjectionMetadata {
        ProjectionMetadata {
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            source_revision: 0,
            generation,
            fingerprint: format!("{fingerprint_index:064x}"),
            rendered_at: "2026-08-11T00:00:00Z".to_string(),
        }
    }

    fn audit_event(index: usize, fingerprint_index: usize, resource_id: &str) -> NewAuditEvent {
        NewAuditEvent {
            contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
            event_id: format!("evt_{index:026}"),
            project_id: PROJECT_ID.to_string(),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            actor_kind: "service".to_string(),
            actor_subject_id: "antdelirium-service".to_string(),
            event_kind: "projection_rendered".to_string(),
            outcome: "succeeded".to_string(),
            occurred_at: "2026-08-11T00:00:00Z".to_string(),
            fingerprint: format!("{fingerprint_index:064x}"),
            correlation_id: format!("state-root-parity-{index}"),
            resource_id: Some(resource_id.to_string()),
            payload: format!("projection-{resource_id}").into_bytes(),
        }
    }

    fn exercise_same_suite(state_root: &ProjectStateRoot) -> ParitySnapshot {
        let database = ProjectDatabase::initialize(state_root).expect("database should initialize");
        CanonicalStateStore::register_project(
            &database,
            &StorageProject {
                project_id: PROJECT_ID.to_string(),
                project_key: "state-root-parity-project".to_string(),
                created_at: "2026-08-11T00:00:00Z".to_string(),
            },
        )
        .expect("project should register");
        CanonicalStateStore::register_workspace(
            &database,
            &StorageWorkspace {
                workspace_id: WORKSPACE_ID.to_string(),
                project_id: PROJECT_ID.to_string(),
                workspace_key: "state-root-parity-workspace".to_string(),
                created_at: "2026-08-11T00:00:00Z".to_string(),
            },
        )
        .expect("workspace should register");
        let writer = AtomicProjectProjectionWriter::new(database.clone(), state_root.clone())
            .expect("writer should bind to state root");

        let current = build_current_task_projection(&CurrentTaskProjectionDocument {
            metadata: metadata(1, 201),
            task: Some(ProjectedCurrentTask {
                task_id: "tsk_00000000000000000000000000".to_string(),
                task_revision: 1,
                objective: "Implement the parity fixture.".to_string(),
                allowed_action: ProjectedTaskAction::Implement,
                acceptance_criteria: vec!["Run the project test suite.".to_string()],
                relevant_paths: vec!["src/lib.rs".to_string()],
            }),
            queue_status: ProjectedQueueStatus::TaskAvailable,
            submission_instruction: "Submit evidence with hierarchy.task.current.submit."
                .to_string(),
        })
        .expect("current-task projection should build");
        writer
            .write_current_task(&current, &audit_event(1, 201, "current_task"))
            .expect("current-task projection should write");

        let errors = build_governance_errors_projection(&GovernanceErrorsProjectionDocument {
            metadata: metadata(2, 202),
            violations: vec![ProjectedGovernanceViolation {
                violation_id: "vio_00000000000000000000000000".to_string(),
                rule_id: "hierarchy.structural.max_nesting".to_string(),
                summary: "Maximum nesting exceeded.".to_string(),
                path: "src/lib.rs".to_string(),
                line: 1,
                quick_fix: Some("Extract the nested branch into a helper.".to_string()),
                public_rule_reference: "hierarchy.rule.max_nesting".to_string(),
            }],
        })
        .expect("governance-errors projection should build");
        writer
            .write_governance_errors(&errors, &audit_event(2, 202, "governance_errors"))
            .expect("governance-errors projection should write");

        let mut guard_entries = fs::read_dir(state_root.project_guard_dir())
            .expect("guard directory should exist")
            .map(|entry| {
                entry
                    .expect("guard entry should read")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        guard_entries.sort();

        ParitySnapshot {
            canonical: CanonicalStateStore::load_canonical_state(
                &database,
                PROJECT_ID,
                WORKSPACE_ID,
            )
            .expect("canonical state should load"),
            health: database.health().expect("storage health should load"),
            current_task: fs::read(state_root.todo_projection_path())
                .expect("current-task projection should read"),
            governance_errors: fs::read(state_root.governance_error_projection_path())
                .expect("governance-errors projection should read"),
            audit_events: AuditStore::audit_event_count(&database, PROJECT_ID)
                .expect("audit event count should load"),
            guard_entries,
        }
    }

    #[test]
    fn legacy_local_and_external_state_roots_have_functional_parity() {
        let root = unique_temp_dir("full");
        let local_project = root.join("local-project");
        let external_project = root.join("external-project");
        let local_original = create_project(&local_project);
        let external_original = create_project(&external_project);

        let legacy_plugin = local_project.join("plugins/parity-fixture");
        let local_state = LegacyProjectStateRootAdapter::new(
            legacy_plugin.clone(),
            legacy_plugin.join("ant_delirium_config"),
            local_project.clone(),
            local_project.clone(),
        )
        .into_state_root();
        let external_state = ProjectStateRoot::with_default_config(
            root.join("protected/state-root"),
            external_project.clone(),
            external_project.clone(),
        );

        let local_snapshot = exercise_same_suite(&local_state);
        let external_snapshot = exercise_same_suite(&external_state);

        assert_eq!(local_snapshot, external_snapshot);
        assert_eq!(
            local_snapshot.guard_entries,
            vec!["governance_error.json", "to-do_list.json"]
        );
        assert_eq!(
            fs::read(local_project.join("Cargo.toml")).unwrap(),
            local_original.0
        );
        assert_eq!(
            fs::read(local_project.join("src/lib.rs")).unwrap(),
            local_original.1
        );
        assert_eq!(
            fs::read(external_project.join("Cargo.toml")).unwrap(),
            external_original.0
        );
        assert_eq!(
            fs::read(external_project.join("src/lib.rs")).unwrap(),
            external_original.1
        );
        assert!(local_state.database_path().starts_with(&legacy_plugin));
        assert!(external_state
            .database_path()
            .starts_with(root.join("protected")));
        assert!(!local_state.root().join("src/lib.rs").exists());
        assert!(!external_state.root().join("src/lib.rs").exists());
        assert!(!external_state.root().join("Cargo.toml").exists());
        assert!(!external_project.join("plugins").exists());
        assert!(!external_project.join("AC_Ant-Delirium_props").exists());
        assert_eq!(
            relative_files(&external_project),
            vec![
                "Cargo.toml",
                "guard/governance_error.json",
                "guard/to-do_list.json",
                "src/lib.rs",
            ],
            "external mode may add only the two public projections to the project"
        );

        fs::remove_dir_all(root).expect("temporary parity tree should be removed");
    }
}
