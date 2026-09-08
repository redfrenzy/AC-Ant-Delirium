use super::*;
use crate::task_package_contracts::{GovernanceTaskPackageV2, GOVERNANCE_TASK_PACKAGE_V2};
use crate::task_package_install::{install_approved_task_package, installation_receipt};
use ac_ant_delirium::structs::{
    CanonicalTaskAdvance, NewAuditEvent, NewReview, NewSubmission, ReviewDecision, ReviewStore,
    SubmissionStore, AUDIT_EVENT_CONTRACT_VERSION, SUBMISSION_CONTRACT_VERSION,
};

fn prepared_fixture(label: &str) -> (ProjectDatabase, GovernanceTaskPackageV2) {
    let root = unique_test_root(label);
    let database = initialize_database(&root);
    database
        .initialize_empty_governance_snapshot(PROJECT_ID, WORKSPACE_ID, "unix:1")
        .unwrap();
    let mut input = source(0);
    let mut second_task = input.plans[0].tasks[0].clone();
    second_task.source_key_sha256 = digest('d');
    second_task.objective = "Finish the first plan".to_string();
    input.plans[0].tasks.push(second_task);
    let mut second_plan = input.plans[0].clone();
    second_plan.source_key_sha256 = digest('e');
    second_plan.objective = "Continue the second plan".to_string();
    second_plan.tasks[0].source_key_sha256 = digest('f');
    second_plan.tasks[1].source_key_sha256 = digest('1');
    input.plans.push(second_plan);
    let proposal = generate_task_package_proposal(&input).unwrap();
    store_task_package_proposal(&database, &proposal).unwrap();
    let attestation = attestation(&proposal);
    let attested = store_task_package_attestation(&database, &attestation).unwrap();
    let approval = approval(&proposal, &attestation, &attested.receipt().artifact_sha256);
    let approved = store_task_package_approval(&database, &approval).unwrap();
    let package = GovernanceTaskPackageV2 {
        contract_version: GOVERNANCE_TASK_PACKAGE_V2.to_string(),
        request_id: "req_44444444444444444444444444".to_string(),
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        expected_state_revision: 0,
        proposal_id: proposal.proposal_id,
        proposal_sha256: proposal.payload_sha256,
        attestation_id: attestation.attestation_id,
        attestation_sha256: attested.receipt().artifact_sha256.clone(),
        approval_id: approval.approval_id,
        approval_sha256: approved.receipt().artifact_sha256.clone(),
        active_plan_id: proposal.plans[0].plan_id.clone(),
        current_task_id: proposal.plans[0].tasks[0].task_id.clone(),
        plans: proposal.plans,
    };
    (database, package)
}

fn event(index: usize, kind: &str, resource: &str, fingerprint: &str) -> NewAuditEvent {
    NewAuditEvent {
        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
        event_id: format!("evt_{index:026}"),
        project_id: PROJECT_ID.to_string(),
        workspace_id: Some(WORKSPACE_ID.to_string()),
        actor_kind: "ci".to_string(),
        actor_subject_id: "synthetic-authorized-ci".to_string(),
        event_kind: kind.to_string(),
        outcome: "succeeded".to_string(),
        occurred_at: "unix:4".to_string(),
        fingerprint: fingerprint.to_string(),
        correlation_id: format!("ref_{index:026}"),
        resource_id: Some(resource.to_string()),
        payload: b"synthetic task package event".to_vec(),
    }
}

fn installation_event(package: &GovernanceTaskPackageV2) -> NewAuditEvent {
    let receipt = installation_receipt(package).unwrap();
    event(
        1,
        "state_transitioned",
        &receipt.artifact_id,
        &receipt.artifact_sha256,
    )
}

