use super::*;
use crate::task_package_contracts::{
    MigrationIdentityRollback, MigrationIdentityRollbackReceipt, MIGRATION_IDENTITY_ROLLBACK_V1,
};
use ac_ant_delirium::structs::{
    CanonicalTaskAdvance, CapabilityAuthorizationOutcome, CapabilityAuthorizationRequest,
    CapabilityChannel, CapabilityOperation, NewReview, NewSubmission, ReviewDecision, ReviewStore,
    SubmissionStore, SUBMISSION_CONTRACT_VERSION,
};

pub(crate) fn rollback_request(f: &Fixture) -> MigrationIdentityRollback {
    let association = association_receipt(&f.request).unwrap();
    MigrationIdentityRollback {
        contract_version: MIGRATION_IDENTITY_ROLLBACK_V1.to_string(),
        request_id: "req_00000000000000000000000007".to_string(),
        association_id: association.association_id,
        association_sha256: association.association_sha256,
        provisional_project_id: f.request.provisional_project_id.clone(),
        provisional_workspace_id: f.request.provisional_workspace_id.clone(),
        candidate_project_id: f.request.candidate_project_id.clone(),
        candidate_workspace_id: f.request.candidate_workspace_id.clone(),
        expected_registry_revision: 1,
        policy: "restore-provisional-preserve-candidate".to_string(),
    }
}

fn rollback(
    f: &Fixture,
    request: &MigrationIdentityRollback,
) -> TransactionalStorageResult<MigrationIdentityRollbackReceipt> {
    let receipt = crate::migration_identity_recovery::rollback_receipt(request)?;
    let mut event = event(
        &f.request.candidate_project_id,
        &f.request.candidate_workspace_id,
        70,
        "operator",
        "migration_applied",
        &receipt.rollback_id,
    );
    event.actor_subject_id =
        crate::service_storage::task_package_subject_sha256(f.authority.effective_uid());
    event.fingerprint = receipt.rollback_sha256;
    crate::migration_identity_recovery::rollback_identity(
        AssociationContext {
            registry: f.storage.registry(),
            provisional: &f.source,
            candidate: &f.candidate,
            peer_uid: f.authority.effective_uid(),
            service_subject_sha256: &f.service_subject,
        },
        request,
        &event,
    )
}

fn event(
    project: &str,
    workspace: &str,
    index: u32,
    actor: &str,
    kind: &str,
    resource: &str,
) -> NewAuditEvent {
    NewAuditEvent {
        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
        event_id: format!("evt_{index:026}"),
        project_id: project.to_string(),
        workspace_id: Some(workspace.to_string()),
        actor_kind: actor.to_string(),
        actor_subject_id: "fixture-reviewer".to_string(),
        event_kind: kind.to_string(),
        outcome: "succeeded".to_string(),
        occurred_at: "unix:5".to_string(),
        fingerprint: format!("{index:064x}"),
        correlation_id: format!("ref_{index:026}"),
        resource_id: Some(resource.to_string()),
        payload: b"preserved fixture evidence".to_vec(),
    }
}

fn finish_candidate_task(f: &Fixture) {
    let project = &f.request.candidate_project_id;
    let workspace = &f.request.candidate_workspace_id;
    let state = f
        .candidate
        .load_canonical_state(project, workspace)
        .unwrap();
    let task = state.current_task_id.as_ref().unwrap();
    let submission = NewSubmission {
        contract_version: SUBMISSION_CONTRACT_VERSION.to_string(),
        submission_id: "sub_00000000000000000000000001".to_string(),
        request_id: "req_00000000000000000000000001".to_string(),
        project_id: project.clone(),
        workspace_id: workspace.clone(),
        task_id: task.clone(),
        task_revision: 1,
        base_revision: None,
        fingerprint: format!("{:064x}", 51),
        payload: b"implementation evidence after association".to_vec(),
        received_at: "unix:5".to_string(),
    };
    f.candidate
        .record_submission(
            &submission,
            &event(
                project,
                workspace,
                51,
                "agent",
                "submission_recorded",
                &submission.submission_id,
            ),
        )
        .unwrap();
    let review = NewReview {
        review_id: "rev_00000000000000000000000001".to_string(),
        project_id: project.clone(),
        submission_id: submission.submission_id,
        reviewer_id: "fixture-reviewer".to_string(),
        decision: ReviewDecision::Accepted,
        fingerprint: format!("{:064x}", 52),
        payload: b"accepted evidence".to_vec(),
        created_at: "unix:6".to_string(),
    };
    f.candidate
        .record_review(
            &review,
            &event(
                project,
                workspace,
                52,
                "reviewer",
                "review_recorded",
                &review.review_id,
            ),
        )
        .unwrap();
    f.candidate
        .advance_accepted_task(
            &CanonicalTaskAdvance {
                project_id: project.clone(),
                workspace_id: workspace.clone(),
                task_id: task.clone(),
                review_id: review.review_id,
                expected_state_revision: state.state_revision,
            },
            &event(project, workspace, 53, "ci", "state_transitioned", task),
        )
        .unwrap();
}

