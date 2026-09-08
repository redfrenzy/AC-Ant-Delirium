#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use crate::libres::Rusqlite_unit::AC_params;
    use crate::structs::{
        atomic_replace, recover_atomic_temporary_files, EventWriteOutcome, NewAuditEvent,
        ProjectDatabase, ProjectStateRoot, ProvisionalProjectRegistration, StorageProject,
        TransactionalStorageError, TransactionalStorageResult, AUDIT_EVENT_CONTRACT_VERSION,
        STORAGE_SCHEMA_VERSION,
    };

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "antdelirium-storage-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary directory should be created");
        path
    }

    fn state_root(root: &Path) -> ProjectStateRoot {
        ProjectStateRoot::with_default_config(
            root.join("protected-state/project/workspace"),
            root.join("project"),
            root.join("project-workspace"),
        )
    }

    fn project() -> StorageProject {
        StorageProject {
            project_id: "prj_00000000000000000000000000".to_string(),
            project_key: "project-fingerprint-v1".to_string(),
            created_at: "2026-08-11T00:00:00Z".to_string(),
        }
    }

    fn event(index: usize, fingerprint_index: usize) -> NewAuditEvent {
        NewAuditEvent {
            contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
            event_id: format!("evt_{index:026}"),
            project_id: project().project_id,
            workspace_id: None,
            actor_kind: "service".to_string(),
            actor_subject_id: "antdelirium-service".to_string(),
            event_kind: "compilation_observed".to_string(),
            outcome: "succeeded".to_string(),
            occurred_at: "2026-08-11T00:00:00Z".to_string(),
            fingerprint: format!("{fingerprint_index:064x}"),
            correlation_id: format!("correlation-{index}"),
            resource_id: None,
            payload: format!("event-{index}").into_bytes(),
        }
    }

    fn initialized_database(root: &Path) -> (ProjectStateRoot, ProjectDatabase) {
        let state_root = state_root(root);
        let database =
            ProjectDatabase::initialize(&state_root).expect("database should initialize");
        database
            .ensure_project(&project())
            .expect("project should be persisted");
        (state_root, database)
    }

    #[test]
    fn initializes_current_storage_with_the_required_connection_policy() {
        let root = unique_temp_dir("initialize");
        let (state_root, database) = initialized_database(&root);

        let health = database.health().expect("health check should pass");
        assert_eq!(health.schema_version, STORAGE_SCHEMA_VERSION);
        assert_eq!(health.journal_mode, "wal");
        assert_eq!(health.synchronous, 2);
        assert!(health.foreign_keys);
        assert!(!health.trusted_schema);
        assert_eq!(health.quick_check, "ok");
        assert_eq!(health.foreign_key_violations, 0);
        assert_eq!(database.database_path(), state_root.database_path());

        #[cfg(unix)]
        {
            assert_eq!(
                fs::metadata(state_root.transactional_state_dir())
                    .expect("state directory metadata should exist")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(database.database_path())
                    .expect("database metadata should exist")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn storage_v14_migration_adds_separate_immutable_task_package_records() {
        let root = unique_temp_dir("storage-v14-task-package-records");
        let (state_root, database) = initialized_database(&root);
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute("DROP TABLE task_package_approvals", [])?;
                transaction.execute("DROP TABLE task_package_attestations", [])?;
                transaction.execute("DROP TABLE task_package_proposals", [])?;
                transaction.execute("DELETE FROM schema_migrations WHERE version >= 14", [])?;
                transaction.pragma_update(None, "user_version", 13)?;
                Ok(())
            })
            .unwrap();
        drop(database);

        let migrated = ProjectDatabase::open_existing_at(state_root.root()).unwrap();
        assert_eq!(
            migrated.health().unwrap().schema_version,
            STORAGE_SCHEMA_VERSION
        );
        let tables = migrated
            .with_immediate_transaction(|transaction| {
                let mut statement = transaction.prepare(
                    "SELECT name FROM sqlite_schema WHERE type = 'table' AND name LIKE 'task_package_%' ORDER BY name",
                )?;
                let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
                let mut tables = Vec::new();
                for row in rows {
                    tables.push(row?);
                }
                Ok(tables)
            })
            .unwrap();
        assert_eq!(
            tables,
            vec![
                "task_package_approvals".to_string(),
                "task_package_attestations".to_string(),
                "task_package_proposals".to_string(),
            ]
        );
        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn provisional_identity_binding_cannot_be_reassigned_by_conflicting_registration() {
        let root = unique_temp_dir("provisional-identity-immutable");
        let (_, database) = initialized_database(&root);
        let second_project = StorageProject {
            project_id: "prj_11111111111111111111111111".to_string(),
            project_key: "second-project--11111111111111111111111111".to_string(),
            created_at: "2026-09-07T00:00:00Z".to_string(),
        };
        database
            .ensure_project(&second_project)
            .expect("second project should be persisted");
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces(workspace_id, project_id, workspace_key, created_at) VALUES (?1, ?2, ?3, ?4)",
                    AC_params![
                        "wsp_00000000000000000000000000",
                        project().project_id,
                        "workspace--00000000000000000000000000",
                        "2026-09-07T00:00:00Z",
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO workspaces(workspace_id, project_id, workspace_key, created_at) VALUES (?1, ?2, ?3, ?4)",
                    AC_params![
                        "wsp_11111111111111111111111111",
                        second_project.project_id,
                        "workspace--11111111111111111111111111",
                        "2026-09-07T00:00:00Z",
                    ],
                )?;
                Ok(())
            })
            .expect("workspace fixtures should be persisted");

        let original = ProvisionalProjectRegistration {
            peer_uid: 1000,
            project_identity_fingerprint: [31_u8; 32],
            workspace_identity_fingerprint: [32_u8; 32],
            project_id: project().project_id,
            workspace_id: "wsp_00000000000000000000000000".to_string(),
            project_key: project().project_key,
            workspace_key: "workspace--00000000000000000000000000".to_string(),
            created_at_unix: 1_788_739_200,
        };
        database
            .store_provisional_registration(&original)
            .expect("initial identity binding should be persisted");
        let conflicting = ProvisionalProjectRegistration {
            peer_uid: original.peer_uid,
            project_identity_fingerprint: original.project_identity_fingerprint,
            workspace_identity_fingerprint: original.workspace_identity_fingerprint,
            project_id: second_project.project_id,
            workspace_id: "wsp_11111111111111111111111111".to_string(),
            project_key: second_project.project_key,
            workspace_key: "workspace--11111111111111111111111111".to_string(),
            created_at_unix: original.created_at_unix + 1,
        };

        assert!(matches!(
            database.store_provisional_registration(&conflicting),
            Err(TransactionalStorageError::Integrity(message))
                if message == "project identity is already bound to another project"
        ));
        assert_eq!(
            database
                .provisional_registration(
                    original.peer_uid,
                    &original.workspace_identity_fingerprint
                )
                .expect("registration lookup should succeed"),
            Some(original)
        );

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn storage_v10_migration_assigns_a_stable_order_to_existing_tasks() {
        let root = unique_temp_dir("v10-task-order");
        let (state_root, database) = initialized_database(&root);
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute("DROP INDEX provisional_project_identity_idx", [])?;
                transaction.execute("DROP INDEX provisional_project_peer_idx", [])?;
                transaction.execute(
                    "ALTER TABLE provisional_project_registrations RENAME TO provisional_project_registrations_v11",
                    [],
                )?;
                transaction.execute_batch(
                    r#"
CREATE TABLE provisional_project_registrations (
    peer_uid INTEGER NOT NULL CHECK (peer_uid >= 0),
    identity_fingerprint BLOB NOT NULL CHECK (length(identity_fingerprint) = 32),
    project_id TEXT NOT NULL UNIQUE REFERENCES projects(project_id) ON DELETE RESTRICT,
    workspace_id TEXT NOT NULL UNIQUE REFERENCES workspaces(workspace_id) ON DELETE RESTRICT,
    project_key TEXT NOT NULL UNIQUE,
    workspace_key TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL CHECK (created_at_unix >= 0),
    PRIMARY KEY (peer_uid, identity_fingerprint)
) STRICT;
CREATE INDEX provisional_project_peer_idx
ON provisional_project_registrations(peer_uid, created_at_unix);
DROP TABLE provisional_project_registrations_v11;
DROP TABLE provisional_project_identities;
"#,
                )?;
                transaction.execute("DELETE FROM schema_migrations WHERE version = 11", [])?;
                transaction.execute(
                    "INSERT INTO plans(plan_id, project_id, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
                    AC_params![
                        "pln_00000000000000000000000000",
                        project().project_id,
                        b"migration fixture".as_slice(),
                        "2026-08-27T00:00:00Z",
                    ],
                )?;
                transaction.execute("DROP INDEX tasks_plan_ordinal_idx", [])?;
                transaction.execute("ALTER TABLE tasks DROP COLUMN task_ordinal", [])?;
                transaction.execute(
                    "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at) VALUES (?1, ?2, ?3, 1, 'available', 'not_required', ?4, ?5)",
                    AC_params![
                        "tsk_00000000000000000000000002",
                        project().project_id,
                        "pln_00000000000000000000000000",
                        b"second lexical task".as_slice(),
                        "2026-08-27T00:00:00Z",
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at) VALUES (?1, ?2, ?3, 1, 'available', 'not_required', ?4, ?5)",
                    AC_params![
                        "tsk_00000000000000000000000001",
                        project().project_id,
                        "pln_00000000000000000000000000",
                        b"first lexical task".as_slice(),
                        "2026-08-27T00:00:00Z",
                    ],
                )?;
                transaction.execute("DROP TABLE task_package_approvals", [])?;
                transaction.execute("DROP TABLE task_package_attestations", [])?;
                transaction.execute("DROP TABLE task_package_proposals", [])?;
                transaction.execute("DROP TABLE backup_checkpoints", [])?;
                transaction.execute(
                    "DELETE FROM schema_migrations WHERE version IN (10, 11, 12, 13, 14, 15)",
                    [],
                )?;
                transaction.pragma_update(None, "user_version", 9)?;
                Ok(())
            })
            .unwrap();
        drop(database);

        let migrated = ProjectDatabase::open_existing_at(state_root.root()).unwrap();
        assert_eq!(
            migrated.health().unwrap().schema_version,
            STORAGE_SCHEMA_VERSION
        );
        let ordinals = migrated
            .with_immediate_transaction(|transaction| {
                Ok((
                    transaction.query_row(
                        "SELECT task_ordinal FROM tasks WHERE task_id = ?1",
                        AC_params!["tsk_00000000000000000000000001"],
                        |row| row.get::<_, i64>(0),
                    )?,
                    transaction.query_row(
                        "SELECT task_ordinal FROM tasks WHERE task_id = ?1",
                        AC_params!["tsk_00000000000000000000000002"],
                        |row| row.get::<_, i64>(0),
                    )?,
                ))
            })
            .unwrap();
        assert_eq!(ordinals, (0, 1));

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn deduplicates_fingerprints_without_creating_sequence_gaps() {
        let root = unique_temp_dir("deduplicate");
        let (_, database) = initialized_database(&root);

        let first = database.record_event(&event(1, 1)).expect("first event");
        let replay = database.record_event(&event(2, 1)).expect("replayed event");
        let second = database.record_event(&event(3, 2)).expect("second event");

        assert_eq!(first, EventWriteOutcome::Inserted { sequence: 1 });
        assert_eq!(replay, EventWriteOutcome::Duplicate { sequence: 1 });
        assert_eq!(second, EventWriteOutcome::Inserted { sequence: 2 });
        assert_eq!(database.event_count(&project().project_id).unwrap(), 2);
        assert_eq!(
            database.last_event_sequence(&project().project_id).unwrap(),
            2
        );

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn rolls_back_an_uncommitted_mutation_and_recovers_cleanly() {
        let root = unique_temp_dir("rollback");
        let (_, database) = initialized_database(&root);

        let result: TransactionalStorageResult<()> =
            database.with_immediate_transaction(|transaction| {
                transaction.execute(
                    "UPDATE projects SET last_event_sequence = 99 WHERE project_id = ?1",
                    AC_params![project().project_id],
                )?;
                Err(TransactionalStorageError::InvalidInput(
                    "simulated interrupted operation".to_string(),
                ))
            });

        assert!(matches!(
            result,
            Err(TransactionalStorageError::InvalidInput(_))
        ));
        assert_eq!(
            database.last_event_sequence(&project().project_id).unwrap(),
            0
        );
        let recovery = database.recover().expect("recovery should pass");
        assert_eq!(recovery.health.quick_check, "ok");
        assert_eq!(recovery.wal_busy, 0);

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn serializes_parallel_writers_per_project() {
        const WRITERS: usize = 8;
        const EVENTS_PER_WRITER: usize = 20;

        let root = unique_temp_dir("parallel-writers");
        let (_, database) = initialized_database(&root);
        let database = Arc::new(database);
        let barrier = Arc::new(Barrier::new(WRITERS));
        let mut handles = Vec::new();

        for writer in 0..WRITERS {
            let database = Arc::clone(&database);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                for offset in 0..EVENTS_PER_WRITER {
                    let index = writer * EVENTS_PER_WRITER + offset + 1;
                    let outcome = database.record_event(&event(index, index)).unwrap();
                    assert!(outcome.inserted());
                }
            }));
        }
        for handle in handles {
            handle.join().expect("writer thread should complete");
        }

        let expected = (WRITERS * EVENTS_PER_WRITER) as i64;
        assert_eq!(
            database.event_count(&project().project_id).unwrap(),
            expected
        );
        assert_eq!(
            database.last_event_sequence(&project().project_id).unwrap(),
            expected
        );
        database.health().expect("database should remain healthy");

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn concurrent_replay_persists_one_event_and_one_sequence() {
        const WRITERS: usize = 12;

        let root = unique_temp_dir("parallel-replay");
        let (_, database) = initialized_database(&root);
        let database = Arc::new(database);
        let barrier = Arc::new(Barrier::new(WRITERS));
        let mut handles = Vec::new();

        for writer in 0..WRITERS {
            let database = Arc::clone(&database);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                let replay = event(writer + 1, 777);
                barrier.wait();
                database.record_event(&replay).unwrap()
            }));
        }

        let mut inserted = 0;
        for handle in handles {
            let outcome = handle.join().expect("replay thread should complete");
            inserted += usize::from(outcome.inserted());
            assert_eq!(outcome.sequence(), 1);
        }
        assert_eq!(inserted, 1);
        assert_eq!(database.event_count(&project().project_id).unwrap(), 1);
        assert_eq!(
            database.last_event_sequence(&project().project_id).unwrap(),
            1
        );

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn parallel_initializers_apply_the_migration_once() {
        const INITIALIZERS: usize = 6;

        let root = unique_temp_dir("parallel-initialize");
        let state_root = Arc::new(state_root(&root));
        let barrier = Arc::new(Barrier::new(INITIALIZERS));
        let mut handles = Vec::new();

        for _ in 0..INITIALIZERS {
            let state_root = Arc::clone(&state_root);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                let database = ProjectDatabase::initialize(&state_root).unwrap();
                database.health().unwrap().schema_version
            }));
        }
        for handle in handles {
            assert_eq!(
                handle.join().expect("initializer thread should complete"),
                STORAGE_SCHEMA_VERSION
            );
        }

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn refuses_to_replace_a_corrupt_database_with_an_empty_one() {
        let root = unique_temp_dir("corrupt");
        let state_root = state_root(&root);
        fs::create_dir_all(state_root.transactional_state_dir()).unwrap();
        let corrupt = b"not a SQLite database";
        fs::write(state_root.database_path(), corrupt).unwrap();

        assert!(ProjectDatabase::initialize(&state_root).is_err());
        assert_eq!(fs::read(state_root.database_path()).unwrap(), corrupt);

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn failed_foreign_key_write_does_not_advance_the_sequence() {
        let root = unique_temp_dir("foreign-key");
        let (_, database) = initialized_database(&root);
        let mut invalid = event(1, 1);
        invalid.workspace_id = Some("wrk_unknown".to_string());

        assert!(database.record_event(&invalid).is_err());
        assert_eq!(database.event_count(&project().project_id).unwrap(), 0);
        assert_eq!(
            database.last_event_sequence(&project().project_id).unwrap(),
            0
        );

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }

    #[test]
    fn atomically_replaces_projection_files_and_recovers_owned_temps() {
        let root = unique_temp_dir("atomic-projection");
        let destination = root.join("guard/to-do_list.json");

        atomic_replace(&destination, b"old").expect("initial projection should be written");
        atomic_replace(&destination, b"new").expect("projection should be replaced");
        assert_eq!(fs::read(&destination).unwrap(), b"new");

        let orphan = destination
            .parent()
            .unwrap()
            .join(".to-do_list.json.antdelirium-tmp-orphan");
        fs::write(&orphan, b"incomplete").unwrap();
        let recovery = recover_atomic_temporary_files(&destination, Duration::ZERO).unwrap();
        assert_eq!(recovery.removed_temporary_files, 1);
        assert_eq!(recovery.preserved_recent_files, 0);
        assert!(!orphan.exists());

        #[cfg(unix)]
        assert_eq!(
            fs::metadata(destination).unwrap().permissions().mode() & 0o777,
            0o600
        );

        fs::remove_dir_all(root).expect("temporary directory should be removed");
    }
}
