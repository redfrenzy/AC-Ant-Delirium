#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;

    use crate::libres::Rusqlite_unit::AC_params;
    use crate::structs::{
        encode_canonical_task_projection_payload, parse_current_task_submission_binding,
        AtomicProjectProjectionWriter, AuditStore, CanonicalStateStore,
        CanonicalTaskProjectionPayload, CurrentTaskProjectionRefreshRequest, EventWriteOutcome,
        ProjectDatabase, ProjectStateRoot, ProjectedTaskAction, StorageProject, StorageWorkspace,
        CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION,
    };

    const PROJECT_ID: &str = "prj_00000000000000000000000000";
    const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";
    const PLAN_ID: &str = "pln_00000000000000000000000000";
    const OTHER_PLAN_ID: &str = "pln_00000000000000000000000001";
    const CURRENT_TASK_ID: &str = "tsk_00000000000000000000000000";
    const FUTURE_TASK_ID: &str = "tsk_00000000000000000000000001";

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture(label: &str) -> (PathBuf, ProjectStateRoot, ProjectDatabase) {
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/current-task-refresh-tests/{label}-{}-{sequence}",
            std::process::id()
        ));
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).expect("project fixture");
        let state_root = ProjectStateRoot::with_default_config(
            root.join("protected"),
            project_root.clone(),
            project_root,
        );
        let database = ProjectDatabase::initialize(&state_root).expect("database fixture");
        CanonicalStateStore::register_project(
            &database,
            &StorageProject {
                project_id: PROJECT_ID.to_string(),
                project_key: "current-task-refresh-project".to_string(),
                created_at: "2026-08-13T00:00:00Z".to_string(),
            },
        )
        .expect("project registration");
        CanonicalStateStore::register_workspace(
            &database,
            &StorageWorkspace {
                workspace_id: WORKSPACE_ID.to_string(),
                project_id: PROJECT_ID.to_string(),
                workspace_key: "current-task-refresh-workspace".to_string(),
                created_at: "2026-08-13T00:00:00Z".to_string(),
            },
        )
        .expect("workspace registration");
        (root, state_root, database)
    }

    fn task_payload(
        objective: &str,
        action: ProjectedTaskAction,
        criteria: &[&str],
        paths: &[&str],
    ) -> Vec<u8> {
        encode_canonical_task_projection_payload(&CanonicalTaskProjectionPayload {
            contract_version: CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION.to_string(),
            objective: objective.to_string(),
            allowed_action: action,
            acceptance_criteria: criteria.iter().map(|value| (*value).to_string()).collect(),
            relevant_paths: paths.iter().map(|value| (*value).to_string()).collect(),
        })
        .expect("canonical task payload")
    }

    fn insert_plan(database: &ProjectDatabase, plan_id: &str, payload: &[u8]) {
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO plans(plan_id, project_id, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
                    AC_params![plan_id, PROJECT_ID, payload, "2026-08-13T00:00:00Z"],
                )?;
                Ok(())
            })
            .expect("plan fixture");
    }

    fn insert_task(
        database: &ProjectDatabase,
        task_id: &str,
        plan_id: &str,
        revision: i64,
        status: &str,
        review_status: &str,
        payload: &[u8],
    ) {
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    AC_params![task_id, PROJECT_ID, plan_id, revision, status, review_status, payload, "2026-08-13T00:00:00Z"],
                )?;
                Ok(())
            })
            .expect("task fixture");
    }

    fn select_current_task(database: &ProjectDatabase, task_id: Option<&str>) {
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "UPDATE projects SET active_plan_id = ?2, current_task_id = ?3, state_revision = state_revision + 1 WHERE project_id = ?1",
                    AC_params![PROJECT_ID, PLAN_ID, task_id],
                )?;
                Ok(())
            })
            .expect("current task fixture");
    }

    fn refresh_request() -> CurrentTaskProjectionRefreshRequest {
        CurrentTaskProjectionRefreshRequest {
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            rendered_at: "2026-08-13T12:00:00Z".to_string(),
        }
    }

    fn assert_no_projection_side_effects(
        state_root: &ProjectStateRoot,
        database: &ProjectDatabase,
    ) {
        assert!(!state_root.todo_projection_path().exists());
        assert_eq!(
            AuditStore::audit_event_count(database, PROJECT_ID).unwrap(),
            0
        );
        let snapshot =
            CanonicalStateStore::load_canonical_state(database, PROJECT_ID, WORKSPACE_ID).unwrap();
        assert_eq!(snapshot.projection_generation, 0);
    }

    #[test]
    fn refresh_projects_only_the_canonical_current_task_and_public_fields() {
        let (root, state_root, database) = fixture("current-only");
        insert_plan(
            &database,
            PLAN_ID,
            b"private plan payload names plan/active.json and future work",
        );
        insert_task(
            &database,
            CURRENT_TASK_ID,
            PLAN_ID,
            3,
            "available",
            "not_required",
            &task_payload(
                "Implement the bounded current task.",
                ProjectedTaskAction::Implement,
                &["The focused test passes.", "cargo check succeeds."],
                &["src/current.rs", "tests/current_test.rs"],
            ),
        );
        insert_task(
            &database,
            FUTURE_TASK_ID,
            PLAN_ID,
            9,
            "blocked",
            "not_required",
            &task_payload(
                "FUTURE-SECRET-OBJECTIVE",
                ProjectedTaskAction::Wait,
                &["FUTURE-SECRET-CRITERION"],
                &["future/secret.rs"],
            ),
        );
        select_current_task(&database, Some(CURRENT_TASK_ID));
        let writer = AtomicProjectProjectionWriter::new(database.clone(), state_root.clone())
            .expect("writer fixture");

        let receipt = writer
            .refresh_current_task_projection(&refresh_request())
            .expect("current task refresh");

        assert_eq!(receipt.generation, 1);
        assert_eq!(receipt.audit, EventWriteOutcome::Inserted { sequence: 1 });
        let projection =
            fs::read_to_string(state_root.todo_projection_path()).expect("current task projection");
        let binding = parse_current_task_submission_binding(projection.as_bytes())
            .expect("public submission binding");
        assert_eq!(binding.task_id, CURRENT_TASK_ID);
        assert_eq!(binding.task_revision, 3);
        for required in [
            "\"objective\": \"Implement the bounded current task.\"",
            "\"allowed_action\": \"implement\"",
            "The focused test passes.",
            "cargo check succeeds.",
            "src/current.rs",
            "tests/current_test.rs",
            "hierarchy.task.current.submit",
        ] {
            assert!(
                projection.contains(required),
                "missing public field: {required}"
            );
        }
        for forbidden in [
            FUTURE_TASK_ID,
            "FUTURE-SECRET",
            "future/secret.rs",
            "private plan payload",
            "plan/active.json",
            "active_plan_id",
            "review_status",
            "implemented",
            "complete_plan",
        ] {
            assert!(
                !projection.contains(forbidden),
                "leaked private field: {forbidden}"
            );
        }
        assert!(!state_root.governance_error_projection_path().exists());
        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            1
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn no_current_task_projects_null_without_reading_a_future_task() {
        let (root, state_root, database) = fixture("no-current");
        insert_plan(&database, PLAN_ID, b"private plan");
        insert_task(
            &database,
            FUTURE_TASK_ID,
            PLAN_ID,
            1,
            "available",
            "not_required",
            &task_payload(
                "FUTURE-SECRET-OBJECTIVE",
                ProjectedTaskAction::Implement,
                &[],
                &["future.rs"],
            ),
        );
        select_current_task(&database, None);
        let writer = AtomicProjectProjectionWriter::new(database, state_root.clone()).unwrap();

        writer
            .refresh_current_task_projection(&refresh_request())
            .expect("empty refresh");

        let projection = fs::read_to_string(state_root.todo_projection_path()).unwrap();
        assert!(projection.contains("\"task\": null"));
        assert!(projection.contains("\"queue_status\": \"no_task_available\""));
        assert!(!projection.contains("FUTURE-SECRET"));
        assert!(!projection.contains(FUTURE_TASK_ID));
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn malformed_current_payload_fails_before_projection_or_audit() {
        let (root, state_root, database) = fixture("malformed");
        insert_plan(&database, PLAN_ID, b"private plan");
        insert_task(
            &database,
            CURRENT_TASK_ID,
            PLAN_ID,
            1,
            "available",
            "not_required",
            b"not-a-canonical-task-payload",
        );
        select_current_task(&database, Some(CURRENT_TASK_ID));
        let writer = AtomicProjectProjectionWriter::new(database.clone(), state_root.clone())
            .expect("writer fixture");

        assert!(writer
            .refresh_current_task_projection(&refresh_request())
            .is_err());
        assert_no_projection_side_effects(&state_root, &database);
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn lifecycle_mismatch_fails_without_projecting_stale_action() {
        let (root, state_root, database) = fixture("lifecycle");
        insert_plan(&database, PLAN_ID, b"private plan");
        insert_task(
            &database,
            CURRENT_TASK_ID,
            PLAN_ID,
            1,
            "submitted",
            "pending",
            &task_payload(
                "Stale implementation action.",
                ProjectedTaskAction::Implement,
                &[],
                &[],
            ),
        );
        select_current_task(&database, Some(CURRENT_TASK_ID));
        let writer = AtomicProjectProjectionWriter::new(database.clone(), state_root.clone())
            .expect("writer fixture");

        assert!(writer
            .refresh_current_task_projection(&refresh_request())
            .is_err());
        assert_no_projection_side_effects(&state_root, &database);
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn canonical_lifecycle_projects_each_allowed_non_implementation_action() {
        let cases = [
            (
                "plan-review",
                "available",
                "not_required",
                ProjectedTaskAction::PlanReview,
                "plan_review",
                "awaiting_review",
            ),
            (
                "implementation-review",
                "submitted",
                "pending",
                ProjectedTaskAction::Review,
                "review",
                "awaiting_review",
            ),
            (
                "blocked",
                "blocked",
                "not_required",
                ProjectedTaskAction::Wait,
                "wait",
                "blocked",
            ),
        ];
        for (label, status, review_status, action, action_value, queue_value) in cases {
            let (root, state_root, database) = fixture(label);
            insert_plan(&database, PLAN_ID, b"private plan");
            insert_task(
                &database,
                CURRENT_TASK_ID,
                PLAN_ID,
                1,
                status,
                review_status,
                &task_payload("Current bounded action.", action, &[], &[]),
            );
            select_current_task(&database, Some(CURRENT_TASK_ID));
            let writer = AtomicProjectProjectionWriter::new(database, state_root.clone()).unwrap();

            writer
                .refresh_current_task_projection(&refresh_request())
                .expect("canonical action refresh");

            let projection = fs::read_to_string(state_root.todo_projection_path()).unwrap();
            assert!(projection.contains(&format!("\"allowed_action\": \"{action_value}\"")));
            assert!(projection.contains(&format!("\"queue_status\": \"{queue_value}\"")));
            fs::remove_dir_all(root).expect("fixture cleanup");
        }
    }

    #[test]
    fn current_task_outside_the_active_plan_is_not_projected() {
        let (root, state_root, database) = fixture("wrong-plan");
        insert_plan(&database, PLAN_ID, b"active private plan");
        insert_plan(&database, OTHER_PLAN_ID, b"inactive private plan");
        insert_task(
            &database,
            CURRENT_TASK_ID,
            OTHER_PLAN_ID,
            1,
            "available",
            "not_required",
            &task_payload(
                "Task from another plan.",
                ProjectedTaskAction::Implement,
                &[],
                &[],
            ),
        );
        select_current_task(&database, Some(CURRENT_TASK_ID));
        let writer = AtomicProjectProjectionWriter::new(database.clone(), state_root.clone())
            .expect("writer fixture");

        assert!(writer
            .refresh_current_task_projection(&refresh_request())
            .is_err());
        assert_no_projection_side_effects(&state_root, &database);
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn canonical_payload_encoder_rejects_private_or_non_relative_content() {
        let absolute = CanonicalTaskProjectionPayload {
            contract_version: CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION.to_string(),
            objective: "Inspect /home/operator/private/plan.json".to_string(),
            allowed_action: ProjectedTaskAction::Implement,
            acceptance_criteria: vec!["cargo check succeeds".to_string()],
            relevant_paths: vec!["../plan/task.json".to_string()],
        };
        assert!(encode_canonical_task_projection_payload(&absolute).is_err());

        let mut wrong_version = absolute;
        wrong_version.contract_version = "canonical-task-projection-v2".to_string();
        wrong_version.objective = "Safe objective".to_string();
        wrong_version.relevant_paths = vec!["src/lib.rs".to_string()];
        assert!(encode_canonical_task_projection_payload(&wrong_version).is_err());
    }

    #[test]
    fn identical_refresh_reuses_generation_and_does_not_touch_a_valid_projection() {
        let (root, state_root, database) = fixture("unchanged");
        insert_plan(&database, PLAN_ID, b"private plan");
        insert_task(
            &database,
            CURRENT_TASK_ID,
            PLAN_ID,
            1,
            "available",
            "not_required",
            &task_payload(
                "Stable current task.",
                ProjectedTaskAction::Implement,
                &["The projection stays stable."],
                &["src/lib.rs"],
            ),
        );
        select_current_task(&database, Some(CURRENT_TASK_ID));
        let writer = AtomicProjectProjectionWriter::new(database.clone(), state_root.clone())
            .expect("writer fixture");

        let first = writer
            .refresh_current_task_projection(&refresh_request())
            .expect("first refresh");
        let path = state_root.todo_projection_path();
        let original = fs::read(&path).expect("first projection");
        let original_modified = fs::metadata(&path).unwrap().modified().unwrap();
        #[cfg(unix)]
        let original_inode = fs::metadata(&path).unwrap().ino();

        let mut later = refresh_request();
        later.rendered_at = "2026-08-13T12:05:00Z".to_string();
        let duplicate = writer
            .refresh_current_task_projection(&later)
            .expect("unchanged refresh");
        assert_eq!(first.generation, 1);
        assert_eq!(first.audit, EventWriteOutcome::Inserted { sequence: 1 });
        assert_eq!(duplicate.generation, 1);
        assert_eq!(
            duplicate.audit,
            EventWriteOutcome::Duplicate { sequence: 1 }
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(
            fs::metadata(&path).unwrap().modified().unwrap(),
            original_modified
        );
        #[cfg(unix)]
        assert_eq!(fs::metadata(&path).unwrap().ino(), original_inode);

        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            1
        );
        let canonical =
            CanonicalStateStore::load_canonical_state(&database, PROJECT_ID, WORKSPACE_ID).unwrap();
        assert_eq!(canonical.projection_generation, 1);
        assert_eq!(canonical.last_event_sequence, 1);
        assert!(fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .contains("antdelirium-tmp")));
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn concurrent_identical_refreshes_commit_one_projection_generation() {
        let (root, state_root, database) = fixture("concurrent-unchanged");
        insert_plan(&database, PLAN_ID, b"private plan");
        insert_task(
            &database,
            CURRENT_TASK_ID,
            PLAN_ID,
            1,
            "available",
            "not_required",
            &task_payload(
                "Concurrent stable task.",
                ProjectedTaskAction::Implement,
                &[],
                &["src/lib.rs"],
            ),
        );
        select_current_task(&database, Some(CURRENT_TASK_ID));
        let writer = AtomicProjectProjectionWriter::new(database.clone(), state_root.clone())
            .expect("writer fixture");
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let writer = writer.clone();
            let barrier = barrier.clone();
            handles.push(thread::spawn(move || {
                barrier.wait();
                writer.refresh_current_task_projection(&refresh_request())
            }));
        }
        let receipts = handles
            .into_iter()
            .map(|handle| handle.join().expect("refresh thread").expect("refresh"))
            .collect::<Vec<_>>();
        assert_eq!(
            receipts
                .iter()
                .filter(|receipt| receipt.audit.inserted())
                .count(),
            1
        );
        assert!(receipts.iter().all(|receipt| receipt.generation == 1));
        assert!(receipts.iter().all(|receipt| receipt.audit.sequence() == 1));
        assert_eq!(
            AuditStore::audit_event_count(&database, PROJECT_ID).unwrap(),
            1
        );
        assert!(state_root.todo_projection_path().is_file());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
