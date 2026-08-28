#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::libres::Rusqlite_unit::AC_params;
    use crate::libres::Sha2_unit::sha256;
    use crate::structs::{
        CapabilityActor, CapabilityAuthorizationOutcome, CapabilityAuthorizationRequest,
        CapabilityChannel, CapabilityOperation, NewCapabilityGrant, NewCapabilityRevocation,
        ProjectDatabase, ProjectStateRoot, StorageProject, STORAGE_SCHEMA_VERSION,
    };

    const PROJECT_ID: &str = "prj_00000000000000000000000000";
    const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";
    const TASK_ID: &str = "tsk_00000000000000000000000000";
    const GRANT_ID: &str = "cap_00000000000000000000000000";
    const REQUEST_ID: &str = "req_00000000000000000000000000";
    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture_root(label: &str) -> PathBuf {
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/capability-auth-tests/{label}-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("project-local capability fixture");
        root
    }

    fn database(root: &PathBuf) -> ProjectDatabase {
        let state_root = ProjectStateRoot::with_default_config(
            root.join("protected"),
            root.join("project"),
            root.join("workspace"),
        );
        let database = ProjectDatabase::initialize(&state_root).expect("capability database");
        database
            .ensure_project(&StorageProject {
                project_id: PROJECT_ID.to_string(),
                project_key: "capability-project".to_string(),
                created_at: "2026-08-13T00:00:00Z".to_string(),
            })
            .expect("registered project");
        database
    }

    fn unregistered_database(root: &PathBuf) -> ProjectDatabase {
        ProjectDatabase::initialize(&ProjectStateRoot::with_default_config(
            root.join("protected"),
            root.join("project"),
            root.join("workspace"),
        ))
        .expect("capability database")
    }

    fn grant(secret: &[u8; 32]) -> NewCapabilityGrant {
        NewCapabilityGrant {
            grant_id: GRANT_ID.to_string(),
            secret_hash: sha256(secret),
            subject_uid: 1000,
            actor: CapabilityActor::Agent,
            channel: CapabilityChannel::Limited,
            operation: CapabilityOperation::SubmissionCreate,
            project_id: Some(PROJECT_ID.to_string()),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            task_id: Some(TASK_ID.to_string()),
            not_before_unix: 100,
            expires_at_unix: 200,
            revocation_epoch: 0,
            granted_at: "2026-08-13T00:00:00Z".to_string(),
        }
    }

    fn request(secret: [u8; 32]) -> CapabilityAuthorizationRequest {
        CapabilityAuthorizationRequest {
            grant_id: GRANT_ID.to_string(),
            presented_secret: secret,
            operation: CapabilityOperation::SubmissionCreate,
            project_id: Some(PROJECT_ID.to_string()),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            task_id: Some(TASK_ID.to_string()),
            presented_connection_nonce: [9; 32],
            request_id: REQUEST_ID.to_string(),
            request_fingerprint: [10; 32],
        }
    }

    fn authorize(
        database: &ProjectDatabase,
        trusted_uid: u32,
        trusted_channel: CapabilityChannel,
        trusted_nonce: &[u8; 32],
        trusted_now_unix: i64,
        request: &CapabilityAuthorizationRequest,
    ) -> CapabilityAuthorizationOutcome {
        database
            .authorize_capability(
                trusted_uid,
                trusted_channel,
                trusted_nonce,
                trusted_now_unix,
                request,
            )
            .expect("authorization transaction")
    }

    #[test]
    fn authenticates_exact_scope_and_deduplicates_request_fingerprint() {
        let root = fixture_root("fresh-replay");
        let database = database(&root);
        let secret = [7; 32];
        database
            .store_capability_grant(&grant(&secret))
            .expect("stored grant hash");
        let request = request(secret);

        let fresh = authorize(
            &database,
            1000,
            CapabilityChannel::Limited,
            &[9; 32],
            150,
            &request,
        );
        let CapabilityAuthorizationOutcome::Fresh(authorized) = fresh else {
            panic!("first authorization must be fresh");
        };
        assert_eq!(authorized.actor(), CapabilityActor::Agent);
        assert_eq!(
            authorized.operation(),
            CapabilityOperation::SubmissionCreate
        );
        assert_eq!(authorized.request_id(), REQUEST_ID);
        assert!(matches!(
            authorize(
                &database,
                1000,
                CapabilityChannel::Limited,
                &[9; 32],
                150,
                &request,
            ),
            CapabilityAuthorizationOutcome::Replay(_)
        ));
        let mut conflicting = request.clone();
        conflicting.request_fingerprint = [11; 32];
        assert_eq!(
            authorize(
                &database,
                1000,
                CapabilityChannel::Limited,
                &[9; 32],
                150,
                &conflicting,
            ),
            CapabilityAuthorizationOutcome::Denied
        );

        database
            .with_immediate_transaction(|transaction| {
                let stored: Vec<u8> = transaction.query_row(
                    "SELECT secret_hash FROM capability_grants WHERE grant_id = ?1",
                    AC_params![GRANT_ID],
                    |row| row.get(0),
                )?;
                assert_eq!(stored, sha256(&secret));
                assert_ne!(stored, secret);
                Ok(())
            })
            .unwrap();
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn ci_attestation_is_allowed_only_on_the_exact_administrative_candidate_scope() {
        let root = fixture_root("ci-migration-attestation");
        let database = database(&root);
        let secret = [12; 32];
        let mut ci_grant = grant(&secret);
        ci_grant.actor = CapabilityActor::Ci;
        ci_grant.channel = CapabilityChannel::Administrative;
        ci_grant.operation = CapabilityOperation::AttestationCreate;
        ci_grant.task_id = None;
        database.store_capability_grant(&ci_grant).unwrap();
        let request = CapabilityAuthorizationRequest {
            grant_id: GRANT_ID.to_string(),
            presented_secret: secret,
            operation: CapabilityOperation::AttestationCreate,
            project_id: Some(PROJECT_ID.to_string()),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            task_id: None,
            presented_connection_nonce: [9; 32],
            request_id: REQUEST_ID.to_string(),
            request_fingerprint: [13; 32],
        };
        let outcome = authorize(
            &database,
            1000,
            CapabilityChannel::Administrative,
            &[9; 32],
            150,
            &request,
        );
        let CapabilityAuthorizationOutcome::Fresh(authorized) = outcome else {
            panic!("CI verifier capability should be fresh");
        };
        assert_eq!(authorized.actor(), CapabilityActor::Ci);
        assert_eq!(
            authorized.operation(),
            CapabilityOperation::AttestationCreate
        );
        assert_eq!(authorized.project_id(), Some(PROJECT_ID));
        assert_eq!(authorized.workspace_id(), Some(WORKSPACE_ID));

        let mut operator_grant = grant(&[14; 32]);
        operator_grant.grant_id = "cap_11111111111111111111111111".to_string();
        operator_grant.actor = CapabilityActor::Operator;
        operator_grant.channel = CapabilityChannel::Administrative;
        operator_grant.operation = CapabilityOperation::AttestationCreate;
        operator_grant.task_id = None;
        assert!(database.store_capability_grant(&operator_grant).is_err());
    }

    #[test]
    fn all_credential_channel_operation_scope_nonce_and_time_mismatches_are_denied() {
        let root = fixture_root("mismatch");
        let database = database(&root);
        let secret = [7; 32];
        database.store_capability_grant(&grant(&secret)).unwrap();
        let baseline = request(secret);

        let mut cases = Vec::new();
        let mut wrong_secret = baseline.clone();
        wrong_secret.presented_secret = [8; 32];
        cases.push((1000, CapabilityChannel::Limited, [9; 32], 150, wrong_secret));
        cases.push((
            999,
            CapabilityChannel::Limited,
            [9; 32],
            150,
            baseline.clone(),
        ));
        cases.push((
            1000,
            CapabilityChannel::Administrative,
            [9; 32],
            150,
            baseline.clone(),
        ));
        let mut wrong_operation = baseline.clone();
        wrong_operation.operation = CapabilityOperation::StateAdmin;
        cases.push((
            1000,
            CapabilityChannel::Limited,
            [9; 32],
            150,
            wrong_operation,
        ));
        let mut wrong_scope = baseline.clone();
        wrong_scope.task_id = Some("tsk_11111111111111111111111111".to_string());
        cases.push((1000, CapabilityChannel::Limited, [9; 32], 150, wrong_scope));
        cases.push((
            1000,
            CapabilityChannel::Limited,
            [8; 32],
            150,
            baseline.clone(),
        ));
        cases.push((
            1000,
            CapabilityChannel::Limited,
            [9; 32],
            200,
            baseline.clone(),
        ));

        let denied_attempts = cases.len() as i64;
        for (uid, channel, nonce, now_unix, candidate) in cases {
            assert_eq!(
                authorize(&database, uid, channel, &nonce, now_unix, &candidate),
                CapabilityAuthorizationOutcome::Denied
            );
        }
        assert_eq!(
            database.service_security_event_count().unwrap(),
            denied_attempts
        );
        database
            .with_immediate_transaction(|transaction| {
                let summary = transaction.query_row(
                    "SELECT min(sequence), max(sequence), count(*), count(DISTINCT event_id), count(DISTINCT correlation_id), min(event_kind), min(outcome), sum(actor_kind IS NOT NULL), sum(operation IS NOT NULL), sum(project_id IS NOT NULL), sum(workspace_id IS NOT NULL), sum(task_id IS NOT NULL) FROM service_security_events",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?, row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?, row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?, row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?, row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?, row.get::<_, i64>(9)?,
                            row.get::<_, i64>(10)?, row.get::<_, i64>(11)?,
                        ))
                    },
                )?;
                assert_eq!(summary.0, 1);
                assert_eq!(summary.1, denied_attempts);
                assert_eq!(summary.2, denied_attempts);
                assert_eq!(summary.3, denied_attempts);
                assert_eq!(summary.4, denied_attempts);
                assert_eq!(summary.5, "authorization_denied");
                assert_eq!(summary.6, "rejected");
                assert_eq!((summary.7, summary.8, summary.9, summary.10, summary.11), (0, 0, 0, 0, 0));

                let mut statement = transaction.prepare("PRAGMA table_info(service_security_events)")?;
                let columns = statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()?;
                for forbidden in [
                    "path",
                    "cwd",
                    "project_key",
                    "secret",
                    "token",
                    "nonce",
                    "grant_id",
                    "request_id",
                    "payload",
                ] {
                    assert!(!columns.iter().any(|column| column == forbidden));
                }
                Ok(())
            })
            .unwrap();
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn revocation_and_epoch_rotation_invalidate_preexisting_grants() {
        let root = fixture_root("revocation");
        let database = database(&root);
        let secret = [7; 32];
        database.store_capability_grant(&grant(&secret)).unwrap();
        database
            .revoke_capability(&NewCapabilityRevocation {
                revocation_id: "rev_00000000000000000000000000".to_string(),
                grant_id: GRANT_ID.to_string(),
                revocation_epoch: 0,
                revoked_at: "2026-08-13T00:01:00Z".to_string(),
                reason: "operator revocation".to_string(),
            })
            .unwrap();
        assert_eq!(
            authorize(
                &database,
                1000,
                CapabilityChannel::Limited,
                &[9; 32],
                150,
                &request(secret),
            ),
            CapabilityAuthorizationOutcome::Denied
        );

        database
            .advance_capability_epoch(1000, CapabilityActor::Agent, 0, 1)
            .unwrap();
        let mut rotated = grant(&[12; 32]);
        rotated.grant_id = "cap_11111111111111111111111111".to_string();
        rotated.revocation_epoch = 1;
        database.store_capability_grant(&rotated).unwrap();
        assert!(database
            .advance_capability_epoch(1000, CapabilityActor::Agent, 1, 1)
            .is_err());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn actor_matrix_prevents_agent_grants_on_the_administrative_channel() {
        let root = fixture_root("actor-matrix");
        let database = database(&root);
        let mut invalid = grant(&[7; 32]);
        invalid.channel = CapabilityChannel::Administrative;
        invalid.operation = CapabilityOperation::StateAdmin;
        assert!(database.store_capability_grant(&invalid).is_err());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn audited_ci_grant_can_authorize_a_pre_cutover_scope_and_be_revoked() {
        let root = fixture_root("managed-pre-cutover-ci");
        let database = unregistered_database(&root);
        let secret = [21; 32];
        let managed = NewCapabilityGrant {
            grant_id: GRANT_ID.to_string(),
            secret_hash: sha256(&secret),
            subject_uid: 1000,
            actor: CapabilityActor::Ci,
            channel: CapabilityChannel::Administrative,
            operation: CapabilityOperation::AttestationCreate,
            project_id: Some(PROJECT_ID.to_string()),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            task_id: None,
            not_before_unix: 100,
            expires_at_unix: 200,
            revocation_epoch: 0,
            granted_at: "unix:100".to_string(),
        };

        let grant_receipt = database
            .grant_managed_capability(1000, &managed, 100)
            .expect("audited pre-cutover grant");
        assert_eq!(grant_receipt.grant_id, GRANT_ID);
        assert_eq!(grant_receipt.audit_sequence, 1);
        assert_eq!(database.capability_management_event_count().unwrap(), 1);

        let request = CapabilityAuthorizationRequest {
            grant_id: GRANT_ID.to_string(),
            presented_secret: secret,
            operation: CapabilityOperation::AttestationCreate,
            project_id: Some(PROJECT_ID.to_string()),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            task_id: None,
            presented_connection_nonce: [9; 32],
            request_id: REQUEST_ID.to_string(),
            request_fingerprint: [22; 32],
        };
        assert!(matches!(
            authorize(
                &database,
                1000,
                CapabilityChannel::Administrative,
                &[9; 32],
                150,
                &request,
            ),
            CapabilityAuthorizationOutcome::Fresh(_)
        ));

        let revocation = database
            .revoke_managed_capability(1000, GRANT_ID, 0, 160)
            .expect("audited revocation");
        assert_eq!(revocation.grant_id, GRANT_ID);
        assert_eq!(revocation.audit_sequence, 2);
        assert_eq!(database.capability_management_event_count().unwrap(), 2);
        let mut second_request = request;
        second_request.request_id = "req_11111111111111111111111111".to_string();
        second_request.request_fingerprint = [23; 32];
        assert_eq!(
            authorize(
                &database,
                1000,
                CapabilityChannel::Administrative,
                &[9; 32],
                170,
                &second_request,
            ),
            CapabilityAuthorizationOutcome::Denied
        );

        database
            .with_immediate_transaction(|transaction| {
                let events = transaction.query_row(
                    "SELECT count(*), min(sequence), max(sequence), group_concat(action_kind, ','), min(operator_uid), min(subject_uid), min(actor_kind), min(channel_kind), min(operation), min(project_id), min(workspace_id), sum(task_id IS NOT NULL) FROM capability_management_events",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?, row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?, row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?, row.get::<_, i64>(5)?,
                            row.get::<_, String>(6)?, row.get::<_, String>(7)?,
                            row.get::<_, String>(8)?, row.get::<_, String>(9)?,
                            row.get::<_, String>(10)?, row.get::<_, i64>(11)?,
                        ))
                    },
                )?;
                assert_eq!(events.0, 2);
                assert_eq!((events.1, events.2), (1, 2));
                assert_eq!(events.3, "grant,revoke");
                assert_eq!((events.4, events.5), (1000, 1000));
                assert_eq!(events.6, "ci");
                assert_eq!(events.7, "administrative");
                assert_eq!(events.8, "attestation.create");
                assert_eq!(events.9, PROJECT_ID);
                assert_eq!(events.10, WORKSPACE_ID);
                assert_eq!(events.11, 0);

                let stored: Vec<u8> = transaction.query_row(
                    "SELECT secret_hash FROM capability_grants WHERE grant_id = ?1",
                    AC_params![GRANT_ID],
                    |row| row.get(0),
                )?;
                assert_eq!(stored, sha256(&secret));
                assert_ne!(stored, secret);

                let mut statement =
                    transaction.prepare("PRAGMA table_info(capability_management_events)")?;
                let columns = statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()?;
                for forbidden in ["path", "cwd", "project_key", "secret", "token", "nonce", "payload"] {
                    assert!(!columns.iter().any(|column| column.contains(forbidden)));
                }
                Ok(())
            })
            .unwrap();
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn managed_ci_state_transition_requires_exact_task_scope() {
        let root = fixture_root("managed-ci-state-transition");
        let database = unregistered_database(&root);
        let secret = [25; 32];
        let managed = NewCapabilityGrant {
            grant_id: GRANT_ID.to_string(),
            secret_hash: sha256(&secret),
            subject_uid: 1000,
            actor: CapabilityActor::Ci,
            channel: CapabilityChannel::Administrative,
            operation: CapabilityOperation::StateTransition,
            project_id: Some(PROJECT_ID.to_string()),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            task_id: Some(TASK_ID.to_string()),
            not_before_unix: 100,
            expires_at_unix: 200,
            revocation_epoch: 0,
            granted_at: "unix:100".to_string(),
        };
        let receipt = database
            .grant_managed_capability(1000, &managed, 100)
            .expect("scoped state transition grant");
        assert_eq!(receipt.audit_sequence, 1);

        let request = CapabilityAuthorizationRequest {
            grant_id: GRANT_ID.to_string(),
            presented_secret: secret,
            operation: CapabilityOperation::StateTransition,
            project_id: Some(PROJECT_ID.to_string()),
            workspace_id: Some(WORKSPACE_ID.to_string()),
            task_id: Some(TASK_ID.to_string()),
            presented_connection_nonce: [9; 32],
            request_id: REQUEST_ID.to_string(),
            request_fingerprint: [26; 32],
        };
        assert!(matches!(
            authorize(
                &database,
                1000,
                CapabilityChannel::Administrative,
                &[9; 32],
                150,
                &request,
            ),
            CapabilityAuthorizationOutcome::Fresh(_)
        ));

        let mut wrong_task = request;
        wrong_task.request_id = "req_11111111111111111111111111".to_string();
        wrong_task.request_fingerprint = [27; 32];
        wrong_task.task_id = Some("tsk_11111111111111111111111111".to_string());
        assert_eq!(
            authorize(
                &database,
                1000,
                CapabilityChannel::Administrative,
                &[9; 32],
                150,
                &wrong_task,
            ),
            CapabilityAuthorizationOutcome::Denied
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn managed_grant_refuses_administrative_privilege_amplification_atomically() {
        let root = fixture_root("managed-no-privilege-amplification");
        let database = unregistered_database(&root);
        let mut privileged = NewCapabilityGrant {
            grant_id: GRANT_ID.to_string(),
            secret_hash: sha256(&[24; 32]),
            subject_uid: 1000,
            actor: CapabilityActor::Operator,
            channel: CapabilityChannel::Administrative,
            operation: CapabilityOperation::InstallationAdmin,
            project_id: None,
            workspace_id: None,
            task_id: None,
            not_before_unix: 100,
            expires_at_unix: 200,
            revocation_epoch: 0,
            granted_at: "unix:100".to_string(),
        };
        assert!(database
            .grant_managed_capability(1000, &privileged, 100)
            .is_err());
        privileged.actor = CapabilityActor::Ci;
        assert!(database
            .grant_managed_capability(1000, &privileged, 100)
            .is_err());
        database
            .with_immediate_transaction(|transaction| {
                let grants: i64 =
                    transaction.query_row("SELECT count(*) FROM capability_grants", [], |row| {
                        row.get(0)
                    })?;
                assert_eq!(grants, 0);
                Ok(())
            })
            .unwrap();
        assert_eq!(database.capability_management_event_count().unwrap(), 0);
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn storage_upgrade_from_v8_preserves_grants_and_removes_only_the_scope_foreign_key() {
        let root = fixture_root("upgrade-v8-grants");
        let state_root = ProjectStateRoot::with_default_config(
            root.join("protected"),
            root.join("project"),
            root.join("workspace"),
        );
        let database = database(&root);
        let secret = [25; 32];
        database.store_capability_grant(&grant(&secret)).unwrap();
        database
            .with_immediate_transaction(|transaction| {
                transaction.execute_batch(
                    r#"
                    DROP TABLE capability_management_events;
                    DROP INDEX capability_scope_idx;
                    DROP INDEX capability_subject_idx;

                    ALTER TABLE capability_request_receipts RENAME TO capability_request_receipts_v9;
                    ALTER TABLE revocations RENAME TO revocations_v9;
                    ALTER TABLE capability_grants RENAME TO capability_grants_v9;

                    CREATE TABLE capability_grants (
                        grant_id TEXT PRIMARY KEY,
                        secret_hash BLOB NOT NULL CHECK (length(secret_hash) = 32),
                        subject_uid INTEGER NOT NULL CHECK (subject_uid >= 0),
                        actor_kind TEXT NOT NULL CHECK (actor_kind IN ('wrapper', 'agent', 'reviewer', 'operator', 'service', 'ci')),
                        channel_kind TEXT NOT NULL CHECK (channel_kind IN ('limited', 'administrative')),
                        operation TEXT NOT NULL CHECK (operation IN ('compile.observe', 'projection.refresh', 'task.current.read', 'submission.create', 'submission.read.assigned', 'review.decide.assigned', 'attestation.create', 'state.transition', 'state.admin', 'installation.admin', 'diagnostic.private')),
                        project_id TEXT REFERENCES projects(project_id) ON DELETE RESTRICT,
                        workspace_id TEXT,
                        task_id TEXT,
                        not_before_unix INTEGER NOT NULL,
                        expires_at_unix INTEGER NOT NULL CHECK (expires_at_unix > not_before_unix),
                        revocation_epoch INTEGER NOT NULL CHECK (revocation_epoch >= 0),
                        granted_at TEXT NOT NULL,
                        FOREIGN KEY (subject_uid, actor_kind) REFERENCES capability_subject_epochs(subject_uid, actor_kind) ON DELETE RESTRICT
                    ) STRICT;
                    CREATE TABLE revocations (
                        revocation_id TEXT PRIMARY KEY,
                        grant_id TEXT NOT NULL UNIQUE REFERENCES capability_grants(grant_id) ON DELETE RESTRICT,
                        revocation_epoch INTEGER NOT NULL CHECK (revocation_epoch >= 0),
                        revoked_at TEXT NOT NULL,
                        reason TEXT NOT NULL
                    ) STRICT;
                    CREATE TABLE capability_request_receipts (
                        grant_id TEXT NOT NULL REFERENCES capability_grants(grant_id) ON DELETE RESTRICT,
                        request_id TEXT NOT NULL,
                        request_fingerprint BLOB NOT NULL CHECK (length(request_fingerprint) = 32),
                        authorized_at_unix INTEGER NOT NULL,
                        PRIMARY KEY (grant_id, request_id)
                    ) STRICT;
                    INSERT INTO capability_grants SELECT * FROM capability_grants_v9;
                    INSERT INTO revocations SELECT * FROM revocations_v9;
                    INSERT INTO capability_request_receipts SELECT * FROM capability_request_receipts_v9;
                    DROP TABLE capability_request_receipts_v9;
                    DROP TABLE revocations_v9;
                    DROP TABLE capability_grants_v9;
                    CREATE INDEX capability_scope_idx ON capability_grants(project_id, workspace_id, task_id);
                    CREATE INDEX capability_subject_idx ON capability_grants(subject_uid, actor_kind, revocation_epoch);
                    DROP INDEX tasks_plan_ordinal_idx;
                    ALTER TABLE tasks DROP COLUMN task_ordinal;
                    DELETE FROM schema_migrations WHERE version IN (9, 10);
                    PRAGMA user_version = 8;
                    "#,
                )?;
                Ok(())
            })
            .unwrap();
        drop(database);

        let upgraded = ProjectDatabase::open_existing_at(state_root.root()).unwrap();
        assert_eq!(
            upgraded.health().unwrap().schema_version,
            STORAGE_SCHEMA_VERSION
        );
        assert!(matches!(
            authorize(
                &upgraded,
                1000,
                CapabilityChannel::Limited,
                &[9; 32],
                150,
                &request(secret),
            ),
            CapabilityAuthorizationOutcome::Fresh(_)
        ));
        upgraded
            .with_immediate_transaction(|transaction| {
                let project_foreign_keys: i64 = transaction.query_row(
                    "SELECT count(*) FROM pragma_foreign_key_list('capability_grants') WHERE \"table\" = 'projects'",
                    [],
                    |row| row.get(0),
                )?;
                let subject_foreign_keys: i64 = transaction.query_row(
                    "SELECT count(*) FROM pragma_foreign_key_list('capability_grants') WHERE \"table\" = 'capability_subject_epochs'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(project_foreign_keys, 0);
                assert_eq!(subject_foreign_keys, 2);
                Ok(())
            })
            .unwrap();
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