#[test]
fn approved_package_installs_atomically_replays_and_advances_across_plans() {
    let (database, package) = prepared_fixture("v2-cycle");
    let install_event = installation_event(&package);
    let installed = install_approved_task_package(&database, &package, &install_event).unwrap();
    assert!(matches!(installed, TaskPackageWriteOutcome::Inserted(_)));
    assert!(matches!(
        install_approved_task_package(&database, &package, &install_event).unwrap(),
        TaskPackageWriteOutcome::Replay(_)
    ));
    let ordered: Vec<(String, i64)> = database
        .with_immediate_transaction(|tx| {
            let mut query =
                tx.prepare("SELECT plan_id, plan_ordinal FROM plans ORDER BY plan_ordinal")?;
            let rows = query
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .unwrap();
    assert_eq!(
        ordered,
        package
            .plans
            .iter()
            .map(|plan| (plan.plan_id.clone(), plan.plan_ordinal))
            .collect::<Vec<_>>()
    );
    let ordered_tasks: Vec<_> = package.plans.iter().flat_map(|plan| &plan.tasks).collect();
    for (index, task) in ordered_tasks.iter().enumerate() {
        let state = database
            .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
            .unwrap();
        assert_eq!(
            state.current_task_id.as_deref(),
            Some(task.task_id.as_str())
        );
        assert_eq!(
            state.active_plan_id.as_deref(),
            Some(package.plans[index / 2].plan_id.as_str())
        );
        let projected: serde_json::Value = serde_json::from_slice(
            &database
                .render_current_task_projection(PROJECT_ID, WORKSPACE_ID, "unix:5")
                .unwrap(),
        )
        .unwrap();
        assert!(serde_json::to_string(&projected)
            .unwrap()
            .contains(&task.objective));
        let submission_id = format!("sub_{index:026}");
        let submission = NewSubmission {
            contract_version: SUBMISSION_CONTRACT_VERSION.to_string(),
            submission_id: submission_id.clone(),
            request_id: format!("req_{index:026}"),
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: task.task_id.clone(),
            task_revision: 1,
            base_revision: None,
            fingerprint: format!("{:064x}", index * 3 + 10),
            payload: b"synthetic submitted evidence".to_vec(),
            received_at: "unix:5".to_string(),
        };
        let mut submission_event = event(
            index * 3 + 10,
            "submission_recorded",
            &submission_id,
            &submission.fingerprint,
        );
        submission_event.actor_kind = "agent".to_string();
        database
            .record_submission(&submission, &submission_event)
            .unwrap();
        let review_id = format!("rev_{index:026}");
        let review = NewReview {
            review_id: review_id.clone(),
            project_id: PROJECT_ID.to_string(),
            submission_id,
            reviewer_id: "independent-reviewer".to_string(),
            decision: ReviewDecision::Accepted,
            fingerprint: format!("{:064x}", index * 3 + 11),
            payload: b"synthetic accepted review".to_vec(),
            created_at: "unix:6".to_string(),
        };
        let mut review_event = event(
            index * 3 + 11,
            "review_recorded",
            &review_id,
            &review.fingerprint,
        );
        review_event.actor_kind = "reviewer".to_string();
        review_event.actor_subject_id = review.reviewer_id.clone();
        database.record_review(&review, &review_event).unwrap();
        let advance = CanonicalTaskAdvance {
            project_id: PROJECT_ID.to_string(),
            workspace_id: WORKSPACE_ID.to_string(),
            task_id: task.task_id.clone(),
            review_id,
            expected_state_revision: state.state_revision,
        };
        let advanced = database
            .advance_accepted_task(
                &advance,
                &event(
                    index * 3 + 12,
                    "state_transitioned",
                    &task.task_id,
                    &format!("{:064x}", index * 3 + 12),
                ),
            )
            .unwrap();
        assert_eq!(
            advanced.next_task_id.as_deref(),
            ordered_tasks
                .get(index + 1)
                .map(|task| task.task_id.as_str())
        );
        assert!(matches!(
            install_approved_task_package(&database, &package, &install_event).unwrap(),
            TaskPackageWriteOutcome::Replay(_)
        ));
        assert_eq!(
            database
                .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
                .unwrap(),
            advanced.state
        );
    }
    let final_state = database
        .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
        .unwrap();
    assert!(final_state.active_plan_id.is_none() && final_state.current_task_id.is_none());
    assert_eq!(final_state.state_revision, 5);
    assert!(final_state
        .tasks
        .iter()
        .all(|task| task.status == "completed"));
    assert_eq!(database.health().unwrap().foreign_key_violations, 0);
}

#[test]
fn changed_decisions_scope_content_or_initial_selection_never_install() {
    let (database, package) = prepared_fixture("v2-invalid");
    let baseline = database
        .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
        .unwrap();
    let mut variants = Vec::new();
    let mut changed = package.clone();
    changed.approval_sha256 = digest('0');
    variants.push(changed);
    let mut changed = package.clone();
    changed.attestation_sha256 = digest('0');
    variants.push(changed);
    let mut changed = package.clone();
    changed.proposal_sha256 = digest('0');
    variants.push(changed);
    let mut changed = package.clone();
    changed.approval_id = "apr_99999999999999999999999999".to_string();
    variants.push(changed);
    let mut changed = package.clone();
    changed.workspace_id = "wsp_99999999999999999999999999".to_string();
    variants.push(changed);
    let mut changed = package.clone();
    changed.plans[0].tasks[0].objective = "Unapproved objective".to_string();
    variants.push(changed);
    for changed in variants {
        assert!(
            install_approved_task_package(&database, &changed, &installation_event(&changed))
                .is_err()
        );
        assert_eq!(
            database
                .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
                .unwrap(),
            baseline
        );
    }
    let mut skipped = package.clone();
    skipped.current_task_id = skipped.plans[0].tasks[1].task_id.clone();
    assert!(installation_receipt(&skipped).is_err());
    let mut skipped_plan = package.clone();
    skipped_plan.active_plan_id = skipped_plan.plans[1].plan_id.clone();
    skipped_plan.current_task_id = skipped_plan.plans[1].tasks[0].task_id.clone();
    assert!(installation_receipt(&skipped_plan).is_err());
    install_approved_task_package(&database, &package, &installation_event(&package)).unwrap();
    let mut new_request = package.clone();
    new_request.request_id = "req_99999999999999999999999999".to_string();
    assert!(install_approved_task_package(
        &database,
        &new_request,
        &installation_event(&new_request)
    )
    .is_err());
}

#[test]
fn failure_after_canonical_writes_rolls_back_everything_and_allows_retry() {
    let (database, package) = prepared_fixture("v2-rollback");
    let baseline = database
        .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
        .unwrap();
    database.with_immediate_transaction(|tx| {
        tx.execute_batch("CREATE TRIGGER simulate_installation_failure BEFORE INSERT ON task_package_installations BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END;")?; Ok(())
    }).unwrap();
    assert!(
        install_approved_task_package(&database, &package, &installation_event(&package)).is_err()
    );
    assert_eq!(
        database
            .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
            .unwrap(),
        baseline
    );
    assert_eq!(database.with_immediate_transaction(|tx| Ok(tx.query_row("SELECT (SELECT count(*) FROM plans) + (SELECT count(*) FROM events) + (SELECT count(*) FROM task_package_installations)", [], |row| row.get::<_, i64>(0))?)).unwrap(), 0);
    database
        .with_immediate_transaction(|tx| {
            tx.execute_batch("DROP TRIGGER simulate_installation_failure;")?;
            Ok(())
        })
        .unwrap();
    install_approved_task_package(&database, &package, &installation_event(&package)).unwrap();
}

#[test]
fn an_existing_plan_cannot_be_replaced_even_when_no_current_task_is_selected() {
    let (database, package) = prepared_fixture("v2-occupied");
    database.with_immediate_transaction(|tx| {
        tx.execute("INSERT INTO plans(plan_id, project_id, status, payload, created_at) VALUES ('pln_99999999999999999999999999', ?1, 'completed', ?2, 'unix:1')",
            AC_params![PROJECT_ID, b"preserved legacy plan".as_slice()])?; Ok(())
    }).unwrap();
    let before = database
        .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
        .unwrap();
    assert!(
        install_approved_task_package(&database, &package, &installation_event(&package)).is_err()
    );
    assert_eq!(
        database
            .load_canonical_state(PROJECT_ID, WORKSPACE_ID)
            .unwrap(),
        before
    );
    assert_eq!(
        database
            .with_immediate_transaction(|tx| Ok(tx.query_row(
                "SELECT payload FROM plans",
                [],
                |row| row.get::<_, Vec<u8>>(0)
            )?))
            .unwrap(),
        b"preserved legacy plan"
    );
}

#[test]
fn concurrent_installation_writes_once_and_returns_identical_receipts() {
    let (database, package) = prepared_fixture("v2-concurrent");
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let database = database.clone();
            let package = package.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                install_approved_task_package(&database, &package, &installation_event(&package))
                    .unwrap()
            })
        })
        .collect();
    let outcomes: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| matches!(result, TaskPackageWriteOutcome::Inserted(_)))
            .count(),
        1
    );
    assert_eq!(outcomes[0].receipt(), outcomes[1].receipt());
}
