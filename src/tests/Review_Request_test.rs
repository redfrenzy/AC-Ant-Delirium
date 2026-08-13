#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::libres::Rusqlite_unit::AC_params;
    use crate::structs::{
        build_assigned_review_decision, build_note_submission_request, record_authorized_review,
        record_authorized_submission, serialize_assigned_review_decision, AuditStore,
        AuthorizedReviewAssignment, AuthorizedReviewMetadata, AuthorizedSubmissionMetadata,
        CanonicalStateStore, CurrentTaskSubmissionBinding, NewAuditEvent, ProjectDatabase,
        ProjectStateRoot, ReviewDecision, StorageProject, StorageWorkspace,
        AUDIT_EVENT_CONTRACT_VERSION,
    };

    const PROJECT_ID: &str = "prj_00000000000000000000000000";
    const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";
    const PLAN_ID: &str = "pln_00000000000000000000000000";
    const TASK_ID: &str = "tsk_00000000000000000000000000";
    const SUBMISSION_ID: &str = "sub_00000000000000000000000000";
    const REVIEW_ID: &str = "rev_00000000000000000000000000";
    const REVIEWER_ID: &str = "reviewer-subject-opaque";

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/review-request-tests/{label}-{}-{nanos}",
            std::process::id(),
        ));
        fs::create_dir_all(&path).expect("temporary directory should be created");
        path
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
                    "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at) VALUES (?1, ?2, ?3, 1, 'submitted', 'pending', ?4, ?5)",
                    AC_params![TASK_ID, PROJECT_ID, PLAN_ID, b"task-v1".as_slice(), "2026-08-13T00:00:00Z"],
                )?;
                transaction.execute(
                    "UPDATE projects SET active_plan_id = ?2, current_task_id = ?3, state_revision = 9 WHERE project_id = ?1",
                    AC_params![PROJECT_ID, PLAN_ID, TASK_ID],
                )?;
                Ok(())
            })
            .expect("canonical task fixture");
        seed_submission(&database);
        database
    }

    fn seed_submission(database: &ProjectDatabase) {
        let request = build_note_submission_request(
            &CurrentTaskSubmissionBinding {
                project_id: PROJECT_ID.to_string(),
                workspace_id: WORKSPACE_ID.to_string(),
                task_id: TASK_ID.to_string(),
                task_revision: 1,
            },
            "req_00000000000000000000000000",
            "implementation evidence",
        )
        .unwrap();
        let event = audit_event(
            "evt_00000000000000000000000000",
            "service",
            "service-subject",
            "submission_recorded",
            80,
            SUBMISSION_ID,
        );
        record_authorized_submission(
            database,
            &request,
            &AuthorizedSubmissionMetadata {
                submission_id: SUBMISSION_ID.to_string(),
                fingerprint: event.fingerprint.clone(),
                received_at: event.occurred_at.clone(),
            },
            &event,
        )
        .expect("submission fixture");
    }

    fn audit_event(
        event_id: &str,
        actor_kind: &str,
        actor_subject_id: &str,
        event_kind: &str,
        fingerprint_index: usize,
        resource_id: &str,
    ) -> NewAuditEvent {
        NewAuditEvent {
            contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
            event_id: event_id.to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            actor_kind: actor_kind.to_string(),
            actor_subject_id: actor_subject_id.to_string(),
            event_kind: event_kind.to_string(),
            outcome: "succeeded".to_string(),
            occurred_at: "2026-08-13T00:00:00Z".to_string(),
            fingerprint: format!("{fingerprint_index:064x}"),
            correlation_id: "ref_00000000000000000000000000".to_string(),
            resource_id: Some(resource_id.to_string()),
            payload: format!("{event_kind} audit").into_bytes(),
        }
    }

    fn assignment() -> AuthorizedReviewAssignment {
        AuthorizedReviewAssignment {
            project_id: PROJECT_ID.to_string(),
            reviewer_id: REVIEWER_ID.to_string(),
            assigned_submission_id: SUBMISSION_ID.to_string(),
        }
    }

    fn review_metadata(event: &NewAuditEvent) -> AuthorizedReviewMetadata {
        AuthorizedReviewMetadata {
            review_id: REVIEW_ID.to_string(),
            fingerprint: event.fingerprint.clone(),
            created_at: event.occurred_at.clone(),
        }
    }

    fn review_count(database: &ProjectDatabase) -> i64 {
        database
            .with_immediate_transaction(|transaction| {
                Ok(
                    transaction.query_row("SELECT count(*) FROM reviews", [], |row| {
                        row.get::<_, i64>(0)
                    })?,
                )
            })
            .unwrap()
    }

    #[test]
    fn assigned_review_payload_contains_only_a_limited_decision() {
        let request = build_assigned_review_decision(
            "req_11111111111111111111111111",
            SUBMISSION_ID,
            ReviewDecision::Accepted,
            "Evidence and checks satisfy the current assignment.",
        )
        .unwrap();
        let payload =
            String::from_utf8(serialize_assigned_review_decision(&request).unwrap()).unwrap();

        for required in [
            "\"operation\": \"review.decide.assigned\"",
            "\"submission_id\"",
            "\"decision\": \"accepted\"",
            "\"justification\"",
        ] {
            assert!(payload.contains(required));
        }
        for forbidden in [
            "\"implemented\"",
            "\"task_reviewed\"",
            "\"review_status\"",
            "\"grade\"",
            "\"active_plan\"",
            "\"state_transition\"",
        ] {
            assert!(!payload.contains(forbidden));
        }
    }

    #[test]
    fn reviewer_cannot_decide_an_unassigned_submission() {
        let root = unique_temp_dir("scope");
        let database = initialized_database(&root);
        let request = build_assigned_review_decision(
            "req_11111111111111111111111111",
            "sub_11111111111111111111111111",
            ReviewDecision::Rejected,
            "The assigned evidence is incomplete.",
        )
        .unwrap();
        let event = audit_event(
            "evt_11111111111111111111111111",
            "reviewer",
            REVIEWER_ID,
            "review_recorded",
            81,
            REVIEW_ID,
        );

        assert!(record_authorized_review(
            &database,
            &request,
            &assignment(),
            &review_metadata(&event),
            &event,
        )
        .is_err());
        assert_eq!(review_count(&database), 0);
        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            1
        );

        fs::remove_dir_all(root).expect("temporary directory cleanup");
    }

    #[test]
    fn authorized_review_is_immutable_and_does_not_transition_the_task() {
        let root = unique_temp_dir("record");
        let database = initialized_database(&root);
        let request = build_assigned_review_decision(
            "req_11111111111111111111111111",
            SUBMISSION_ID,
            ReviewDecision::Accepted,
            "The assigned submission satisfies its evidence contract.",
        )
        .unwrap();
        let event = audit_event(
            "evt_11111111111111111111111111",
            "reviewer",
            REVIEWER_ID,
            "review_recorded",
            81,
            REVIEW_ID,
        );
        let before =
            CanonicalStateStore::load_canonical_state(&database, PROJECT_ID, WORKSPACE_ID).unwrap();

        let inserted = record_authorized_review(
            &database,
            &request,
            &assignment(),
            &review_metadata(&event),
            &event,
        )
        .unwrap();
        assert!(inserted.inserted());
        let duplicate = record_authorized_review(
            &database,
            &request,
            &assignment(),
            &review_metadata(&event),
            &event,
        )
        .unwrap();
        assert!(!duplicate.inserted());

        let after =
            CanonicalStateStore::load_canonical_state(&database, PROJECT_ID, WORKSPACE_ID).unwrap();
        assert_eq!(after.state_revision, before.state_revision);
        assert_eq!(after.active_plan_id, before.active_plan_id);
        assert_eq!(after.current_task_id, before.current_task_id);
        assert_eq!(after.tasks, before.tasks);
        assert_eq!(review_count(&database), 1);
        assert_eq!(after.last_event_sequence, before.last_event_sequence + 1);

        let conflicting = build_assigned_review_decision(
            "req_22222222222222222222222222",
            SUBMISSION_ID,
            ReviewDecision::Rejected,
            "A conflicting second decision.",
        )
        .unwrap();
        let conflict_event = audit_event(
            "evt_22222222222222222222222222",
            "reviewer",
            REVIEWER_ID,
            "review_recorded",
            82,
            "rev_22222222222222222222222222",
        );
        assert!(record_authorized_review(
            &database,
            &conflicting,
            &assignment(),
            &AuthorizedReviewMetadata {
                review_id: "rev_22222222222222222222222222".to_string(),
                fingerprint: conflict_event.fingerprint.clone(),
                created_at: conflict_event.occurred_at.clone(),
            },
            &conflict_event,
        )
        .is_err());
        assert_eq!(review_count(&database), 1);

        fs::remove_dir_all(root).expect("temporary directory cleanup");
    }

    #[test]
    fn review_and_audit_event_are_rolled_back_when_the_journal_reveals_a_path() {
        let root = unique_temp_dir("reject-private-path");
        let database = initialized_database(&root);
        let request = build_assigned_review_decision(
            "req_11111111111111111111111111",
            SUBMISSION_ID,
            ReviewDecision::Accepted,
            "The assigned submission satisfies the evidence contract.",
        )
        .unwrap();

        for payload in [
            br#"{"location":"/home/operator/private/plan/task.json"}"#.as_slice(),
            br#"{"location":"\u002fhome\u002foperator\u002fprivate\u002fplan"}"#.as_slice(),
        ] {
            let mut event = audit_event(
                "evt_11111111111111111111111111",
                "reviewer",
                REVIEWER_ID,
                "review_recorded",
                81,
                REVIEW_ID,
            );
            event.payload = payload.to_vec();
            assert!(record_authorized_review(
                &database,
                &request,
                &assignment(),
                &review_metadata(&event),
                &event,
            )
            .is_err());
        }

        assert_eq!(review_count(&database), 0);
        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            1
        );
        fs::remove_dir_all(root).expect("temporary directory cleanup");
    }
}
