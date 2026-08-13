#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::libres::Rusqlite_unit::AC_params;
    use crate::structs::{
        build_current_task_projection, build_note_submission_request,
        parse_current_task_submission_binding, record_authorized_submission,
        serialize_submission_request, AuditStore, AuthorizedSubmissionMetadata,
        CanonicalStateStore, CurrentTaskProjectionDocument, NewAuditEvent, ProjectDatabase,
        ProjectStateRoot, ProjectedCurrentTask, ProjectedQueueStatus, ProjectedTaskAction,
        ProjectionMetadata, StorageProject, StorageWorkspace, SubmissionStore,
        AUDIT_EVENT_CONTRACT_VERSION,
    };

    const PROJECT_ID: &str = "prj_00000000000000000000000000";
    const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";
    const PLAN_ID: &str = "pln_00000000000000000000000000";
    const TASK_ID: &str = "tsk_00000000000000000000000000";
    const REQUEST_ID: &str = "req_00000000000000000000000000";
    const SUBMISSION_ID: &str = "sub_00000000000000000000000000";

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/submission-request-tests/{label}-{}-{nanos}",
            std::process::id(),
        ));
        fs::create_dir_all(&path).expect("temporary directory should be created");
        path
    }

    fn task_projection() -> Vec<u8> {
        build_current_task_projection(&CurrentTaskProjectionDocument {
            metadata: ProjectionMetadata {
                project_id: PROJECT_ID.to_string(),
                workspace_id: WORKSPACE_ID.to_string(),
                source_revision: 7,
                generation: 1,
                fingerprint: format!("{:064x}", 70),
                rendered_at: "2026-08-13T00:00:00Z".to_string(),
            },
            task: Some(ProjectedCurrentTask {
                task_id: TASK_ID.to_string(),
                task_revision: 3,
                objective: "Implement the current task.".to_string(),
                allowed_action: ProjectedTaskAction::Implement,
                acceptance_criteria: vec!["Tests pass.".to_string()],
                relevant_paths: vec!["src/lib.rs".to_string()],
            }),
            queue_status: ProjectedQueueStatus::TaskAvailable,
            submission_instruction: "Submit evidence with hierarchy.task.current.submit."
                .to_string(),
        })
        .expect("projection should be valid")
        .serialized_contract
    }

    fn initialized_database(root: &Path) -> ProjectDatabase {
        let state_root = ProjectStateRoot::with_default_config(
            root.join("protected/project"),
            root.join("source-project"),
            root.join("source-worktree"),
        );
        let database = ProjectDatabase::initialize(&state_root).expect("database initialization");
        CanonicalStateStore::register_project(
            &database,
            &StorageProject {
                project_id: PROJECT_ID.to_string(),
                project_key: "project-key-v1".to_string(),
                created_at: "2026-08-13T00:00:00Z".to_string(),
            },
        )
        .expect("project registration");
        CanonicalStateStore::register_workspace(
            &database,
            &StorageWorkspace {
                workspace_id: WORKSPACE_ID.to_string(),
                project_id: PROJECT_ID.to_string(),
                workspace_key: "workspace-key-v1".to_string(),
                created_at: "2026-08-13T00:00:00Z".to_string(),
            },
        )
        .expect("workspace registration");
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO plans(plan_id, project_id, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
                    AC_params![PLAN_ID, PROJECT_ID, b"plan-v1".as_slice(), "2026-08-13T00:00:00Z"],
                )?;
                transaction.execute(
                    "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at) VALUES (?1, ?2, ?3, 3, 'available', 'not_required', ?4, ?5)",
                    AC_params![TASK_ID, PROJECT_ID, PLAN_ID, b"task-v1".as_slice(), "2026-08-13T00:00:00Z"],
                )?;
                transaction.execute(
                    "UPDATE projects SET active_plan_id = ?2, current_task_id = ?3, state_revision = 7 WHERE project_id = ?1",
                    AC_params![PROJECT_ID, PLAN_ID, TASK_ID],
                )?;
                Ok(())
            })
            .expect("canonical task fixture");
        database
    }

    fn audit_event() -> NewAuditEvent {
        NewAuditEvent {
            contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
            event_id: "evt_00000000000000000000000000".to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            actor_kind: "service".to_string(),
            actor_subject_id: "authorized-agent-capability".to_string(),
            event_kind: "submission_recorded".to_string(),
            outcome: "succeeded".to_string(),
            occurred_at: "2026-08-13T00:00:00Z".to_string(),
            fingerprint: format!("{:064x}", 71),
            correlation_id: "ref_00000000000000000000000000".to_string(),
            resource_id: Some(SUBMISSION_ID.to_string()),
            payload: b"submission recorded".to_vec(),
        }
    }

    #[test]
    fn projection_binding_builds_only_a_submission_v1_note_request() {
        let binding = parse_current_task_submission_binding(&task_projection())
            .expect("binding should come from the public projection");
        let request = build_note_submission_request(
            &binding,
            REQUEST_ID,
            "cargo check and focused tests passed",
        )
        .expect("submission request should be valid");
        let payload = String::from_utf8(serialize_submission_request(&request).unwrap()).unwrap();

        assert_eq!(binding.project_id, PROJECT_ID);
        assert_eq!(binding.workspace_id, WORKSPACE_ID);
        assert_eq!(binding.task_id, TASK_ID);
        assert_eq!(binding.task_revision, 3);
        for required in [
            "\"contract_version\": \"submission-v1\"",
            "\"request_id\"",
            "\"project_id\"",
            "\"workspace_id\"",
            "\"task_id\"",
            "\"task_revision\"",
            "\"kind\": \"note\"",
            "\"evidence\"",
        ] {
            assert!(payload.contains(required));
        }
        for forbidden in [
            "\"implemented\"",
            "\"task_reviewed\"",
            "\"review_status\"",
            "\"grade\"",
            "\"active_plan\"",
            "\"approval\"",
            "\"state_root\"",
        ] {
            assert!(!payload.contains(forbidden));
        }
    }

    #[test]
    fn missing_or_ambiguous_task_binding_is_rejected() {
        let no_task = task_projection()
            .windows(TASK_ID.len())
            .position(|window| window == TASK_ID.as_bytes())
            .map(|start| {
                let mut projection = task_projection();
                projection[start..start + TASK_ID.len()].fill(b'x');
                projection
            })
            .expect("task id marker");
        assert!(parse_current_task_submission_binding(&no_task).is_err());

        let mut duplicate = String::from_utf8(task_projection()).unwrap();
        duplicate.insert_str(
            duplicate.len() - 2,
            ",\n  \"task_id\": \"tsk_11111111111111111111111111\"",
        );
        assert!(parse_current_task_submission_binding(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn authorized_submission_is_immutable_and_does_not_transition_canonical_state() {
        let root = unique_temp_dir("record");
        let database = initialized_database(&root);
        let binding = parse_current_task_submission_binding(&task_projection()).unwrap();
        let request = build_note_submission_request(
            &binding,
            REQUEST_ID,
            "implemented=true active_plan=changed remains evidence text only",
        )
        .unwrap();
        let event = audit_event();
        let metadata = AuthorizedSubmissionMetadata {
            submission_id: SUBMISSION_ID.to_string(),
            fingerprint: event.fingerprint.clone(),
            received_at: event.occurred_at.clone(),
        };
        let before =
            CanonicalStateStore::load_canonical_state(&database, PROJECT_ID, WORKSPACE_ID).unwrap();

        let inserted =
            record_authorized_submission(&database, &request, &metadata, &event).unwrap();
        assert!(inserted.inserted());
        let duplicate =
            record_authorized_submission(&database, &request, &metadata, &event).unwrap();
        assert!(!duplicate.inserted());

        let after =
            CanonicalStateStore::load_canonical_state(&database, PROJECT_ID, WORKSPACE_ID).unwrap();
        assert_eq!(after.state_revision, before.state_revision);
        assert_eq!(after.active_plan_id, before.active_plan_id);
        assert_eq!(after.current_task_id, before.current_task_id);
        assert_eq!(after.tasks, before.tasks);
        assert_eq!(
            SubmissionStore::submission_count(&database, PROJECT_ID).unwrap(),
            1
        );
        assert_eq!(after.last_event_sequence, before.last_event_sequence + 1);

        fs::remove_dir_all(root).expect("temporary directory cleanup");
    }

    #[test]
    fn submission_and_audit_event_are_rolled_back_when_the_journal_reveals_a_path() {
        let root = unique_temp_dir("reject-private-path");
        let database = initialized_database(&root);
        let binding = parse_current_task_submission_binding(&task_projection()).unwrap();
        let request =
            build_note_submission_request(&binding, REQUEST_ID, "valid evidence").unwrap();
        let metadata = AuthorizedSubmissionMetadata {
            submission_id: SUBMISSION_ID.to_string(),
            fingerprint: format!("{:064x}", 71),
            received_at: "2026-08-13T00:00:00Z".to_string(),
        };

        for payload in [
            br#"{"location":"/home/operator/private/plan/task.json"}"#.as_slice(),
            br#"{"location":"\u002fhome\u002foperator\u002fprivate\u002fplan"}"#.as_slice(),
        ] {
            let mut event = audit_event();
            event.payload = payload.to_vec();
            assert!(record_authorized_submission(&database, &request, &metadata, &event).is_err());
        }

        assert_eq!(
            SubmissionStore::submission_count(&database, PROJECT_ID).unwrap(),
            0
        );
        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            0
        );
        fs::remove_dir_all(root).expect("temporary directory cleanup");
    }
}
