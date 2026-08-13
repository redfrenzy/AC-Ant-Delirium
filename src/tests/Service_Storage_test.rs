use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

use ac_ant_delirium::libres::Rusqlite_unit::AC_params;
use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::structs::{
    CanonicalStateStore, CapabilityActor, CapabilityAuthorizationOutcome,
    CapabilityAuthorizationRequest, CapabilityChannel, CapabilityOperation, NewCapabilityGrant,
    NewCapabilityRevocation, StorageWorkspace,
};

#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};

const PROJECT_ID: &str = "prj_00000000000000000000000000";
const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";
const TASK_ID: &str = "tsk_00000000000000000000000000";
const PLAN_ID: &str = "pln_00000000000000000000000000";
const GRANT_ID: &str = "cap_00000000000000000000000000";
const REQUEST_ID: &str = "req_00000000000000000000000000";
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn unique_root(label: &str) -> PathBuf {
    let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/service-storage-tests/{label}-{}-{sequence}",
        std::process::id(),
    ));
    fs::create_dir_all(&root).expect("service fixture root");
    root
}

fn authority() -> VerifiedServiceAuthority {
    VerifiedServiceAuthority::for_current_test_process().expect("Linux effective uid")
}

fn project() -> StorageProject {
    StorageProject {
        project_id: PROJECT_ID.to_string(),
        project_key: "demo--00000000000000000000000000".to_string(),
        created_at: "2026-08-13T00:00:00Z".to_string(),
    }
}

fn seed_scoped_resource(database: &ProjectDatabase) {
    CanonicalStateStore::register_workspace(
        database,
        &StorageWorkspace {
            workspace_id: WORKSPACE_ID.to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_key: "wsp--00000000000000000000000000".to_string(),
            created_at: "2026-08-13T00:00:00Z".to_string(),
        },
    )
    .expect("workspace scope");
    database
        .with_immediate_transaction(|transaction| {
            transaction.execute(
                "INSERT INTO plans(plan_id, project_id, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
                AC_params![PLAN_ID, PROJECT_ID, b"{}", "2026-08-13T00:00:00Z"],
            )?;
            transaction.execute(
                "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at) VALUES (?1, ?2, ?3, 1, 'available', 'not_required', ?4, ?5)",
                AC_params![TASK_ID, PROJECT_ID, PLAN_ID, b"{}", "2026-08-13T00:00:00Z"],
            )?;
            Ok(())
        })
        .expect("task scope");
}

fn authorize_capability(
    registry: &ProjectDatabase,
    actor: CapabilityActor,
    channel: CapabilityChannel,
    operation: CapabilityOperation,
    project_id: Option<&str>,
    workspace_id: Option<&str>,
    task_id: Option<&str>,
) -> AuthorizedCapability {
    let secret = [7_u8; 32];
    let trusted_now_unix = trusted_service_unix_time().expect("trusted service clock");
    registry
        .store_capability_grant(&NewCapabilityGrant {
            grant_id: GRANT_ID.to_string(),
            secret_hash: sha256(&secret),
            subject_uid: 1000,
            actor,
            channel,
            operation,
            project_id: project_id.map(str::to_string),
            workspace_id: workspace_id.map(str::to_string),
            task_id: task_id.map(str::to_string),
            not_before_unix: trusted_now_unix - 60,
            expires_at_unix: trusted_now_unix + 60,
            revocation_epoch: 0,
            granted_at: "2026-08-13T00:00:00Z".to_string(),
        })
        .expect("stored capability");
    let outcome = registry
        .authorize_capability(
            1000,
            channel,
            &[9_u8; 32],
            trusted_now_unix,
            &CapabilityAuthorizationRequest {
                grant_id: GRANT_ID.to_string(),
                presented_secret: secret,
                operation,
                project_id: project_id.map(str::to_string),
                workspace_id: workspace_id.map(str::to_string),
                task_id: task_id.map(str::to_string),
                presented_connection_nonce: [9_u8; 32],
                request_id: REQUEST_ID.to_string(),
                request_fingerprint: [10_u8; 32],
            },
        )
        .expect("capability authorization");
    match outcome {
        CapabilityAuthorizationOutcome::Fresh(capability) => capability,
        _ => panic!("fresh capability expected"),
    }
}

