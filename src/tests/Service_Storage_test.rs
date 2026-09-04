use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

use ac_ant_delirium::libres::Rusqlite_unit::AC_params;
use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::structs::{
    AuditStore, BackupCheckpointStore, CanonicalStateStore, CanonicalTaskAdvance,
    CanonicalTaskPackage, CanonicalTaskPackageTask, CapabilityActor,
    CapabilityAuthorizationOutcome, CapabilityAuthorizationRequest, CapabilityChannel,
    CapabilityOperation, NewAuditEvent, NewCapabilityGrant, NewCapabilityRevocation, NewReview,
    NewSubmission, ReviewDecision, ReviewStore, StorageWorkspace, SubmissionStore,
    AUDIT_EVENT_CONTRACT_VERSION, GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION,
    SUBMISSION_CONTRACT_VERSION,
};

#[cfg(unix)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::fd::OwnedFd;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
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

#[cfg(unix)]
fn open_directory(path: &Path) -> OwnedFd {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .unwrap()
        .into()
}

#[cfg(unix)]
fn workspace_fingerprint(project_fingerprint: &[u8; 32], path: &Path) -> [u8; 32] {
    let metadata = fs::metadata(path).unwrap();
    let mut material = b"antdelirium-provisional-workspace-identity-v2\0".to_vec();
    material.extend_from_slice(project_fingerprint);
    material.extend_from_slice(b"inode\0");
    material.extend_from_slice(&metadata.dev().to_be_bytes());
    material.extend_from_slice(&metadata.ino().to_be_bytes());
    sha256(&material)
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
        root.join(PROJECTS_DIR_NAME)
            .join(&project().project_key)
            .join("backup"),
        root.join(PROJECTS_DIR_NAME)
            .join(&project().project_key)
            .join("backup/repository.git"),
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

#[cfg(unix)]
#[test]
fn workspace_descriptor_must_match_the_registered_filesystem_identity() {
    let root = unique_root("workspace-descriptor");
    let workspace = unique_root("workspace-descriptor-source");
    let unrelated = unique_root("workspace-descriptor-unrelated");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let project_fingerprint = [42_u8; 32];
    let workspace_fingerprint = workspace_fingerprint(&project_fingerprint, &workspace);
    let registration = storage
        .create_provisional_registration(
            &authority,
            1000,
            &project_fingerprint,
            &workspace_fingerprint,
            "descriptor-project",
        )
        .unwrap();

    storage
        .validate_workspace_descriptor(
            1000,
            &registration.project_id,
            &registration.workspace_id,
            &open_directory(&workspace),
        )
        .expect("registered workspace descriptor");
    assert!(storage
        .validate_workspace_descriptor(
            1000,
            &registration.project_id,
            &registration.workspace_id,
            &open_directory(&unrelated),
        )
        .is_err());
    assert!(storage
        .validate_workspace_descriptor(
            1001,
            &registration.project_id,
            &registration.workspace_id,
            &open_directory(&workspace),
        )
        .is_err());
}

#[test]
fn state_admin_installs_a_generic_task_package_without_a_physical_locator() {
    let root = unique_root("generic-task-package");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let registration = storage
        .create_provisional_registration(
            &authority,
            1000,
            &[42_u8; 32],
            &[42_u8; 32],
            "synthetic-project",
        )
        .unwrap();
    let capability = authorize_capability(
        storage.registry(),
        CapabilityActor::Operator,
        CapabilityChannel::Administrative,
        CapabilityOperation::StateAdmin,
        None,
        None,
        None,
    );
    let package = CanonicalTaskPackage {
        contract_version: GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION.to_string(),
        project_id: registration.project_id.clone(),
        workspace_id: registration.workspace_id.clone(),
        plan_id: PLAN_ID.to_string(),
        expected_state_revision: 0,
        tasks: vec![CanonicalTaskPackageTask {
            task_id: TASK_ID.to_string(),
            task_revision: 1,
            objective: "Implement the synthetic service task.".to_string(),
            acceptance_criteria: vec!["The protected service test passes.".to_string()],
            relevant_paths: vec!["src/lib.rs".to_string()],
        }],
    };

    let installed = storage
        .install_generic_task_package(&authority, &capability, &package)
        .unwrap();
    assert_eq!(installed.state.state_revision, 1);
    assert_eq!(installed.state.current_task_id.as_deref(), Some(TASK_ID));
    assert_eq!(installed.state.tasks.len(), 1);

    let replay = storage
        .install_generic_task_package(&authority, &capability, &package)
        .unwrap();
    assert!(!replay.audit.inserted());
    assert_eq!(replay.state, installed.state);

    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn agent_cannot_install_or_advance_canonical_tasks_and_each_attempt_is_journaled() {
    let root = unique_root("agent-administrative-mutation");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let registration = storage
        .create_provisional_registration(
            &authority,
            1000,
            &[43_u8; 32],
            &[43_u8; 32],
            "synthetic-project",
        )
        .unwrap();
    let database = storage
        .open_registered_project(&authority, &registration.project_id)
        .unwrap();
    let canonical_before = database
        .load_canonical_state(&registration.project_id, &registration.workspace_id)
        .unwrap();
    let capability = authorize_capability(
        storage.registry(),
        CapabilityActor::Agent,
        CapabilityChannel::Limited,
        CapabilityOperation::SubmissionCreate,
        Some(&registration.project_id),
        Some(&registration.workspace_id),
        Some(TASK_ID),
    );
    let package = CanonicalTaskPackage {
        contract_version: GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION.to_string(),
        project_id: registration.project_id.clone(),
        workspace_id: registration.workspace_id.clone(),
        plan_id: PLAN_ID.to_string(),
        expected_state_revision: 0,
        tasks: vec![CanonicalTaskPackageTask {
            task_id: TASK_ID.to_string(),
            task_revision: 1,
            objective: "Unauthorized task.".to_string(),
            acceptance_criteria: Vec::new(),
            relevant_paths: vec!["src/lib.rs".to_string()],
        }],
    };

    assert!(storage
        .install_generic_task_package(&authority, &capability, &package)
        .is_err());
    assert!(storage
        .advance_reviewed_task(
            &authority,
            &capability,
            &CanonicalTaskAdvance {
                project_id: registration.project_id.clone(),
                workspace_id: registration.workspace_id.clone(),
                task_id: TASK_ID.to_string(),
                review_id: "rev_00000000000000000000000000".to_string(),
                expected_state_revision: 0,
            },
        )
        .is_err());
    assert_eq!(
        storage.registry().service_security_event_count().unwrap(),
        2
    );
    let canonical_after = database
        .load_canonical_state(&registration.project_id, &registration.workspace_id)
        .unwrap();
    assert_eq!(canonical_after, canonical_before);
    storage
        .registry()
        .with_immediate_transaction(|transaction| {
            let summary = transaction.query_row(
                "SELECT count(*), count(DISTINCT event_id), count(DISTINCT correlation_id), min(event_kind), min(outcome), min(actor_kind), min(operation), min(project_id), min(workspace_id), min(task_id) FROM service_security_events",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?, row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?, row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?, row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?, row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?, row.get::<_, String>(9)?,
                    ))
                },
            )?;
            assert_eq!(summary.0, 2);
            assert_eq!(summary.1, 2);
            assert_eq!(summary.2, 2);
            assert_eq!(summary.3, "request_rejected");
            assert_eq!(summary.4, "rejected");
            assert_eq!(summary.5, "agent");
            assert_eq!(summary.6, "submission.create");
            assert_eq!(summary.7, registration.project_id);
            assert_eq!(summary.8, registration.workspace_id);
            assert_eq!(summary.9, TASK_ID);
            Ok(())
        })
        .unwrap();

    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn interrupted_advance_commit_is_reused_and_finalized_atomically() {
    let root = unique_root("accepted-task-advance");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let registration = storage
        .create_provisional_registration(
            &authority,
            1000,
            &[44_u8; 32],
            &[44_u8; 32],
            "synthetic-project",
        )
        .unwrap();
    let database = storage
        .open_registered_project(&authority, &registration.project_id)
        .unwrap();
    let package = CanonicalTaskPackage {
        contract_version: GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION.to_string(),
        project_id: registration.project_id.clone(),
        workspace_id: registration.workspace_id.clone(),
        plan_id: PLAN_ID.to_string(),
        expected_state_revision: 0,
        tasks: vec![CanonicalTaskPackageTask {
            task_id: TASK_ID.to_string(),
            task_revision: 1,
            objective: "Advance only after accepted review.".to_string(),
            acceptance_criteria: vec!["Protected advancement succeeds.".to_string()],
            relevant_paths: vec!["src/lib.rs".to_string()],
        }],
    };
    let event = |index: i64, kind: &str, actor: &str, resource: &str| NewAuditEvent {
        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
        event_id: format!("evt_{index:026}"),
        project_id: registration.project_id.clone(),
        workspace_id: Some(registration.workspace_id.clone()),
        actor_kind: actor.to_string(),
        actor_subject_id: format!("authorized-{actor}"),
        event_kind: kind.to_string(),
        outcome: "succeeded".to_string(),
        occurred_at: "2026-08-27T20:00:00Z".to_string(),
        fingerprint: format!("{index:064x}"),
        correlation_id: format!("correlation-{index}"),
        resource_id: Some(resource.to_string()),
        payload: format!("event-{index}").into_bytes(),
    };
    CanonicalStateStore::install_task_package(
        &database,
        &package,
        &event(1, "state_transitioned", "operator", PLAN_ID),
    )
    .unwrap();
    let submission = NewSubmission {
        contract_version: SUBMISSION_CONTRACT_VERSION.to_string(),
        submission_id: "sub_00000000000000000000000000".to_string(),
        request_id: "accepted-task-advance-request".to_string(),
        project_id: registration.project_id.clone(),
        workspace_id: registration.workspace_id.clone(),
        task_id: TASK_ID.to_string(),
        task_revision: 1,
        base_revision: None,
        fingerprint: format!("{:064x}", 2),
        payload: b"synthetic accepted evidence".to_vec(),
        received_at: "2026-08-27T20:00:01Z".to_string(),
    };
    SubmissionStore::record_submission(
        &database,
        &submission,
        &event(2, "submission_recorded", "agent", &submission.submission_id),
    )
    .unwrap();
    let project_root = storage.projects_dir.join(&registration.project_key);
    let backup = crate::protected_git_backup::ProtectedGitBackup::initialize(
        &project_root,
        authority.effective_uid(),
    )
    .unwrap();
    backup
        .commit_snapshot(
            &crate::protected_git_backup::BackupCommitMetadata {
                transition: crate::protected_git_backup::BackupTransition::Baseline,
                project_id: registration.project_id.clone(),
                workspace_id: registration.workspace_id.clone(),
                task_id: Some(TASK_ID.to_string()),
                task_revision: Some(1),
                state_revision: 1,
                occurred_at_unix: 1_788_000_000,
                observed_compilation_fingerprint: Some("a".repeat(64)),
                transition_id: None,
            },
            vec![crate::protected_git_backup::BackupSnapshotEntry {
                relative_path: "src/lib.rs".to_string(),
                executable: false,
                contents: b"pub fn accepted() {}\n".to_vec(),
            }],
        )
        .unwrap();
    backup
        .commit_snapshot(
            &crate::protected_git_backup::BackupCommitMetadata {
                transition: crate::protected_git_backup::BackupTransition::Submission,
                project_id: registration.project_id.clone(),
                workspace_id: registration.workspace_id.clone(),
                task_id: Some(TASK_ID.to_string()),
                task_revision: Some(1),
                state_revision: 1,
                occurred_at_unix: 1_788_000_001,
                observed_compilation_fingerprint: None,
                transition_id: Some(submission.submission_id.clone()),
            },
            vec![crate::protected_git_backup::BackupSnapshotEntry {
                relative_path: "src/lib.rs".to_string(),
                executable: false,
                contents: b"pub fn accepted() {}\n".to_vec(),
            }],
        )
        .unwrap();
    let review = NewReview {
        review_id: "rev_00000000000000000000000000".to_string(),
        project_id: registration.project_id.clone(),
        submission_id: submission.submission_id.clone(),
        reviewer_id: "authorized-reviewer".to_string(),
        decision: ReviewDecision::Accepted,
        fingerprint: format!("{:064x}", 3),
        payload: b"accepted assigned review".to_vec(),
        created_at: "2026-08-27T20:00:02Z".to_string(),
    };
    ReviewStore::record_review(
        &database,
        &review,
        &event(3, "review_recorded", "reviewer", &review.review_id),
    )
    .unwrap();
    storage
        .commit_review_snapshot(
            &authority,
            &registration.project_id,
            &registration.workspace_id,
            TASK_ID,
            &submission.submission_id,
            &review.review_id,
        )
        .unwrap();
    let interrupted_receipt = backup
        .commit_snapshot_from_transition(
            &crate::protected_git_backup::BackupCommitMetadata {
                transition: crate::protected_git_backup::BackupTransition::Advance,
                project_id: registration.project_id.clone(),
                workspace_id: registration.workspace_id.clone(),
                task_id: Some(TASK_ID.to_string()),
                task_revision: Some(1),
                state_revision: 2,
                occurred_at_unix: 1_788_000_003,
                observed_compilation_fingerprint: None,
                transition_id: Some(review.review_id.clone()),
            },
            crate::protected_git_backup::BackupTransition::Review,
            &review.review_id,
        )
        .unwrap();
    assert!(database
        .backup_checkpoint(&registration.project_id, "advance", &review.review_id)
        .unwrap()
        .is_none());
    assert_eq!(
        database
            .load_canonical_state(&registration.project_id, &registration.workspace_id)
            .unwrap()
            .state_revision,
        1
    );
    let capability = authorize_capability(
        storage.registry(),
        CapabilityActor::Ci,
        CapabilityChannel::Administrative,
        CapabilityOperation::StateTransition,
        Some(&registration.project_id),
        Some(&registration.workspace_id),
        Some(TASK_ID),
    );
    let advance = CanonicalTaskAdvance {
        project_id: registration.project_id.clone(),
        workspace_id: registration.workspace_id.clone(),
        task_id: TASK_ID.to_string(),
        review_id: review.review_id.clone(),
        expected_state_revision: 1,
    };
    let advanced = storage
        .advance_reviewed_task(&authority, &capability, &advance)
        .unwrap();
    assert_eq!(advanced.audit.sequence(), 4);
    assert_eq!(advanced.state.state_revision, 2);
    assert_eq!(advanced.completed_task_id, TASK_ID);
    assert_eq!(advanced.next_task_id, None);
    assert_eq!(advanced.state.current_task_id, None);
    assert_eq!(advanced.state.active_plan_id, None);
    let checkpoint = database
        .backup_checkpoint(
            &advanced.state.project_id,
            "advance",
            "rev_00000000000000000000000000",
        )
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint.commit_id, interrupted_receipt.commit_id);
    assert_eq!(checkpoint.recorded_at, "unix:1788000003");
    let replay = storage
        .advance_reviewed_task(&authority, &capability, &advance)
        .unwrap();
    assert_eq!(replay.state, advanced.state);
    assert_eq!(replay.audit.sequence(), advanced.audit.sequence());
    assert!(!replay.audit.inserted());
    let repository = gix::open(project_root.join("backup/repository.git")).unwrap();
    let commit = repository
        .find_reference("refs/heads/snapshots")
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let snapshot = commit
        .tree()
        .unwrap()
        .lookup_entry_by_path("snapshot.json")
        .unwrap()
        .unwrap()
        .object()
        .unwrap()
        .into_blob()
        .data
        .clone();
    let snapshot: serde_json::Value = serde_json::from_slice(&snapshot).unwrap();
    assert_eq!(snapshot["transition"], "advance");
    assert_eq!(snapshot["transition_id"], "rev_00000000000000000000000000");
    assert_eq!(commit.ancestors().all().unwrap().count(), 4);

    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn provisional_bootstrap_is_uid_scoped_deduplicated_and_projection_ready() {
    let root = unique_root("provisional-bootstrap");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let fingerprint = [41_u8; 32];

    let first = storage
        .bootstrap_provisional(
            &authority,
            authority.effective_uid(),
            &fingerprint,
            &fingerprint,
            "Demo Crate",
        )
        .expect("first provisional bootstrap");
    let second = storage
        .bootstrap_provisional(
            &authority,
            authority.effective_uid(),
            &fingerprint,
            &fingerprint,
            "renamed-client-hint",
        )
        .expect("deduplicated provisional bootstrap");

    assert_eq!(first.project_id, second.project_id);
    assert_eq!(first.workspace_id, second.workspace_id);
    assert_ne!(first.compile.capability_id, second.compile.capability_id);
    assert_ne!(
        first.projection.capability_id,
        second.projection.capability_id
    );
    assert_eq!(
        storage
            .registry
            .provisional_registration_count(authority.effective_uid())
            .unwrap(),
        1
    );
    let registration = storage
        .registry
        .provisional_registration(authority.effective_uid(), &fingerprint)
        .unwrap()
        .expect("stored registration");
    assert!(registration.project_key.starts_with("demo-crate--"));
    let database = ProjectDatabase::open_existing_at(
        &root.join(PROJECTS_DIR_NAME).join(registration.project_key),
    )
    .expect("provisional database");
    assert_eq!(database.audit_event_count(&first.project_id).unwrap(), 0);
    let rendered_at = "2026-08-14T00:00:00Z";
    let current = database
        .render_current_task_projection(&first.project_id, &first.workspace_id, rendered_at)
        .expect("empty current task projection");
    let governance = database
        .render_governance_errors_projection(&first.project_id, &first.workspace_id, rendered_at)
        .expect("empty governance projection");
    assert!(String::from_utf8(current)
        .unwrap()
        .contains("\"task\": null"));
    assert!(String::from_utf8(governance)
        .unwrap()
        .contains("\"violations\": ["));

    fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn provisional_bootstrap_shares_project_and_separates_workspaces() {
    let root = unique_root("project-workspace-identities");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let project_fingerprint = [51_u8; 32];

    let main = storage
        .bootstrap_provisional(
            &authority,
            authority.effective_uid(),
            &project_fingerprint,
            &[52_u8; 32],
            "shared-project",
        )
        .unwrap();
    let linked = storage
        .bootstrap_provisional(
            &authority,
            authority.effective_uid(),
            &project_fingerprint,
            &[53_u8; 32],
            "ignored-renamed-hint",
        )
        .unwrap();

    assert_eq!(main.project_id, linked.project_id);
    assert_ne!(main.workspace_id, linked.workspace_id);
    assert_eq!(storage.registry().registered_projects().unwrap().len(), 1);
    assert_eq!(
        storage
            .registry()
            .provisional_registration_count(authority.effective_uid())
            .unwrap(),
        2
    );
    assert!(storage
        .registry()
        .contains_resource_scope(&main.project_id, Some(&main.workspace_id), None)
        .unwrap());
    assert!(storage
        .registry()
        .contains_resource_scope(&linked.project_id, Some(&linked.workspace_id), None)
        .unwrap());
    assert!(storage
        .bootstrap_provisional(
            &authority,
            authority.effective_uid(),
            &[54_u8; 32],
            &[52_u8; 32],
            "conflicting-project",
        )
        .is_err());

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
