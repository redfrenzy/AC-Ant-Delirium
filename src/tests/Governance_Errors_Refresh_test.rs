#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::libres::Rusqlite_unit::AC_params;
    use crate::structs::{
        build_governance_errors_projection, AtomicProjectProjectionWriter, AuditStore,
        CanonicalGovernanceSnapshotRequest, CanonicalStateStore,
        GovernanceErrorsProjectionDocument, GovernanceErrorsRefreshRequest, ProjectDatabase,
        ProjectProjectionWriter, ProjectStateRoot, ProjectedGovernanceViolation,
        ProjectionMetadata, StorageProject, StorageWorkspace,
    };

    const PROJECT_ID: &str = "prj_00000000000000000000000000";
    const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";
    const SECOND_WORKSPACE_ID: &str = "wsp_00000000000000000000000001";

    fn unique_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/governance-refresh-tests")
            .join(format!("{label}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).expect("fixture root");
        root
    }

    fn initialized(label: &str) -> (PathBuf, ProjectDatabase, AtomicProjectProjectionWriter) {
        let root = unique_root(label);
        let state_root = ProjectStateRoot::with_default_config(
            root.join("protected/project"),
            root.join("source-project"),
            root.join("source-worktree"),
        );
        fs::create_dir_all(state_root.project_root()).expect("project root");
        fs::create_dir_all(state_root.workspace_root()).expect("workspace root");
        let database = ProjectDatabase::initialize(&state_root).expect("database");
        database
            .register_project(&StorageProject {
                project_id: PROJECT_ID.to_string(),
                project_key: format!("governance-{label}"),
                created_at: "2026-08-13T00:00:00Z".to_string(),
            })
            .expect("project");
        database
            .register_workspace(&StorageWorkspace {
                workspace_id: WORKSPACE_ID.to_string(),
                project_id: PROJECT_ID.to_string(),
                workspace_key: "primary".to_string(),
                created_at: "2026-08-13T00:00:00Z".to_string(),
            })
            .expect("workspace");
        let writer = AtomicProjectProjectionWriter::new(database.clone(), state_root)
            .expect("projection writer");
        (root, database, writer)
    }

    fn violation(path: &str) -> ProjectedGovernanceViolation {
        ProjectedGovernanceViolation {
            violation_id: "vio_00000000000000000000000000".to_string(),
            rule_id: "hierarchy.structural.max_nesting".to_string(),
            summary: "Maximum nesting exceeded.".to_string(),
            path: path.to_string(),
            line: 7,
            quick_fix: None,
            public_rule_reference: "hierarchy.rule.max_nesting".to_string(),
        }
    }

    fn snapshot(
        violations: Vec<ProjectedGovernanceViolation>,
    ) -> CanonicalGovernanceSnapshotRequest {
        CanonicalGovernanceSnapshotRequest {
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            observed_at: "2026-08-13T00:00:00Z".to_string(),
            violations,
        }
    }

    fn refresh() -> GovernanceErrorsRefreshRequest {
        GovernanceErrorsRefreshRequest {
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            rendered_at: "2026-08-13T00:00:01Z".to_string(),
        }
    }

    #[test]
    fn projects_only_the_protected_snapshot_and_accepts_an_empty_set() {
        let (root, database, writer) = initialized("empty");
        writer
            .replace_canonical_governance_snapshot(&snapshot(Vec::new()))
            .expect("empty snapshot is authoritative");
        writer
            .refresh_governance_errors_projection(&refresh())
            .expect("empty projection");

        let rendered = fs::read_to_string(writer.governance_errors_path()).expect("projection");
        assert!(rendered.contains("\"violations\": [\n\n  ]"));
        assert_eq!(database.audit_event_count(PROJECT_ID).unwrap(), 2);
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn missing_stale_or_tampered_snapshot_never_writes_a_projection_event() {
        let (missing_root, missing_db, missing_writer) = initialized("missing");
        assert!(missing_writer
            .refresh_governance_errors_projection(&refresh())
            .is_err());
        assert!(!missing_writer.governance_errors_path().exists());
        assert_eq!(missing_db.audit_event_count(PROJECT_ID).unwrap(), 0);
        fs::remove_dir_all(missing_root).expect("fixture cleanup");

        let (stale_root, stale_db, stale_writer) = initialized("stale");
        stale_writer
            .replace_canonical_governance_snapshot(&snapshot(vec![violation("src/lib.rs")]))
            .expect("snapshot");
        stale_db
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "UPDATE projects SET state_revision = state_revision + 1 WHERE project_id = ?1",
                    AC_params![PROJECT_ID],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(stale_writer
            .refresh_governance_errors_projection(&refresh())
            .is_err());
        assert!(!stale_writer.governance_errors_path().exists());
        assert_eq!(stale_db.audit_event_count(PROJECT_ID).unwrap(), 1);
        fs::remove_dir_all(stale_root).expect("fixture cleanup");

        let (tampered_root, tampered_db, tampered_writer) = initialized("tampered");
        tampered_writer
            .replace_canonical_governance_snapshot(&snapshot(vec![violation("src/lib.rs")]))
            .expect("snapshot");
        tampered_db
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "UPDATE governance_snapshots SET payload = ?3 WHERE project_id = ?1 AND workspace_id = ?2",
                    AC_params![PROJECT_ID, WORKSPACE_ID, b"tampered".as_slice()],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(tampered_writer
            .refresh_governance_errors_projection(&refresh())
            .is_err());
        assert!(!tampered_writer.governance_errors_path().exists());
        assert_eq!(tampered_db.audit_event_count(PROJECT_ID).unwrap(), 1);
        fs::remove_dir_all(tampered_root).expect("fixture cleanup");
    }

    #[test]
    fn administrative_paths_and_unbound_direct_writes_are_rejected() {
        for path in [
            "guard/governance_error.json",
            "plugins/tool/config.json",
            "plan/current.json",
            "Plans/private.json",
            "ant_delirium_config/default_rules.toml",
            "AC_Ant-Delirium_props/Logs/private.log",
            ".git/config",
        ] {
            let (root, database, writer) = initialized(&format!("reserved-{}", path.len()));
            assert!(writer
                .replace_canonical_governance_snapshot(&snapshot(vec![violation(path)]))
                .is_err());
            assert_eq!(database.audit_event_count(PROJECT_ID).unwrap(), 0);
            assert!(!writer.governance_errors_path().exists());
            fs::remove_dir_all(root).expect("fixture cleanup");
        }

        let (root, database, writer) = initialized("unbound");
        let projection = build_governance_errors_projection(&GovernanceErrorsProjectionDocument {
            metadata: ProjectionMetadata {
                project_id: PROJECT_ID.to_string(),
                workspace_id: WORKSPACE_ID.to_string(),
                source_revision: 0,
                generation: 1,
                fingerprint: "1".repeat(64),
                rendered_at: "2026-08-13T00:00:01Z".to_string(),
            },
            violations: vec![violation("src/lib.rs")],
        })
        .expect("typed projection");
        let event = crate::structs::NewAuditEvent {
            contract_version: crate::structs::AUDIT_EVENT_CONTRACT_VERSION.to_string(),
            event_id: "evt_00000000000000000000000000".to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            actor_kind: "service".to_string(),
            actor_subject_id: "antdelirium-service".to_string(),
            event_kind: "projection_rendered".to_string(),
            outcome: "succeeded".to_string(),
            occurred_at: "2026-08-13T00:00:01Z".to_string(),
            fingerprint: "1".repeat(64),
            correlation_id: "unbound-projection".to_string(),
            resource_id: Some("governance_errors".to_string()),
            payload: b"unbound".to_vec(),
        };
        assert!(writer.write_governance_errors(&projection, &event).is_err());
        assert_eq!(database.audit_event_count(PROJECT_ID).unwrap(), 0);
        assert!(!writer.governance_errors_path().exists());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn identical_violation_bytes_have_distinct_workspace_scoped_audit_identities() {
        let (root, database, writer) = initialized("workspace-scope");
        database
            .register_workspace(&StorageWorkspace {
                workspace_id: SECOND_WORKSPACE_ID.to_string(),
                project_id: PROJECT_ID.to_string(),
                workspace_key: "secondary".to_string(),
                created_at: "2026-08-13T00:00:00Z".to_string(),
            })
            .expect("second workspace");
        let shared = vec![violation("src/lib.rs")];
        writer
            .replace_canonical_governance_snapshot(&snapshot(shared.clone()))
            .expect("primary snapshot");
        writer
            .replace_canonical_governance_snapshot(&CanonicalGovernanceSnapshotRequest {
                project_id: PROJECT_ID.to_string(),
                workspace_id: SECOND_WORKSPACE_ID.to_string(),
                observed_at: "2026-08-13T00:00:00Z".to_string(),
                violations: shared,
            })
            .expect("secondary snapshot");

        assert_eq!(database.audit_event_count(PROJECT_ID).unwrap(), 2);
        database
            .with_immediate_transaction(|transaction| {
                let distinct = transaction.query_row(
                    "SELECT count(DISTINCT fingerprint) FROM governance_snapshots WHERE project_id = ?1",
                    AC_params![PROJECT_ID],
                    |row| row.get::<_, i64>(0),
                )?;
                assert_eq!(distinct, 2);
                Ok(())
            })
            .unwrap();
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
