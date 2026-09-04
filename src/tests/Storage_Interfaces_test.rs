#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use crate::libres::Rusqlite_unit::AC_params;
    use crate::structs::{
        build_current_task_projection, materialize_limited_project_projections,
        AtomicProjectProjectionWriter, AuditStore, BackupCheckpointRecord, BackupCheckpointStore,
        CanonicalGovernanceSnapshotRequest, CanonicalStateStore, CanonicalStateTransition,
        CanonicalTaskAdvance, CanonicalTaskPackage, CanonicalTaskPackageTask,
        CurrentTaskProjection, CurrentTaskProjectionDocument, EventWriteOutcome,
        GovernanceErrorsProjection, GovernanceErrorsRefreshRequest, NewAuditEvent, NewReview,
        NewSubmission, ProjectDatabase, ProjectProjectionWriter, ProjectStateRoot,
        ProjectedCurrentTask, ProjectedGovernanceViolation, ProjectedQueueStatus,
        ProjectedTaskAction, ProjectionArtifactKind, ProjectionMetadata, ReviewDecision,
        ReviewStore, StorageProject, StorageWorkspace, SubmissionStore, TransactionalStorageError,
        AGENT_PROJECTION_CONTRACT_VERSION, AUDIT_EVENT_CONTRACT_VERSION,
        CANONICAL_STATE_CONTRACT_VERSION, GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION,
        STORAGE_SCHEMA_VERSION, SUBMISSION_CONTRACT_VERSION,
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
        database
            .initialize_empty_governance_snapshot(PROJECT_ID, WORKSPACE_ID, "2026-08-11T00:00:00Z")
            .unwrap();
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

    fn task_package() -> CanonicalTaskPackage {
        CanonicalTaskPackage {
            contract_version: GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION.to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            plan_id: PLAN_ID.to_string(),
            expected_state_revision: 0,
            tasks: vec![
                CanonicalTaskPackageTask {
                    task_id: TASK_ID.to_string(),
                    task_revision: 1,
                    objective: "Implement the synthetic governance task.".to_string(),
                    acceptance_criteria: vec!["The focused test passes.".to_string()],
                    relevant_paths: vec!["src/lib.rs".to_string()],
                },
                CanonicalTaskPackageTask {
                    task_id: "tsk_00000000000000000000000001".to_string(),
                    task_revision: 1,
                    objective: "Implement the next synthetic task.".to_string(),
                    acceptance_criteria: vec!["The next-action projection advances.".to_string()],
                    relevant_paths: vec!["src/main.rs".to_string()],
                },
            ],
        }
    }

    fn task_package_event(index: usize, fingerprint_index: usize) -> NewAuditEvent {
        let mut event = audit_event(
            index,
            "state_transitioned",
            fingerprint_index,
            Some(PLAN_ID),
        );
        event.actor_kind = "operator".to_string();
        event.actor_subject_id = "authorized-operator".to_string();
        event.payload = b"governance task package installed".to_vec();
        event
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
    fn task_package_initializes_an_empty_project_atomically_and_idempotently() {
        let root = unique_temp_dir("task-package");
        let (_, database) = initialized(&root);
        let package = task_package();
        let event = task_package_event(1, 12);

        let installed =
            CanonicalStateStore::install_task_package(&database, &package, &event).unwrap();
        assert_eq!(installed.audit, EventWriteOutcome::Inserted { sequence: 1 });
        assert_eq!(installed.state.state_revision, 1);
        assert_eq!(installed.state.active_plan_id.as_deref(), Some(PLAN_ID));
        assert_eq!(installed.state.current_task_id.as_deref(), Some(TASK_ID));
        assert_eq!(installed.state.tasks.len(), 2);
        let ordinals = database
            .with_immediate_transaction(|transaction| {
                Ok((
                    transaction.query_row(
                        "SELECT task_ordinal FROM tasks WHERE task_id = ?1",
                        AC_params![TASK_ID],
                        |row| row.get::<_, i64>(0),
                    )?,
                    transaction.query_row(
                        "SELECT task_ordinal FROM tasks WHERE task_id = ?1",
                        AC_params!["tsk_00000000000000000000000001"],
                        |row| row.get::<_, i64>(0),
                    )?,
                ))
            })
            .unwrap();
        assert_eq!(ordinals, (0, 1));

        let projection = database
            .render_current_task_projection(PROJECT_ID, WORKSPACE_ID, "2026-08-11T00:00:01Z")
            .unwrap();
        let projection = String::from_utf8(projection).unwrap();
        assert!(projection.contains("Implement the synthetic governance task."));
        assert!(projection.contains("src/lib.rs"));
        assert!(!projection.contains("Implement the next synthetic task."));
        let governance = database
            .render_governance_errors_projection(PROJECT_ID, WORKSPACE_ID, "2026-08-11T00:00:01Z")
            .unwrap();
        let governance: serde_json::Value = serde_json::from_slice(&governance).unwrap();
        assert_eq!(governance["source_revision"], 1);
        assert_eq!(governance["violations"], serde_json::json!([]));

        let replay =
            CanonicalStateStore::install_task_package(&database, &package, &event).unwrap();
        assert_eq!(replay.audit, EventWriteOutcome::Duplicate { sequence: 1 });
        assert_eq!(replay.state, installed.state);
        assert_eq!(database.event_count(PROJECT_ID).unwrap(), 1);

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn task_package_refuses_overwrite_invalid_paths_and_partial_persistence() {
        let root = unique_temp_dir("task-package-rejection");
        let (_, database) = initialized(&root);

        let mut invalid = task_package();
        invalid.tasks[0].relevant_paths = vec!["../private-plan.json".to_string()];
        assert!(CanonicalStateStore::install_task_package(
            &database,
            &invalid,
            &task_package_event(1, 13),
        )
        .is_err());
        let empty = database
            .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
            .unwrap();
        assert_eq!(empty.state_revision, 0);
        assert!(empty.tasks.is_empty());
        assert_eq!(empty.last_event_sequence, 0);
        let governance = database
            .render_governance_errors_projection(PROJECT_ID, WORKSPACE_ID, "2026-08-11T00:00:01Z")
            .unwrap();
        let governance: serde_json::Value = serde_json::from_slice(&governance).unwrap();
        assert_eq!(governance["source_revision"], 0);

        let package = task_package();
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "DELETE FROM governance_snapshots WHERE project_id = ?1 AND workspace_id = ?2",
                    AC_params![PROJECT_ID, WORKSPACE_ID],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(CanonicalStateStore::install_task_package(
            &database,
            &package,
            &task_package_event(2, 14),
        )
        .is_err());
        let rolled_back = database
            .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
            .unwrap();
        assert_eq!(rolled_back.state_revision, 0);
        assert!(rolled_back.tasks.is_empty());
        assert_eq!(rolled_back.last_event_sequence, 0);
        database
            .initialize_empty_governance_snapshot(PROJECT_ID, WORKSPACE_ID, "2026-08-11T00:00:02Z")
            .unwrap();

        CanonicalStateStore::install_task_package(&database, &package, &task_package_event(2, 14))
            .unwrap();
        let conflicting = task_package_event(3, 15);
        assert!(
            CanonicalStateStore::install_task_package(&database, &package, &conflicting).is_err()
        );
        let state = database
            .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
            .unwrap();
        assert_eq!(state.state_revision, 1);
        assert_eq!(state.tasks.len(), 2);
        assert_eq!(state.last_event_sequence, 1);

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn accepted_review_advances_by_package_order_and_records_history_atomically() {
        let root = unique_temp_dir("accepted-advance");
        let (_, database) = initialized(&root);
        let package = task_package();
        CanonicalStateStore::install_task_package(&database, &package, &task_package_event(1, 16))
            .unwrap();

        let submission = NewSubmission {
            contract_version: SUBMISSION_CONTRACT_VERSION.to_string(),
            submission_id: "sub_00000000000000000000000000".to_string(),
            request_id: "request-accepted-advance".to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: TASK_ID.to_string(),
            task_revision: 1,
            base_revision: None,
            fingerprint: format!("{:064x}", 17),
            payload: b"accepted advance submission".to_vec(),
            received_at: "2026-08-11T00:00:01Z".to_string(),
        };
        let submission_event = audit_event(
            2,
            "submission_recorded",
            17,
            Some(&submission.submission_id),
        );
        SubmissionStore::record_submission(&database, &submission, &submission_event).unwrap();

        let review = NewReview {
            review_id: "rev_00000000000000000000000000".to_string(),
            project_id: PROJECT_ID.to_string(),
            submission_id: submission.submission_id.clone(),
            reviewer_id: "authorized-reviewer".to_string(),
            decision: ReviewDecision::Accepted,
            fingerprint: format!("{:064x}", 18),
            payload: b"accepted assigned review".to_vec(),
            created_at: "2026-08-11T00:00:02Z".to_string(),
        };
        let mut review_event = audit_event(3, "review_recorded", 18, Some(&review.review_id));
        review_event.actor_kind = "reviewer".to_string();
        review_event.actor_subject_id = review.reviewer_id.clone();
        ReviewStore::record_review(&database, &review, &review_event).unwrap();

        let advance = CanonicalTaskAdvance {
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: TASK_ID.to_string(),
            review_id: review.review_id.clone(),
            expected_state_revision: 1,
        };
        let mut advance_event = audit_event(4, "state_transitioned", 19, Some(TASK_ID));
        advance_event.actor_kind = "ci".to_string();
        advance_event.actor_subject_id = "authorized-state-transition".to_string();
        let invalid_review_id = "rev_99999999999999999999999999";
        let invalid_advance = CanonicalTaskAdvance {
            review_id: invalid_review_id.to_string(),
            ..advance.clone()
        };
        let invalid_checkpoint = BackupCheckpointRecord {
            transition_kind: "advance".to_string(),
            transition_id: invalid_review_id.to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: Some(TASK_ID.to_string()),
            task_revision: Some(1),
            state_revision: 2,
            commit_id: "a".repeat(40),
            parent_commit_id: Some("b".repeat(40)),
            source_fingerprint: "c".repeat(64),
            entry_count: 2,
            byte_count: 41,
            recorded_at: "unix:1788000000".to_string(),
        };
        assert!(CanonicalStateStore::advance_accepted_task_with_checkpoint(
            &database,
            &invalid_advance,
            &advance_event,
            &invalid_checkpoint,
        )
        .is_err());
        assert!(database
            .backup_checkpoint(PROJECT_ID, "advance", invalid_review_id)
            .unwrap()
            .is_none());
        assert_eq!(database.event_count(PROJECT_ID).unwrap(), 3);

        let checkpoint = BackupCheckpointRecord {
            transition_id: review.review_id.clone(),
            commit_id: "d".repeat(40),
            ..invalid_checkpoint
        };
        let outcome = CanonicalStateStore::advance_accepted_task_with_checkpoint(
            &database,
            &advance,
            &advance_event,
            &checkpoint,
        )
        .unwrap();
        assert_eq!(outcome.audit, EventWriteOutcome::Inserted { sequence: 4 });
        assert_eq!(outcome.state.state_revision, 2);
        assert_eq!(outcome.completed_task_id, TASK_ID);
        assert_eq!(
            outcome.next_task_id.as_deref(),
            Some("tsk_00000000000000000000000001")
        );
        assert_eq!(outcome.state.current_task_id, outcome.next_task_id);
        let completed = outcome
            .state
            .tasks
            .iter()
            .find(|task| task.task_id == TASK_ID)
            .unwrap();
        assert_eq!(completed.status, "completed");
        assert_eq!(completed.review_status, "accepted");
        assert_eq!(outcome.state.last_event_sequence, 4);

        let governance = database
            .render_governance_errors_projection(PROJECT_ID, WORKSPACE_ID, "2026-08-11T00:00:03Z")
            .unwrap();
        let governance: serde_json::Value = serde_json::from_slice(&governance).unwrap();
        assert_eq!(governance["source_revision"], 2);

        assert_eq!(
            database
                .backup_checkpoint(PROJECT_ID, "advance", &review.review_id)
                .unwrap(),
            Some(checkpoint.clone())
        );
        let replay = CanonicalStateStore::advance_accepted_task_with_checkpoint(
            &database,
            &advance,
            &advance_event,
            &checkpoint,
        )
        .unwrap();
        assert_eq!(replay.audit, EventWriteOutcome::Duplicate { sequence: 4 });
        assert_eq!(replay.state, outcome.state);

        let final_task_id = "tsk_00000000000000000000000001";
        let final_submission = NewSubmission {
            contract_version: SUBMISSION_CONTRACT_VERSION.to_string(),
            submission_id: "sub_00000000000000000000000001".to_string(),
            request_id: "request-final-advance".to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: final_task_id.to_string(),
            task_revision: 1,
            base_revision: None,
            fingerprint: format!("{:064x}", 20),
            payload: b"final advance submission".to_vec(),
            received_at: "2026-08-11T00:00:04Z".to_string(),
        };
        let final_submission_event = audit_event(
            5,
            "submission_recorded",
            20,
            Some(&final_submission.submission_id),
        );
        SubmissionStore::record_submission(&database, &final_submission, &final_submission_event)
            .unwrap();
        let final_review = NewReview {
            review_id: "rev_00000000000000000000000001".to_string(),
            project_id: PROJECT_ID.to_string(),
            submission_id: final_submission.submission_id.clone(),
            reviewer_id: "authorized-reviewer".to_string(),
            decision: ReviewDecision::Accepted,
            fingerprint: format!("{:064x}", 21),
            payload: b"accepted final review".to_vec(),
            created_at: "2026-08-11T00:00:05Z".to_string(),
        };
        let mut final_review_event =
            audit_event(6, "review_recorded", 21, Some(&final_review.review_id));
        final_review_event.actor_kind = "reviewer".to_string();
        final_review_event.actor_subject_id = final_review.reviewer_id.clone();
        ReviewStore::record_review(&database, &final_review, &final_review_event).unwrap();
        let final_advance = CanonicalTaskAdvance {
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: final_task_id.to_string(),
            review_id: final_review.review_id,
            expected_state_revision: 2,
        };
        let mut final_advance_event = audit_event(7, "state_transitioned", 22, Some(final_task_id));
        final_advance_event.actor_kind = "ci".to_string();
        final_advance_event.actor_subject_id = "authorized-state-transition".to_string();
        let final_outcome = CanonicalStateStore::advance_accepted_task(
            &database,
            &final_advance,
            &final_advance_event,
        )
        .unwrap();
        assert_eq!(final_outcome.state.state_revision, 3);
        assert_eq!(final_outcome.completed_task_id, final_task_id);
        assert_eq!(final_outcome.next_task_id, None);
        assert_eq!(final_outcome.state.current_task_id, None);
        assert_eq!(final_outcome.state.active_plan_id, None);
        assert_eq!(final_outcome.state.last_event_sequence, 7);
        let plan_status = database
            .with_immediate_transaction(|transaction| {
                Ok(transaction.query_row(
                    "SELECT status FROM plans WHERE plan_id = ?1",
                    AC_params![PLAN_ID],
                    |row| row.get::<_, String>(0),
                )?)
            })
            .unwrap();
        assert_eq!(plan_status, "completed");
        let governance = database
            .render_governance_errors_projection(PROJECT_ID, WORKSPACE_ID, "2026-08-11T00:00:06Z")
            .unwrap();
        let governance: serde_json::Value = serde_json::from_slice(&governance).unwrap();
        assert_eq!(governance["source_revision"], 3);

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
        let checkpoint = BackupCheckpointRecord {
            transition_kind: "submission".to_string(),
            transition_id: submission.submission_id.clone(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: Some(TASK_ID.to_string()),
            task_revision: Some(1),
            state_revision: 0,
            commit_id: "a".repeat(40),
            parent_commit_id: Some("b".repeat(40)),
            source_fingerprint: "c".repeat(64),
            entry_count: 2,
            byte_count: 41,
            recorded_at: "unix:1788000000".to_string(),
        };

        let inserted = SubmissionStore::record_submission_with_checkpoint(
            &database,
            &submission,
            &event,
            &checkpoint,
        )
        .unwrap();
        assert!(inserted.inserted());
        assert_eq!(inserted.event_sequence(), 1);
        assert_eq!(
            database
                .backup_checkpoint(PROJECT_ID, "submission", &submission.submission_id)
                .unwrap(),
            Some(checkpoint.clone())
        );
        let duplicate = SubmissionStore::record_submission_with_checkpoint(
            &database,
            &submission,
            &event,
            &checkpoint,
        )
        .unwrap();
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
            snapshot_fingerprint: None,
            serialized_contract: b"{\"contract_version\":\"agent-projection-v1\",\"artifact_kind\":\"governance_errors\"}".to_vec(),
        };
        let errors_event = audit_event(2, "projection_rendered", 31, Some("governance_errors"));
        assert!(writer
            .write_governance_errors(&errors, &errors_event)
            .is_err());
        writer
            .replace_canonical_governance_snapshot(&CanonicalGovernanceSnapshotRequest {
                project_id: PROJECT_ID.to_string(),
                workspace_id: WORKSPACE_ID.to_string(),
                observed_at: "2026-08-11T00:00:00Z".to_string(),
                violations: vec![ProjectedGovernanceViolation {
                    violation_id: "vio_00000000000000000000000000".to_string(),
                    rule_id: "hierarchy.rule.max_nesting".to_string(),
                    summary: "Maximum nesting exceeded.".to_string(),
                    path: "src/lib.rs".to_string(),
                    line: 1,
                    quick_fix: None,
                    public_rule_reference: "hierarchy.rule.max_nesting".to_string(),
                }],
            })
            .unwrap();
        writer
            .refresh_governance_errors_projection(&GovernanceErrorsRefreshRequest {
                project_id: PROJECT_ID.to_string(),
                workspace_id: WORKSPACE_ID.to_string(),
                rendered_at: "2026-08-11T00:00:01Z".to_string(),
            })
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
        let governance = fs::read_to_string(writer.governance_errors_path()).unwrap();
        assert!(governance.contains("\"artifact_kind\": \"governance_errors\""));
        assert!(governance.contains("\"path\": \"src/lib.rs\""));
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
        assert_eq!(state.last_event_sequence, 3);

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
                assert_eq!(
                    result,
                    (
                        i64::from(STORAGE_SCHEMA_VERSION),
                        i64::from(STORAGE_SCHEMA_VERSION)
                    )
                );
                Ok(())
            })
            .unwrap();

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn limited_projection_delivery_writes_only_valid_fixed_artifacts_and_deduplicates_bytes() {
        let root = unique_temp_dir("limited-projection-delivery");
        let current = br#"{
  "contract_version": "agent-projection-v1",
  "artifact_kind": "current_task",
  "project_ref": "prj_00000000000000000000000000",
  "workspace_ref": "wsp_00000000000000000000000000",
  "source_revision": 0,
  "task": null,
  "queue_status": "no_task_available",
  "submission": {"contract_version":"submission-v1","instruction":"Submit evidence with hierarchy.task.current.submit."}
}
"#;
        let governance = br#"{
  "contract_version": "agent-projection-v1",
  "artifact_kind": "governance_errors",
  "project_ref": "prj_00000000000000000000000000",
  "workspace_ref": "wsp_00000000000000000000000000",
  "source_revision": 0,
  "violations": []
}
"#;

        materialize_limited_project_projections(&root, current, governance).unwrap();
        let todo = root.join("guard/to-do_list.json");
        let errors = root.join("guard/governance_error.json");
        let first = fs::metadata(&todo).unwrap();
        materialize_limited_project_projections(&root, current, governance).unwrap();
        let second = fs::metadata(&todo).unwrap();

        assert_eq!(fs::read(&todo).unwrap(), current);
        assert_eq!(fs::read(&errors).unwrap(), governance);
        #[cfg(unix)]
        {
            assert_eq!(first.ino(), second.ino());
            assert_eq!(second.permissions().mode() & 0o777, 0o600);
        }
        let swapped = materialize_limited_project_projections(&root, governance, current);
        assert!(swapped.is_err());
        assert_eq!(fs::read(&todo).unwrap(), current);

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
    fn projection_writer_rejects_administrative_fields_and_future_task_collections() {
        let root = unique_temp_dir("writer-administrative-projection-gate");
        let (state_root, database) = initialized(&root);
        let writer =
            AtomicProjectProjectionWriter::new(database.clone(), state_root.clone()).unwrap();

        for (offset, serialized_contract) in [
            br#"{"contract_version":"agent-projection-v1","tasks":[{"objective":"future"}]}"#
                .as_slice(),
            br#"{"contract_version":"agent-projection-v1","configuration":{"enabled":false}}"#
                .as_slice(),
            br#"{"contract_version":"agent-projection-v1","objective":"read task_orchestrator.json"}"#
                .as_slice(),
            br#"{"contract_version":"agent-projection-v1","path":"plan/private.json"}"#
                .as_slice(),
            br#"{tasks:[{"objective":"future"}]}"#.as_slice(),
        ]
        .iter()
        .enumerate()
        {
            let fingerprint = format!("{:064x}", 50 + offset);
            let projection = CurrentTaskProjection {
                contract_version: AGENT_PROJECTION_CONTRACT_VERSION.to_string(),
                metadata: ProjectionMetadata {
                    project_id: PROJECT_ID.to_string(),
                    workspace_id: WORKSPACE_ID.to_string(),
                    source_revision: 0,
                    generation: 1,
                    fingerprint: fingerprint.clone(),
                    rendered_at: "2026-08-11T00:00:00Z".to_string(),
                },
                task_ref: None,
                serialized_contract: serialized_contract.to_vec(),
            };
            let event = audit_event(
                10 + offset,
                "projection_rendered",
                50 + offset,
                Some("current_task"),
            );
            assert!(writer.write_current_task(&projection, &event).is_err());
        }

        assert_eq!(database.event_count(PROJECT_ID).unwrap(), 0);
        assert!(!state_root.todo_projection_path().exists());
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

    #[test]
    fn backup_checkpoint_binding_is_append_only_idempotent_and_queryable() {
        let root = unique_temp_dir("backup-checkpoint-binding");
        let (_state_root, database) = initialized(&root);
        seed_plan_and_task(&database);
        let checkpoint = BackupCheckpointRecord {
            transition_kind: "submission".to_string(),
            transition_id: "sub_00000000000000000000000000".to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: Some(TASK_ID.to_string()),
            task_revision: Some(1),
            state_revision: 1,
            commit_id: "a".repeat(40),
            parent_commit_id: Some("b".repeat(40)),
            source_fingerprint: "c".repeat(64),
            entry_count: 2,
            byte_count: 41,
            recorded_at: "unix:1788000000".to_string(),
        };

        assert!(database.record_backup_checkpoint(&checkpoint).unwrap());
        assert_eq!(
            database
                .backup_checkpoint(PROJECT_ID, "submission", "sub_00000000000000000000000000",)
                .unwrap(),
            Some(checkpoint.clone()),
        );
        let mut replay = checkpoint.clone();
        replay.recorded_at = "unix:1788000001".to_string();
        assert!(!database.record_backup_checkpoint(&replay).unwrap());

        let mut conflict = checkpoint;
        conflict.commit_id = "d".repeat(40);
        assert!(matches!(
            database.record_backup_checkpoint(&conflict),
            Err(TransactionalStorageError::Integrity(_)),
        ));
        assert!(database
            .backup_checkpoint(PROJECT_ID, "review", "sub_00000000000000000000000000")
            .is_err());
        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }
}