fn snapshot_root(f: &Fixture, operation_id: &str) -> PathBuf {
    let (id, digest): (String, String) = f.storage.registry().with_immediate_transaction(|tx| {
        Ok(tx.query_row("SELECT snapshot_id, manifest_sha256 FROM migration_identity_recovery_points WHERE operation_id = ?1", AC_params![operation_id], |r| Ok((r.get(0)?, r.get(1)?)))?)
    }).unwrap();
    let root = f.root.join("registry/identity-backups").join(id);
    let bytes = std::fs::read(root.join("manifest.json")).unwrap();
    assert_eq!(
        crate::legacy_staging::lowercase_hex(&sha256(&bytes)),
        digest
    );
    let manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for entry in manifest["databases"].as_array().unwrap() {
        let file =
            std::fs::File::open(root.join(format!("{}.sqlite3", entry["scope"].as_str().unwrap())))
                .unwrap();
        assert_eq!(
            crate::legacy_staging::lowercase_hex(
                &ac_ant_delirium::libres::Sha2_unit::sha256_reader(file).unwrap()
            ),
            entry["sha256"].as_str().unwrap()
        );
    }
    root
}

#[test]
fn explicit_quarantine_and_rollback_preserve_both_histories_and_never_resurrect_capabilities() {
    let mut f = fixture("quarantine-rollback");
    let source_package = CanonicalTaskPackage {
        contract_version: GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION.to_string(),
        project_id: f.request.provisional_project_id.clone(),
        workspace_id: f.request.provisional_workspace_id.clone(),
        plan_id: "pln_00000000000000000000000001".to_string(),
        expected_state_revision: 0,
        tasks: vec![CanonicalTaskPackageTask {
            task_id: "tsk_00000000000000000000000001".to_string(),
            task_revision: 1,
            objective: "Preserve the original task".to_string(),
            acceptance_criteria: vec!["No task is merged or lost".to_string()],
            relevant_paths: vec!["src/lib.rs".to_string()],
        }],
    };
    f.source
        .install_task_package(
            &source_package,
            &event(
                &source_package.project_id,
                &source_package.workspace_id,
                50,
                "operator",
                "state_transitioned",
                &source_package.plan_id,
            ),
        )
        .unwrap();
    let source_before = f
        .source
        .load_canonical_state(&source_package.project_id, &source_package.workspace_id)
        .unwrap();
    assert!(f.apply(&f.request).is_err());
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
    f.request.provisional_state_policy = "quarantine-preserve".to_string();
    let associated = f.apply(&f.request).unwrap();
    assert_eq!(f.apply(&f.request).unwrap(), associated);
    let before = snapshot_root(&f, &associated.association_id);
    let snapshot = ac_ant_delirium::libres::Rusqlite_unit::AC_Connection::open_with_flags(
        before.join("provisional.sqlite3"),
        ac_ant_delirium::libres::Rusqlite_unit::AC_OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM tasks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    finish_candidate_task(&f);
    let candidate_before = f
        .candidate
        .load_canonical_state(
            &f.request.candidate_project_id,
            &f.request.candidate_workspace_id,
        )
        .unwrap();
    assert_eq!(candidate_before.tasks[0].status, "completed");
    let events_before = f
        .candidate
        .event_count(&f.request.candidate_project_id)
        .unwrap();
    let candidate_token = f
        .storage
        .bootstrap_provisional(
            &f.authority,
            f.authority.effective_uid(),
            &[41; 32],
            &[42; 32],
            "same-name",
        )
        .unwrap();
    let request = rollback_request(&f);
    let receipt = rollback(&f, &request).unwrap();
    assert_eq!(rollback(&f, &request).unwrap(), receipt);
    assert_eq!(receipt.resulting_registry_revision, 2);
    assert!(f.apply(&f.request).is_err());
    assert_eq!(
        f.source
            .load_canonical_state(&source_package.project_id, &source_package.workspace_id)
            .unwrap(),
        source_before
    );
    assert_eq!(
        f.candidate
            .load_canonical_state(
                &f.request.candidate_project_id,
                &f.request.candidate_workspace_id
            )
            .unwrap(),
        candidate_before
    );
    assert_eq!(
        f.candidate
            .event_count(&f.request.candidate_project_id)
            .unwrap(),
        events_before
    );
    snapshot_root(&f, &receipt.rollback_id);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for token in [old, candidate_token] {
        let request = CapabilityAuthorizationRequest {
            grant_id: token.compile.capability_id,
            presented_secret: token.compile.token,
            operation: CapabilityOperation::CompileObserve,
            project_id: Some(token.project_id),
            workspace_id: Some(token.workspace_id),
            task_id: None,
            presented_connection_nonce: [81; 32],
            request_id: "req_00000000000000000000000008".to_string(),
            request_fingerprint: [82; 32],
        };
        assert_eq!(
            f.storage
                .registry()
                .authorize_capability(
                    f.authority.effective_uid(),
                    CapabilityChannel::Limited,
                    &[81; 32],
                    now,
                    &request
                )
                .unwrap(),
            CapabilityAuthorizationOutcome::Denied
        );
    }
    let reopened = ProtectedServiceStorage::initialize(&f.authority, &f.root).unwrap();
    let active = reopened
        .bootstrap_provisional(
            &f.authority,
            f.authority.effective_uid(),
            &[41; 32],
            &[42; 32],
            "same-name",
        )
        .unwrap();
    assert_eq!(active.project_id, f.request.provisional_project_id);
    let projected = f
        .source
        .render_current_task_projection(&active.project_id, &active.workspace_id, "unix:8")
        .unwrap();
    assert!(String::from_utf8(projected)
        .unwrap()
        .contains("Preserve the original task"));
}

#[test]
fn rollback_conflicts_partial_failure_and_concurrency_leave_a_single_append_only_transition() {
    let f = fixture("rollback-atomic");
    f.apply(&f.request).unwrap();
    let request = rollback_request(&f);
    for field in [
        "association_sha256",
        "association_id",
        "candidate_workspace_id",
        "provisional_workspace_id",
        "expected_registry_revision",
        "policy",
    ] {
        let mut value = serde_json::to_value(&request).unwrap();
        value[field] = match field {
            "association_id" => "asn_00000000000000000000000009".into(),
            "candidate_workspace_id" | "provisional_workspace_id" => {
                "wsp_00000000000000000000000009".into()
            }
            "expected_registry_revision" => 0.into(),
            "policy" => "discard-candidate".into(),
            _ => "9".repeat(64).into(),
        };
        assert!(
            rollback(&f, &serde_json::from_value(value).unwrap()).is_err(),
            "{field}"
        );
    }
    f.storage.registry().with_immediate_transaction(|tx| {
        tx.execute_batch("CREATE TRIGGER fixture_fail_rollback BEFORE INSERT ON migration_identity_rollbacks BEGIN SELECT RAISE(ABORT, 'fixture'); END;")?; Ok(())
    }).unwrap();
    let revocations: i64 = f
        .storage
        .registry()
        .with_immediate_transaction(|tx| {
            Ok(tx.query_row("SELECT count(*) FROM revocations", [], |r| r.get(0))?)
        })
        .unwrap();
    let events = f
        .storage
        .registry()
        .event_count(&f.request.candidate_project_id)
        .unwrap();
    assert!(rollback(&f, &request).is_err());
    assert!(f
        .storage
        .registry()
        .project_identity_is_retired(&f.request.provisional_project_id)
        .unwrap());
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
            assert_eq!(
                tx.query_row("SELECT count(*) FROM revocations", [], |r| r
                    .get::<_, i64>(0))?,
                revocations
            );
            assert_eq!(
                tx.query_row(
                    "SELECT revision FROM migration_identity_registry_revision",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                1
            );
            assert_eq!(
                tx.query_row(
                    "SELECT count(*) FROM migration_identity_recovery_points",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                1
            );
            tx.execute_batch("DROP TRIGGER fixture_fail_rollback;")?;
            Ok(())
        })
        .unwrap();
    let barrier = std::sync::Barrier::new(2);
    let receipts = std::thread::scope(|scope| {
        let handles = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    rollback(&f, &request).unwrap()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(receipts[0], receipts[1]);
    assert_eq!(
        f.storage
            .registry()
            .event_count(&f.request.candidate_project_id)
            .unwrap(),
        events + 1
    );
    for sql in [
        "DELETE FROM migration_identity_rollbacks",
        "UPDATE migration_identity_rollbacks SET payload = X'7B7D'",
        "DELETE FROM migration_identity_recovery_points",
    ] {
        assert!(f
            .storage
            .registry()
            .with_immediate_transaction(|tx| {
                tx.execute(sql, [])?;
                Ok(())
            })
            .is_err());
    }
}

#[test]
fn missing_private_snapshot_destination_refuses_association_without_touching_state() {
    let f = fixture("snapshot-failure");
    let blocker = f.root.join("registry/identity-backups");
    std::fs::write(&blocker, b"preserve this sentinel").unwrap();
    assert!(f.apply(&f.request).is_err());
    assert_eq!(f.association_count(), 0);
    assert_eq!(std::fs::read(blocker).unwrap(), b"preserve this sentinel");
}

#[test]
fn storage_v18_preserves_v17_associations_and_can_rollback_without_an_old_recovery_point() {
    let f = fixture("v17-migration");
    let associated = f.apply(&f.request).unwrap();
    let original: Vec<u8> = f.storage.registry().with_immediate_transaction(|tx| {
        let view: String = tx.query_row("SELECT sql FROM sqlite_schema WHERE name = 'effective_project_registrations'", [], |r| r.get(0))?;
        tx.execute_batch("DROP VIEW effective_project_registrations; DROP VIEW active_migration_identity_associations;
            DROP TABLE migration_identity_rollbacks; DROP TABLE migration_identity_recovery_points;
            DELETE FROM schema_migrations WHERE version = 18; PRAGMA user_version = 17;")?;
        tx.execute_batch(&view.replace("active_migration_identity_associations", "migration_identity_associations"))?;
        Ok(tx.query_row("SELECT payload FROM migration_identity_associations", [], |r| r.get(0))?)
    }).unwrap();
    let reopened = ProjectDatabase::open_existing_at(&f.root.join("registry")).unwrap();
    assert_eq!(reopened.health().unwrap().schema_version, 18);
    reopened
        .with_immediate_transaction(|tx| {
            assert_eq!(
                tx.query_row(
                    "SELECT payload FROM migration_identity_associations",
                    [],
                    |r| r.get::<_, Vec<u8>>(0)
                )?,
                original
            );
            assert_eq!(
                tx.query_row(
                    "SELECT count(*) FROM active_migration_identity_associations",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                1
            );
            assert_eq!(
                tx.query_row(
                    "SELECT count(*) FROM migration_identity_rollbacks",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
            assert_eq!(
                tx.query_row(
                    "SELECT count(*) FROM migration_identity_recovery_points",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(f.apply(&f.request).unwrap(), associated);
    let receipt = rollback(&f, &rollback_request(&f)).unwrap();
    assert_eq!(receipt.project_id, f.request.provisional_project_id);
    snapshot_root(&f, &receipt.rollback_id);
}
