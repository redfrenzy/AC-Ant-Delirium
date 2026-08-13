#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::libres::Rusqlite_unit::AC_params;
    use crate::structs::{
        build_current_task_projection, AtomicProjectProjectionWriter, AuditStore,
        CanonicalStateStore, CanonicalStateTransition, CurrentTaskProjection,
        CurrentTaskProjectionDocument, EventWriteOutcome, GovernanceErrorsProjection,
        NewAuditEvent, NewSubmission, ProjectDatabase, ProjectProjectionWriter, ProjectStateRoot,
        ProjectedCurrentTask, ProjectedQueueStatus, ProjectedTaskAction, ProjectionArtifactKind,
        ProjectionMetadata, StorageProject, StorageWorkspace, SubmissionStore,
        TransactionalStorageError, AGENT_PROJECTION_CONTRACT_VERSION, AUDIT_EVENT_CONTRACT_VERSION,
        CANONICAL_STATE_CONTRACT_VERSION, STORAGE_SCHEMA_VERSION, SUBMISSION_CONTRACT_VERSION,
    };

    const PROJECT_ID: &str = "prj_00000000000000000000000000";
    const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";
    const PLAN_ID: &str = "pln_00000000000000000000000000";
    const TASK_ID: &str = "tsk_00000000000000000000000000";

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/storage-interfaces-tests/{label}-{}-{nanos}",
            std::process::id(),
        ));
        fs::create_dir_all(&path).expect("temporary directory should be created");
        path
    }

    fn state_root(root: &Path) -> ProjectStateRoot {
        ProjectStateRoot::with_default_config(
            root.join("protected/project"),
            root.join("source-project"),
            root.join("source-worktree"),
        )
    }

    fn project() -> StorageProject {
        StorageProject {
            project_id: PROJECT_ID.to_string(),
            project_key: "project-key-v1".to_string(),
            created_at: "2026-08-11T00:00:00Z".to_string(),
        }
    }

    fn workspace() -> StorageWorkspace {
        StorageWorkspace {
            workspace_id: WORKSPACE_ID.to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_key: "workspace-key-v1".to_string(),
            created_at: "2026-08-11T00:00:00Z".to_string(),
        }
    }

    fn audit_event(
        index: usize,
        event_kind: &str,
        fingerprint_index: usize,
        resource_id: Option<&str>,
    ) -> NewAuditEvent {
        NewAuditEvent {
            contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
            event_id: format!("evt_{index:026}"),
            project_id: PROJECT_ID.to_string(),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            actor_kind: "service".to_string(),
            actor_subject_id: "antdelirium-service".to_string(),
            event_kind: event_kind.to_string(),
            outcome: "succeeded".to_string(),
            occurred_at: "2026-08-11T00:00:00Z".to_string(),
            fingerprint: format!("{fingerprint_index:064x}"),
            correlation_id: format!("correlation-{index}"),
            resource_id: resource_id.map(str::to_string),
            payload: format!("audit-{index}").into_bytes(),
        }
    }

    fn initialized(root: &Path) -> (ProjectStateRoot, ProjectDatabase) {
        let state_root = state_root(root);
        fs::create_dir_all(state_root.project_root()).expect("source project fixture");
        fs::create_dir_all(state_root.workspace_root()).expect("source worktree fixture");
        let database =
            ProjectDatabase::initialize(&state_root).expect("database should initialize");
        CanonicalStateStore::register_project(&database, &project()).unwrap();
        CanonicalStateStore::register_workspace(&database, &workspace()).unwrap();
        (state_root, database)
    }

    fn seed_plan_and_task(database: &ProjectDatabase) {
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO plans(plan_id, project_id, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
                    AC_params![PLAN_ID, PROJECT_ID, b"plan-v1".as_slice(), "2026-08-11T00:00:00Z"],
                )?;
                transaction.execute(
                    "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at) VALUES (?1, ?2, ?3, 1, 'available', 'not_required', ?4, ?5)",
                    AC_params![TASK_ID, PROJECT_ID, PLAN_ID, b"task-v1".as_slice(), "2026-08-11T00:00:00Z"],
                )?;
                Ok(())
            })
            .expect("canonical fixture should be inserted");
    }

    #[test]
    fn audit_store_does_not_mutate_the_canonical_state_revision() {
        let root = unique_temp_dir("audit-boundary");
        let (_, database) = initialized(&root);
        let event = audit_event(1, "compilation_observed", 1, None);

        assert_eq!(
            AuditStore::append_audit_event(&database, &event).unwrap(),
            EventWriteOutcome::Inserted { sequence: 1 }
        );
        let state =
            CanonicalStateStore::load_canonical_state(&database, PROJECT_ID, WORKSPACE_ID).unwrap();
        assert_eq!(state.contract_version, CANONICAL_STATE_CONTRACT_VERSION);
        assert_eq!(state.state_revision, 0);
        assert_eq!(state.last_event_sequence, 1);
        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            1
        );

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn canonical_transition_and_audit_commit_or_roll_back_together() {
        let root = unique_temp_dir("canonical-transition");
        let (_, database) = initialized(&root);
        seed_plan_and_task(&database);
        let transition = CanonicalStateTransition {
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            expected_state_revision: 0,
            active_plan_id: Some(PLAN_ID.to_string()),
            current_task_id: Some(TASK_ID.to_string()),
        };
        let event = audit_event(1, "state_transitioned", 10, Some(TASK_ID));

        let applied =
            CanonicalStateStore::apply_canonical_transition(&database, &transition, &event)
                .unwrap();
        assert_eq!(applied.audit, EventWriteOutcome::Inserted { sequence: 1 });
        assert_eq!(applied.state.state_revision, 1);
        assert_eq!(applied.state.current_task_id.as_deref(), Some(TASK_ID));
        assert_eq!(applied.state.tasks.len(), 1);

        let replay =
            CanonicalStateStore::apply_canonical_transition(&database, &transition, &event)
                .unwrap();
        assert_eq!(replay.audit, EventWriteOutcome::Duplicate { sequence: 1 });
        assert_eq!(replay.state.state_revision, 1);

        let stale_event = audit_event(2, "state_transitioned", 11, Some(TASK_ID));
        let stale =
            CanonicalStateStore::apply_canonical_transition(&database, &transition, &stale_event);
        assert!(matches!(
            stale,
            Err(TransactionalStorageError::Integrity(_))
        ));
        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            1
        );
        assert_eq!(
            AuditStore::last_audit_sequence(&database, PROJECT_ID).unwrap(),
            1
        );

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn submission_store_is_immutable_deduplicated_and_audited() {
        let root = unique_temp_dir("submission");
        let (_, database) = initialized(&root);
        seed_plan_and_task(&database);
        let submission = NewSubmission {
            contract_version: SUBMISSION_CONTRACT_VERSION.to_string(),
            submission_id: "sub_00000000000000000000000000".to_string(),
            request_id: "request-1".to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: TASK_ID.to_string(),
            task_revision: 1,
            base_revision: Some("git-revision-1".to_string()),
            fingerprint: format!("{:064x}", 20),
            payload: b"submission-v1".to_vec(),
            received_at: "2026-08-11T00:00:00Z".to_string(),
        };
        let mut event = audit_event(
            1,
            "submission_recorded",
            20,
            Some(&submission.submission_id),
        );

        let inserted = SubmissionStore::record_submission(&database, &submission, &event).unwrap();
        assert!(inserted.inserted());
        assert_eq!(inserted.event_sequence(), 1);
        let duplicate = SubmissionStore::record_submission(&database, &submission, &event).unwrap();
        assert!(!duplicate.inserted());
        assert_eq!(duplicate.submission_id(), submission.submission_id);
        assert_eq!(
            SubmissionStore::submission_count(&database, PROJECT_ID).unwrap(),
            1
        );

        let mut conflicting = submission.clone();
        conflicting.fingerprint = format!("{:064x}", 21);
        event.fingerprint = conflicting.fingerprint.clone();
        assert!(SubmissionStore::record_submission(&database, &conflicting, &event).is_err());
        assert_eq!(
            SubmissionStore::submission_count(&database, PROJECT_ID).unwrap(),
            1
        );
        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            1
        );

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn projection_writer_uses_only_the_two_fixed_project_paths() {
        let root = unique_temp_dir("projection");
        let (state_root, database) = initialized(&root);
        assert!(!state_root.project_guard_dir().exists());
        let writer =
            AtomicProjectProjectionWriter::new(database.clone(), state_root.clone()).unwrap();
        assert!(state_root.project_guard_dir().is_dir());
        assert!(fs::read_dir(state_root.project_guard_dir())
            .unwrap()
            .next()
            .is_none());
        let current_metadata = ProjectionMetadata {
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            source_revision: 0,
            generation: 1,
            fingerprint: format!("{:064x}", 30),
            rendered_at: "2026-08-11T00:00:00Z".to_string(),
        };
        let current = CurrentTaskProjection {
            contract_version: AGENT_PROJECTION_CONTRACT_VERSION.to_string(),
            metadata: current_metadata.clone(),
            task_ref: None,
            serialized_contract:
                b"{\"contract_version\":\"agent-projection-v1\",\"artifact_kind\":\"current_task\"}"
                    .to_vec(),
        };
        let current_event = audit_event(1, "projection_rendered", 30, Some("current_task"));
        let current_receipt = writer.write_current_task(&current, &current_event).unwrap();
        assert_eq!(
            current_receipt.artifact_kind,
            ProjectionArtifactKind::CurrentTask
        );

        let errors_metadata = ProjectionMetadata {
            generation: 2,
            fingerprint: format!("{:064x}", 31),
            ..current_metadata
        };
        let errors = GovernanceErrorsProjection {
            contract_version: AGENT_PROJECTION_CONTRACT_VERSION.to_string(),
            metadata: errors_metadata,
            serialized_contract: b"{\"contract_version\":\"agent-projection-v1\",\"artifact_kind\":\"governance_errors\"}".to_vec(),
        };
        let errors_event = audit_event(2, "projection_rendered", 31, Some("governance_errors"));
        writer
            .write_governance_errors(&errors, &errors_event)
            .unwrap();

        assert_eq!(
            writer.current_task_path(),
            state_root.todo_projection_path()
        );
        assert_eq!(
            writer.governance_errors_path(),
            state_root.governance_error_projection_path()
        );
        assert_eq!(
            fs::read(writer.current_task_path()).unwrap(),
            current.serialized_contract
        );
        assert_eq!(
            fs::read(writer.governance_errors_path()).unwrap(),
            errors.serialized_contract
        );
        let mut names = fs::read_dir(state_root.project_guard_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec!["governance_error.json", "to-do_list.json"]);
        let state = database
            .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
            .unwrap();
        assert_eq!(state.state_revision, 0);
        assert_eq!(state.projection_generation, 2);
        assert_eq!(state.last_event_sequence, 2);

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn storage_migrations_record_all_contract_and_authorization_versions() {
        let root = unique_temp_dir("migration-v3");
        let (_, database) = initialized(&root);

        database
            .with_immediate_transaction(|transaction| {
                let result = transaction.query_row(
                    "SELECT count(*), max(version) FROM schema_migrations",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )?;
                assert_eq!(result, (4, i64::from(STORAGE_SCHEMA_VERSION)));
                Ok(())
            })
            .unwrap();

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn rejects_unknown_contract_versions_before_persisting_any_boundary() {
        let root = unique_temp_dir("contract-version");
        let (state_root, database) = initialized(&root);
        seed_plan_and_task(&database);

        let mut invalid_audit = audit_event(1, "compilation_observed", 40, None);
        invalid_audit.contract_version = "audit-event-v2".to_string();
        assert!(AuditStore::append_audit_event(&database, &invalid_audit).is_err());

        let submission = NewSubmission {
            contract_version: "submission-v2".to_string(),
            submission_id: "sub_00000000000000000000000001".to_string(),
            request_id: "request-version".to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: TASK_ID.to_string(),
            task_revision: 1,
            base_revision: None,
            fingerprint: format!("{:064x}", 41),
            payload: b"submission-v2".to_vec(),
            received_at: "2026-08-11T00:00:00Z".to_string(),
        };
        let submission_event = audit_event(
            2,
            "submission_recorded",
            41,
            Some(&submission.submission_id),
        );
        assert!(
            SubmissionStore::record_submission(&database, &submission, &submission_event).is_err()
        );

        let writer =
            AtomicProjectProjectionWriter::new(database.clone(), state_root.clone()).unwrap();
        let projection = CurrentTaskProjection {
            contract_version: "agent-projection-v2".to_string(),
            metadata: ProjectionMetadata {
                project_id: PROJECT_ID.to_string(),
                workspace_id: WORKSPACE_ID.to_string(),
                source_revision: 0,
                generation: 1,
                fingerprint: format!("{:064x}", 42),
                rendered_at: "2026-08-11T00:00:00Z".to_string(),
            },
            task_ref: None,
            serialized_contract: b"{}".to_vec(),
        };
        let projection_event = audit_event(3, "projection_rendered", 42, Some("current_task"));
        assert!(writer
            .write_current_task(&projection, &projection_event)
            .is_err());

        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            0
        );
        assert_eq!(
            SubmissionStore::submission_count(&database, PROJECT_ID).unwrap(),
            0
        );
        assert!(state_root.project_guard_dir().is_dir());
        assert!(fs::read_dir(state_root.project_guard_dir())
            .unwrap()
            .next()
            .is_none());

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn projection_writer_rejects_a_database_from_another_state_root() {
        let first = unique_temp_dir("writer-first");
        let second = unique_temp_dir("writer-second");
        let (_, database) = initialized(&first);
        let other_root = state_root(&second);

        assert!(AtomicProjectProjectionWriter::new(database, other_root).is_err());

        fs::remove_dir_all(first).expect("first temporary directory should be removed");
        fs::remove_dir_all(second).expect("second temporary directory should be removed");
    }

    #[test]
    fn projection_writer_rejects_serialized_absolute_paths_before_writing() {
        let root = unique_temp_dir("writer-public-path-gate");
        let (state_root, database) = initialized(&root);
        let writer =
            AtomicProjectProjectionWriter::new(database.clone(), state_root.clone()).unwrap();
        let projection = CurrentTaskProjection {
            contract_version: AGENT_PROJECTION_CONTRACT_VERSION.to_string(),
            metadata: ProjectionMetadata {
                project_id: PROJECT_ID.to_string(),
                workspace_id: WORKSPACE_ID.to_string(),
                source_revision: 0,
                generation: 1,
                fingerprint: format!("{:064x}", 43),
                rendered_at: "2026-08-11T00:00:00Z".to_string(),
            },
            task_ref: None,
            serialized_contract:
                br#"{"contract_version":"agent-projection-v1","objective":"\u002fhome\u002foperator\u002fprivate.rs"}"#.to_vec(),
        };
        let event = audit_event(4, "projection_rendered", 43, Some("current_task"));

        assert!(writer.write_current_task(&projection, &event).is_err());
        assert_eq!(database.event_count(PROJECT_ID).unwrap(), 0);
        assert!(state_root.project_guard_dir().is_dir());
        assert!(fs::read_dir(state_root.project_guard_dir())
            .unwrap()
            .next()
            .is_none());

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn projection_writer_rejects_a_task_other_than_the_canonical_current_task() {
        let root = unique_temp_dir("writer-current-task-binding");
        let (state_root, database) = initialized(&root);
        seed_plan_and_task(&database);
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "UPDATE projects SET active_plan_id = ?2, current_task_id = ?3 WHERE project_id = ?1",
                    AC_params![PROJECT_ID, PLAN_ID, TASK_ID],
                )?;
                Ok(())
            })
            .unwrap();
        let writer =
            AtomicProjectProjectionWriter::new(database.clone(), state_root.clone()).unwrap();
        let projection = build_current_task_projection(&CurrentTaskProjectionDocument {
            metadata: ProjectionMetadata {
                project_id: PROJECT_ID.to_string(),
                workspace_id: WORKSPACE_ID.to_string(),
                source_revision: 0,
                generation: 1,
                fingerprint: format!("{:064x}", 44),
                rendered_at: "2026-08-11T00:00:00Z".to_string(),
            },
            task: Some(ProjectedCurrentTask {
                task_id: "tsk_00000000000000000000000001".to_string(),
                task_revision: 1,
                objective: "Future task".to_string(),
                allowed_action: ProjectedTaskAction::Implement,
                acceptance_criteria: Vec::new(),
                relevant_paths: Vec::new(),
            }),
            queue_status: ProjectedQueueStatus::TaskAvailable,
            submission_instruction: "Submit evidence with hierarchy.task.current.submit."
                .to_string(),
        })
        .unwrap();
        let event = audit_event(5, "projection_rendered", 44, Some("current_task"));

        assert!(writer.write_current_task(&projection, &event).is_err());
        assert!(!state_root.todo_projection_path().exists());
        assert_eq!(database.event_count(PROJECT_ID).unwrap(), 0);
        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }
}
