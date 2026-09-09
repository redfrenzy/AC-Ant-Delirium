use super::*;
use crate::service_authority::VerifiedServiceAuthority;
use crate::service_storage::ProtectedServiceStorage;
use crate::task_package_contracts::MIGRATION_IDENTITY_ASSOCIATION_V1;
use ac_ant_delirium::structs::{
    CanonicalStateStore, CanonicalTaskPackage, CanonicalTaskPackageTask, StorageProject,
    StorageWorkspace, AUDIT_EVENT_CONTRACT_VERSION, GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION,
};
use std::path::PathBuf;

pub(crate) struct Fixture {
    pub(crate) root: PathBuf,
    pub(crate) authority: VerifiedServiceAuthority,
    pub(crate) storage: ProtectedServiceStorage,
    pub(crate) source: ProjectDatabase,
    pub(crate) candidate: ProjectDatabase,
    pub(crate) request: MigrationIdentityAssociation,
    pub(crate) service_subject: String,
}

pub(crate) fn fixture(label: &str) -> Fixture {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/identity-association-tests/{label}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let authority = VerifiedServiceAuthority::for_current_test_process().unwrap();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let initial = storage
        .bootstrap_provisional(
            &authority,
            authority.effective_uid(),
            &[41; 32],
            &[42; 32],
            "same-name",
        )
        .unwrap();
    let registration = storage
        .registry()
        .provisional_registration(authority.effective_uid(), &[42; 32])
        .unwrap()
        .unwrap();
    let source =
        ProjectDatabase::open_existing_at(&root.join("projects").join(registration.project_key))
            .unwrap();
    let project = StorageProject {
        project_id: "prj_00000000000000000000000001".to_string(),
        project_key: "same-name--00000000000000000000000001".to_string(),
        created_at: "migration-candidate".to_string(),
    };
    let workspace = StorageWorkspace {
        project_id: project.project_id.clone(),
        workspace_id: "wsp_00000000000000000000000001".to_string(),
        workspace_key: "workspace--00000000000000000000000001".to_string(),
        created_at: "migration-candidate".to_string(),
    };
    let candidate_root = root.join("projects").join(&project.project_key);
    std::fs::create_dir(&candidate_root).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&candidate_root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let candidate = ProjectDatabase::initialize_at(&candidate_root).unwrap();
    candidate.ensure_project(&project).unwrap();
    storage.registry().ensure_project(&project).unwrap();
    candidate.register_workspace(&workspace).unwrap();
    storage.registry().register_workspace(&workspace).unwrap();
    candidate
        .initialize_empty_governance_snapshot(
            &project.project_id,
            &workspace.workspace_id,
            "unix:1",
        )
        .unwrap();
    let package = CanonicalTaskPackage {
        contract_version: GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION.to_string(),
        project_id: project.project_id.clone(),
        workspace_id: workspace.workspace_id.clone(),
        plan_id: "pln_00000000000000000000000001".to_string(),
        expected_state_revision: 0,
        tasks: vec![CanonicalTaskPackageTask {
            task_id: "tsk_00000000000000000000000001".to_string(),
            task_revision: 1,
            objective: "Implement the approved migrated objective".to_string(),
            acceptance_criteria: vec!["Existing selection is preserved".to_string()],
            relevant_paths: vec!["src/lib.rs".to_string()],
        }],
    };
    candidate
        .install_task_package(
            &package,
            &NewAuditEvent {
                contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
                event_id: "evt_00000000000000000000000001".to_string(),
                project_id: project.project_id.clone(),
                workspace_id: Some(workspace.workspace_id.clone()),
                actor_kind: "operator".to_string(),
                actor_subject_id: "fixture-operator".to_string(),
                event_kind: "state_transitioned".to_string(),
                outcome: "succeeded".to_string(),
                occurred_at: "unix:1".to_string(),
                fingerprint: "1".repeat(64),
                correlation_id: "ref_00000000000000000000000001".to_string(),
                resource_id: Some(package.plan_id.clone()),
                payload: b"fixture".to_vec(),
            },
        )
        .unwrap();
    let mut request = MigrationIdentityAssociation {
        contract_version: MIGRATION_IDENTITY_ASSOCIATION_V1.to_string(),
        request_id: "req_00000000000000000000000001".to_string(),
        migration_id: "mig_00000000000000000000000001".to_string(),
        cutover_id: String::new(),
        approval_id: "apr_00000000000000000000000001".to_string(),
        provisional_project_id: initial.project_id,
        provisional_workspace_id: initial.workspace_id,
        candidate_project_id: project.project_id,
        candidate_workspace_id: workspace.workspace_id,
        project_identity_sha256: crate::legacy_staging::lowercase_hex(&[41; 32]),
        workspace_identity_sha256: crate::legacy_staging::lowercase_hex(&[42; 32]),
        candidate_logical_state_sha256: String::new(),
        cutover_binding_sha256: String::new(),
        provisional_state_policy: "require-empty".to_string(),
        expected_registry_revision: 0,
    };
    let mut material = b"migration-cutover-service-v1\0".to_vec();
    material.extend_from_slice(&authority.effective_uid().to_be_bytes());
    let service_subject = crate::legacy_staging::lowercase_hex(&sha256(&material));
    crate::legacy_cutover::legacy_cutover_tests::seed_association_fixture(
        &candidate,
        storage.registry(),
        &mut request,
        &service_subject,
    );
    Fixture {
        root,
        authority,
        storage,
        source,
        candidate,
        request,
        service_subject,
    }
}