#[test]
fn initializes_private_registry_and_idempotent_canonical_project_storage() {
    let root = unique_root("initialize");
    #[cfg(unix)]
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).expect("loosen fixture");

    let authority = authority();
    let storage =
        ProtectedServiceStorage::initialize(&authority, &root).expect("private service storage");
    let first = storage
        .materialize_project(&authority, &project())
        .expect("canonical project storage");
    let second = storage
        .materialize_project(&authority, &project())
        .expect("idempotent materialization");

    assert_eq!(first.database_path(), second.database_path());
    assert_eq!(
        storage.registry.registered_projects().unwrap(),
        vec![project()]
    );
    assert_eq!(first.registered_projects().unwrap(), vec![project()]);
    assert_eq!(
        first.health().unwrap().schema_version,
        STORAGE_SCHEMA_VERSION
    );

    #[cfg(unix)]
    for directory in [
        root.clone(),
        root.join(REGISTRY_DIR_NAME),
        root.join(PROJECTS_DIR_NAME),
        root.join(PROJECTS_DIR_NAME).join(&project().project_key),
        root.join(PROJECTS_DIR_NAME)
            .join(&project().project_key)
            .join("state"),
    ] {
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    #[cfg(unix)]
    for file in [
        storage.registry.database_path().to_path_buf(),
        first.database_path().to_path_buf(),
    ] {
        assert_eq!(
            fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    drop(storage);
    ProtectedServiceStorage::initialize(&authority, &root).expect("restart recovery");
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn registered_database_loss_is_not_replaced_with_empty_state() {
    let root = unique_root("missing-database");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let database = storage.materialize_project(&authority, &project()).unwrap();
    let database_path = database.database_path().to_path_buf();
    drop(database);
    drop(storage);
    fs::remove_file(&database_path).expect("simulate missing registered database");

    assert!(matches!(
        ProtectedServiceStorage::initialize(&authority, &root),
        Err(ProtectedStorageError::Storage)
    ));
    assert!(!database_path.exists());
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn interrupted_empty_project_directory_is_resumed_but_unexpected_content_is_not() {
    let root = unique_root("interrupted-directory");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let project_root = root.join(PROJECTS_DIR_NAME).join(&project().project_key);
    fs::create_dir(&project_root).expect("interrupted project directory");
    fs::create_dir(project_root.join("state")).expect("interrupted state directory");

    storage
        .materialize_project(&authority, &project())
        .expect("resume safe empty layout");

    let mut unexpected = project();
    unexpected.project_id = "prj_11111111111111111111111111".to_string();
    unexpected.project_key = "other--11111111111111111111111111".to_string();
    let unexpected_root = root.join(PROJECTS_DIR_NAME).join(&unexpected.project_key);
    fs::create_dir(&unexpected_root).unwrap();
    fs::write(unexpected_root.join("unknown"), b"preserve").unwrap();
    assert!(matches!(
        storage.materialize_project(&authority, &unexpected),
        Err(ProtectedStorageError::Storage)
    ));
    assert_eq!(
        fs::read(unexpected_root.join("unknown")).unwrap(),
        b"preserve"
    );
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn corrupt_registered_database_is_preserved_for_explicit_recovery() {
    let root = unique_root("corrupt-database");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let database = storage.materialize_project(&authority, &project()).unwrap();
    let database_path = database.database_path().to_path_buf();
    drop(database);
    drop(storage);
    let corrupt = b"preserve-corrupt-canonical-state";
    fs::write(&database_path, corrupt).expect("simulate corruption");

    assert!(matches!(
        ProtectedServiceStorage::initialize(&authority, &root),
        Err(ProtectedStorageError::Storage)
    ));
    assert_eq!(fs::read(&database_path).unwrap(), corrupt);
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn invalid_identity_cannot_escape_the_private_projects_directory() {
    let root = unique_root("invalid-identity");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let mut invalid = project();
    invalid.project_key = "../outside--00000000000000000000000000".to_string();

    assert_eq!(
        storage.materialize_project(&authority, &invalid).err(),
        Some(ProtectedStorageError::InvalidProjectIdentity)
    );
    assert!(!root.join("outside--00000000000000000000000000").exists());
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[cfg(unix)]
#[test]
fn symlinked_service_root_is_rejected_without_touching_its_target() {
    let parent = unique_root("symlink");
    let target = parent.join("target");
    let link = parent.join("service-root");
    fs::create_dir(&target).expect("symlink target");
    symlink(&target, &link).expect("service root symlink");

    assert!(matches!(
        ProtectedServiceStorage::initialize(&authority(), &link),
        Err(ProtectedStorageError::InvalidRoot)
    ));
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    fs::remove_dir_all(parent).expect("fixture cleanup");
}

#[cfg(unix)]
#[test]
fn symlinked_sqlite_sidecar_is_rejected_without_touching_its_target() {
    let root = unique_root("sidecar-symlink");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let database = storage.materialize_project(&authority, &project()).unwrap();
    let mut wal_path = database.database_path().as_os_str().to_os_string();
    wal_path.push("-wal");
    let wal_path = PathBuf::from(wal_path);
    drop(database);
    drop(storage);
    if wal_path.exists() {
        fs::remove_file(&wal_path).unwrap();
    }
    let target = root.join("sidecar-target");
    fs::write(&target, b"do-not-touch").unwrap();
    symlink(&target, &wal_path).unwrap();

    assert!(matches!(
        ProtectedServiceStorage::initialize(&authority, &root),
        Err(ProtectedStorageError::Storage)
    ));
    assert_eq!(fs::read(&target).unwrap(), b"do-not-touch");
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn state_directory_contract_accepts_one_absolute_path_only() {
    assert_eq!(
        parse_state_directory(OsStr::new("/service-owned/state")),
        Ok(PathBuf::from("/service-owned/state"))
    );
    for invalid in ["", "relative/state", "/one:/two"] {
        assert_eq!(
            parse_state_directory(OsStr::new(invalid)),
            Err(ProtectedStorageError::InvalidRoot)
        );
    }
}

#[test]
fn resolves_authorized_opaque_scope_through_the_private_registry_only() {
    let root = unique_root("resolve-authorized-scope");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let database = storage.materialize_project(&authority, &project()).unwrap();
    seed_scoped_resource(&database);
    let capability = authorize_capability(
        storage.registry(),
        CapabilityActor::Agent,
        CapabilityChannel::Limited,
        CapabilityOperation::SubmissionCreate,
        Some(PROJECT_ID),
        Some(WORKSPACE_ID),
        Some(TASK_ID),
    );

    let resolved = storage
        .resolve_authorized_resource(&authority, &capability)
        .expect("service-only opaque resolution");

    assert_eq!(resolved.project_id(), PROJECT_ID);
    assert_eq!(resolved.workspace_id(), Some(WORKSPACE_ID));
    assert_eq!(resolved.task_id(), Some(TASK_ID));
    assert_eq!(
        resolved.database().database_path(),
        database.database_path()
    );
    assert_eq!(
        storage.registry().service_security_event_count().unwrap(),
        0
    );
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn refuses_inactive_scope_without_path_fallback() {
    let root = unique_root("refuse-scope-fallback");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let database = storage.materialize_project(&authority, &project()).unwrap();
    seed_scoped_resource(&database);
    database
        .with_immediate_transaction(|transaction| {
            transaction.execute(
                "UPDATE workspaces SET status = 'retired' WHERE workspace_id = ?1",
                AC_params![WORKSPACE_ID],
            )?;
            Ok(())
        })
        .unwrap();
    let capability = authorize_capability(
        storage.registry(),
        CapabilityActor::Agent,
        CapabilityChannel::Limited,
        CapabilityOperation::SubmissionCreate,
        Some(PROJECT_ID),
        Some(WORKSPACE_ID),
        Some(TASK_ID),
    );

    assert!(matches!(
        storage.resolve_authorized_resource(&authority, &capability),
        Err(())
    ));
    assert_eq!(
        storage.registry().service_security_event_count().unwrap(),
        1
    );
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn refuses_unknown_workspace_without_selecting_by_cwd_or_project_name() {
    let root = unique_root("unknown-workspace");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    storage.materialize_project(&authority, &project()).unwrap();
    let capability = authorize_capability(
        storage.registry(),
        CapabilityActor::Agent,
        CapabilityChannel::Limited,
        CapabilityOperation::TaskCurrentRead,
        Some(PROJECT_ID),
        Some("wsp_11111111111111111111111111"),
        None,
    );

    assert!(storage
        .resolve_authorized_resource(&authority, &capability)
        .is_err());
    assert_eq!(
        storage.registry().service_security_event_count().unwrap(),
        1
    );
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn refuses_capability_issued_by_a_project_database_instead_of_the_registry() {
    let root = unique_root("wrong-capability-issuer");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let database = storage.materialize_project(&authority, &project()).unwrap();
    seed_scoped_resource(&database);
    let capability = authorize_capability(
        &database,
        CapabilityActor::Agent,
        CapabilityChannel::Limited,
        CapabilityOperation::SubmissionCreate,
        Some(PROJECT_ID),
        Some(WORKSPACE_ID),
        Some(TASK_ID),
    );

    assert!(storage
        .resolve_authorized_resource(&authority, &capability)
        .is_err());
    assert_eq!(
        storage.registry().service_security_event_count().unwrap(),
        1
    );
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn rechecks_revocation_in_the_registry_before_resolving_a_path() {
    let root = unique_root("revoked-before-resolution");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let database = storage.materialize_project(&authority, &project()).unwrap();
    seed_scoped_resource(&database);
    let capability = authorize_capability(
        storage.registry(),
        CapabilityActor::Agent,
        CapabilityChannel::Limited,
        CapabilityOperation::SubmissionCreate,
        Some(PROJECT_ID),
        Some(WORKSPACE_ID),
        Some(TASK_ID),
    );
    storage
        .registry()
        .revoke_capability(&NewCapabilityRevocation {
            revocation_id: "rev_00000000000000000000000000".to_string(),
            grant_id: GRANT_ID.to_string(),
            revocation_epoch: 0,
            revoked_at: "2026-08-13T00:01:00Z".to_string(),
            reason: "revoked before dispatch".to_string(),
        })
        .unwrap();

    assert!(storage
        .resolve_authorized_resource(&authority, &capability)
        .is_err());
    assert_eq!(
        storage.registry().service_security_event_count().unwrap(),
        1
    );
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn global_administrative_capability_cannot_select_a_default_project() {
    let root = unique_root("no-default-project");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    storage.materialize_project(&authority, &project()).unwrap();
    let capability = authorize_capability(
        storage.registry(),
        CapabilityActor::Operator,
        CapabilityChannel::Administrative,
        CapabilityOperation::DiagnosticPrivate,
        None,
        None,
        None,
    );

    assert!(matches!(
        storage.resolve_authorized_resource(&authority, &capability),
        Err(())
    ));
    assert_eq!(
        storage.registry().service_security_event_count().unwrap(),
        1
    );
    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn journals_authenticated_rejection_with_opaque_scope_and_no_locator_fields() {
    let root = unique_root("journal-authenticated-rejection");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let database = storage.materialize_project(&authority, &project()).unwrap();
    seed_scoped_resource(&database);
    let capability = authorize_capability(
        storage.registry(),
        CapabilityActor::Reviewer,
        CapabilityChannel::Limited,
        CapabilityOperation::ReviewDecideAssigned,
        Some(PROJECT_ID),
        Some(WORKSPACE_ID),
        Some(TASK_ID),
    );

    let receipt = storage
        .audit_rejected_authorized_request(&capability)
        .expect("protected rejection journal");

    assert_eq!(receipt.sequence(), 1);
    assert!(receipt.event_id().starts_with("evt_"));
    assert!(receipt.correlation_id().starts_with("ref_"));
    assert_eq!(
        storage.registry().service_security_event_count().unwrap(),
        1
    );
    storage
        .registry()
        .with_immediate_transaction(|transaction| {
            let persisted = transaction.query_row(
                "SELECT event_kind, outcome, peer_uid, channel_kind, actor_kind, operation, project_id, workspace_id, task_id FROM service_security_events WHERE sequence = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?, row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?, row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?, row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?, row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )?;
            assert_eq!(
                persisted,
                (
                    "request_rejected".to_string(),
                    "rejected".to_string(),
                    1000,
                    "limited".to_string(),
                    "reviewer".to_string(),
                    "review.decide.assigned".to_string(),
                    PROJECT_ID.to_string(),
                    WORKSPACE_ID.to_string(),
                    TASK_ID.to_string(),
                )
            );
            Ok(())
        })
        .unwrap();
    fs::remove_dir_all(root).expect("fixture cleanup");
}