impl Fixture {
    fn apply(
        &self,
        request: &MigrationIdentityAssociation,
    ) -> TransactionalStorageResult<MigrationIdentityAssociationReceipt> {
        let receipt = association_receipt(request)?;
        associate_identity(
            AssociationContext {
                registry: self.storage.registry(),
                provisional: &self.source,
                candidate: &self.candidate,
                peer_uid: self.authority.effective_uid(),
                service_subject_sha256: &self.service_subject,
            },
            request,
            &NewAuditEvent {
                contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
                event_id: "evt_00000000000000000000000002".to_string(),
                project_id: request.candidate_project_id.clone(),
                workspace_id: Some(request.candidate_workspace_id.clone()),
                actor_kind: "operator".to_string(),
                actor_subject_id: crate::service_storage::task_package_subject_sha256(
                    self.authority.effective_uid(),
                ),
                event_kind: "migration_applied".to_string(),
                outcome: "succeeded".to_string(),
                occurred_at: "unix:2".to_string(),
                fingerprint: receipt.association_sha256,
                correlation_id: "ref_00000000000000000000000002".to_string(),
                resource_id: Some(receipt.association_id),
                payload: b"association fixture".to_vec(),
            },
        )
    }
    fn association_count(&self) -> i64 {
        self.storage
            .registry()
            .with_immediate_transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT count(*) FROM migration_identity_associations",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }
}

#[test]
fn explicit_association_preserves_state_and_routes_only_the_exact_observed_identity() {
    let f = fixture("exact");
    use ac_ant_delirium::structs::{
        CapabilityAuthorizationOutcome, CapabilityAuthorizationRequest, CapabilityChannel,
        CapabilityOperation,
    };
    let old = f
        .storage
        .bootstrap_provisional(
            &f.authority,
            f.authority.effective_uid(),
            &[41; 32],
            &[42; 32],
            "same-name",
        )
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let old_request = CapabilityAuthorizationRequest {
        grant_id: old.compile.capability_id,
        presented_secret: old.compile.token,
        operation: CapabilityOperation::CompileObserve,
        project_id: Some(old.project_id),
        workspace_id: Some(old.workspace_id),
        task_id: None,
        presented_connection_nonce: [61; 32],
        request_id: "req_00000000000000000000000008".to_string(),
        request_fingerprint: [62; 32],
    };
    let CapabilityAuthorizationOutcome::Fresh(old_authority) = f
        .storage
        .registry()
        .authorize_capability(
            f.authority.effective_uid(),
            CapabilityChannel::Limited,
            &[61; 32],
            now,
            &old_request,
        )
        .unwrap()
    else {
        panic!("fresh wrapper authority expected")
    };
    let before = f
        .candidate
        .load_canonical_state(
            &f.request.candidate_project_id,
            &f.request.candidate_workspace_id,
        )
        .unwrap();
    let source = f
        .source
        .load_canonical_state(
            &f.request.provisional_project_id,
            &f.request.provisional_workspace_id,
        )
        .unwrap();
    let receipt = f.apply(&f.request).unwrap();
    assert!(!f
        .storage
        .registry()
        .authorized_capability_is_current(&old_authority, now)
        .unwrap());
    assert_eq!(
        f.storage
            .registry()
            .authorize_capability(
                f.authority.effective_uid(),
                CapabilityChannel::Limited,
                &[61; 32],
                now,
                &old_request
            )
            .unwrap(),
        CapabilityAuthorizationOutcome::Denied
    );
    assert_eq!(receipt.resulting_registry_revision, 1);
    assert_eq!(f.apply(&f.request).unwrap(), receipt);
    assert_eq!(f.association_count(), 1);
    assert_eq!(
        f.candidate
            .load_canonical_state(
                &f.request.candidate_project_id,
                &f.request.candidate_workspace_id
            )
            .unwrap(),
        before
    );
    assert_eq!(
        f.source
            .load_canonical_state(
                &f.request.provisional_project_id,
                &f.request.provisional_workspace_id
            )
            .unwrap(),
        source
    );
    let observed = f
        .storage
        .bootstrap_provisional(
            &f.authority,
            f.authority.effective_uid(),
            &[41; 32],
            &[42; 32],
            "untrusted-name",
        )
        .unwrap();
    assert_eq!(observed.project_id, f.request.candidate_project_id);
    assert_eq!(observed.workspace_id, f.request.candidate_workspace_id);
    assert!(f
        .storage
        .registry()
        .project_identity_is_retired(&f.request.provisional_project_id)
        .unwrap());
    assert!(f
        .storage
        .registry()
        .provisional_registration_for_scope(
            f.authority.effective_uid(),
            &f.request.provisional_project_id,
            &f.request.provisional_workspace_id
        )
        .unwrap()
        .is_none());
    assert!(f
        .storage
        .bootstrap_provisional(
            &f.authority,
            f.authority.effective_uid(),
            &[99; 32],
            &[42; 32],
            "same-name"
        )
        .is_err());
    assert!(f
        .storage
        .bootstrap_provisional(
            &f.authority,
            f.authority.effective_uid(),
            &[41; 32],
            &[43; 32],
            "same-name"
        )
        .is_err());
    let other = f
        .storage
        .bootstrap_provisional(
            &f.authority,
            f.authority.effective_uid(),
            &[44; 32],
            &[45; 32],
            "same-name",
        )
        .unwrap();
    assert_ne!(other.project_id, f.request.candidate_project_id);
    let reopened = ProtectedServiceStorage::initialize(&f.authority, &f.root).unwrap();
    let observed = reopened
        .bootstrap_provisional(
            &f.authority,
            f.authority.effective_uid(),
            &[41; 32],
            &[42; 32],
            "same-name",
        )
        .unwrap();
    assert_eq!(observed.project_id, f.request.candidate_project_id);
    ac_ant_delirium::structs::validate_serialized_public_document(
        &serde_json::to_vec(&receipt).unwrap(),
        "association receipt",
    )
    .unwrap();
}

#[test]
fn wrong_identity_binding_revision_scope_or_policy_never_changes_the_registry() {
    let f = fixture("reject");
    for field in [
        "project_identity_sha256",
        "workspace_identity_sha256",
        "candidate_logical_state_sha256",
        "cutover_binding_sha256",
        "candidate_workspace_id",
        "approval_id",
        "migration_id",
        "cutover_id",
        "expected_registry_revision",
        "provisional_state_policy",
    ] {
        let mut value = serde_json::to_value(&f.request).unwrap();
        value[field] = match field {
            "candidate_workspace_id" => "wsp_00000000000000000000000009".into(),
            "approval_id" => "apr_00000000000000000000000009".into(),
            "migration_id" => "mig_00000000000000000000000009".into(),
            "cutover_id" => "cut_00000000000000000000000009".into(),
            "expected_registry_revision" => 1.into(),
            "provisional_state_policy" => "discard-provisional".into(),
            _ => "9".repeat(64).into(),
        };
        assert!(
            f.apply(&serde_json::from_value(value).unwrap()).is_err(),
            "{field}"
        );
        assert_eq!(f.association_count(), 0);
    }
    f.source.with_immediate_transaction(|tx| {
        tx.execute("INSERT INTO plans(plan_id, project_id, payload, created_at) VALUES ('pln_00000000000000000000000009', ?1, X'7B7D', 'unix:1')", AC_params![f.request.provisional_project_id])?;
        Ok(())
    }).unwrap();
    assert!(f.apply(&f.request).is_err());
    assert_eq!(f.association_count(), 0);
}

#[test]
fn failure_after_audit_rolls_back_and_concurrent_retries_write_once() {
    let f = fixture("atomic");
    f.storage.registry().with_immediate_transaction(|tx| {
        tx.execute_batch("CREATE TRIGGER fixture_abort_association BEFORE INSERT ON migration_identity_associations BEGIN SELECT RAISE(ABORT, 'fixture'); END;")?;
        Ok(())
    }).unwrap();
    let events = f
        .storage
        .registry()
        .event_count(&f.request.candidate_project_id)
        .unwrap();
    assert!(f.apply(&f.request).is_err());
    assert_eq!(f.association_count(), 0);
    assert_eq!(
        f.storage
            .registry()
            .event_count(&f.request.candidate_project_id)
            .unwrap(),
        events
    );
    f.storage
        .registry()
        .with_immediate_transaction(|tx| {
            tx.execute_batch("DROP TRIGGER fixture_abort_association;")?;
            Ok(())
        })
        .unwrap();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let handles = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    f.apply(&f.request).unwrap()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results[0], results[1]);
    assert_eq!(f.association_count(), 1);
    assert_eq!(
        f.storage
            .registry()
            .event_count(&f.request.candidate_project_id)
            .unwrap(),
        events + 1
    );
    let mut conflict = f.request.clone();
    conflict.request_id = "req_00000000000000000000000009".to_string();
    assert!(f.apply(&conflict).is_err());
    assert!(f
        .storage
        .registry()
        .with_immediate_transaction(|tx| {
            tx.execute(
                "UPDATE migration_identity_associations SET receipt = X'7B7D'",
                [],
            )?;
            Ok(())
        })
        .is_err());
    assert!(f
        .storage
        .registry()
        .with_immediate_transaction(|tx| {
            tx.execute("DELETE FROM migration_identity_associations", [])?;
            Ok(())
        })
        .is_err());
    f.candidate
        .with_immediate_transaction(|tx| {
            tx.execute(
                "UPDATE projects SET state_revision = state_revision + 1 WHERE project_id = ?1",
                AC_params![f.request.candidate_project_id],
            )?;
            Ok(())
        })
        .unwrap();
    let advanced = f
        .candidate
        .load_canonical_state(
            &f.request.candidate_project_id,
            &f.request.candidate_workspace_id,
        )
        .unwrap();
    assert_eq!(f.apply(&f.request).unwrap(), results[0]);
    assert_eq!(
        f.candidate
            .load_canonical_state(
                &f.request.candidate_project_id,
                &f.request.candidate_workspace_id
            )
            .unwrap(),
        advanced
    );
}

#[test]
fn changed_candidate_or_unmentioned_worktree_cannot_be_associated() {
    let f = fixture("candidate-changed");
    f.candidate
        .with_immediate_transaction(|tx| {
            tx.execute(
                "UPDATE projects SET current_task_id = NULL WHERE project_id = ?1",
                AC_params![f.request.candidate_project_id],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(f.apply(&f.request).is_err());
    let f = fixture("worktree");
    f.storage
        .bootstrap_provisional(
            &f.authority,
            f.authority.effective_uid(),
            &[41; 32],
            &[43; 32],
            "same-name",
        )
        .unwrap();
    assert!(f.apply(&f.request).is_err());
    assert_eq!(f.association_count(), 0);
}

#[path = "Migration_Identity_Recovery_test.rs"]
pub(crate) mod recovery;
