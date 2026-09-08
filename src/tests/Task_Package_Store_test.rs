use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ant_delirium::structs::{
    CanonicalStateStore, ProjectDatabase, ProjectStateRoot, StorageProject, StorageWorkspace,
    STORAGE_SCHEMA_VERSION,
};

use super::*;
use crate::task_package_contracts::{
    TaskGenerationSource, TaskGenerationSourcePlan, TaskGenerationSourceTask,
    TASK_GENERATION_SOURCE_V1, TASK_PACKAGE_APPROVAL_V1, TASK_PACKAGE_ATTESTATION_V1,
};
use crate::task_package_generator::generate_task_package_proposal;

const PROJECT_ID: &str = "prj_00000000000000000000000000";
const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";

#[path = "Task_Package_Install_test.rs"]
mod installation;

fn digest(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn unique_test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/task-package-store-tests")
        .join(format!(
            "antdelirium-task-package-store-{label}-{}-{nonce}",
            std::process::id()
        ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn initialize_database(root: &Path) -> ProjectDatabase {
    let state_root = ProjectStateRoot::with_default_config(
        root.join("protected/project/workspace"),
        root.join("project"),
        root.join("workspace"),
    );
    let database = ProjectDatabase::initialize(&state_root).unwrap();
    database
        .ensure_project(&StorageProject {
            project_id: PROJECT_ID.to_string(),
            project_key: "project-key".to_string(),
            created_at: "unix:1".to_string(),
        })
        .unwrap();
    database
        .register_workspace(&StorageWorkspace {
            workspace_id: WORKSPACE_ID.to_string(),
            project_id: PROJECT_ID.to_string(),
            workspace_key: "workspace-key".to_string(),
            created_at: "unix:1".to_string(),
        })
        .unwrap();
    database
}

fn source(expected_state_revision: i64) -> TaskGenerationSource {
    TaskGenerationSource {
        contract_version: TASK_GENERATION_SOURCE_V1.to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        source_format: "ordered-objectives-v1".to_string(),
        source_sha256: digest('a'),
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        expected_state_revision,
        plans: vec![TaskGenerationSourcePlan {
            source_key_sha256: digest('b'),
            objective: "Persist separate decisions".to_string(),
            tasks: vec![TaskGenerationSourceTask {
                source_key_sha256: digest('c'),
                objective: "Keep proposal immutable".to_string(),
                acceptance_criteria: vec!["Replay preserves exact bytes".to_string()],
                relevant_paths: vec!["src/lib.rs".to_string()],
            }],
        }],
    }
}

fn attestation(proposal: &TaskPackageProposal) -> TaskPackageAttestation {
    TaskPackageAttestation {
        contract_version: TASK_PACKAGE_ATTESTATION_V1.to_string(),
        attestation_id: "att_11111111111111111111111111".to_string(),
        proposal_id: proposal.proposal_id.clone(),
        proposal_sha256: proposal.payload_sha256.clone(),
        source_sha256: proposal.source_sha256.clone(),
        project_id: proposal.project_id.clone(),
        workspace_id: proposal.workspace_id.clone(),
        expected_state_revision: proposal.expected_state_revision,
        verifier_subject_sha256: digest('d'),
        decision: "accepted".to_string(),
        attested_at: "unix:2".to_string(),
        state: "attested".to_string(),
    }
}

fn approval(
    proposal: &TaskPackageProposal,
    attestation: &TaskPackageAttestation,
    attestation_sha256: &str,
) -> TaskPackageApproval {
    TaskPackageApproval {
        contract_version: TASK_PACKAGE_APPROVAL_V1.to_string(),
        approval_id: "apr_22222222222222222222222222".to_string(),
        proposal_id: proposal.proposal_id.clone(),
        proposal_sha256: proposal.payload_sha256.clone(),
        attestation_id: attestation.attestation_id.clone(),
        attestation_sha256: attestation_sha256.to_string(),
        project_id: proposal.project_id.clone(),
        workspace_id: proposal.workspace_id.clone(),
        expected_state_revision: proposal.expected_state_revision,
        operator_subject_sha256: digest('e'),
        decision: "approved".to_string(),
        approved_at: "unix:3".to_string(),
        state: "approved".to_string(),
    }
}

#[test]
fn proposal_attestation_and_approval_are_separate_reentrant_records() {
    let root = unique_test_root("chain");
    let database = initialize_database(&root);
    assert_eq!(
        database.health().unwrap().schema_version,
        STORAGE_SCHEMA_VERSION
    );
    let proposal = generate_task_package_proposal(&source(0)).unwrap();

    let prepared = store_task_package_proposal(&database, &proposal).unwrap();
    assert!(matches!(prepared, TaskPackageWriteOutcome::Inserted(_)));
    assert!(matches!(
        store_task_package_proposal(&database, &proposal).unwrap(),
        TaskPackageWriteOutcome::Replay(_)
    ));
    let (inspected, inspection_receipt) = inspect_task_package_proposal(
        &database,
        "req_11111111111111111111111111",
        PROJECT_ID,
        WORKSPACE_ID,
        0,
        &proposal.proposal_id,
    )
    .unwrap();
    assert_eq!(inspected, proposal);
    assert_eq!(inspection_receipt.operation, "inspect");
    assert_eq!(inspection_receipt.previous_state_revision, 0);
    assert_eq!(inspection_receipt.resulting_state_revision, 0);

    let attestation = attestation(&proposal);
    let attested = store_task_package_attestation(&database, &attestation).unwrap();
    assert!(matches!(attested, TaskPackageWriteOutcome::Inserted(_)));
    assert!(matches!(
        store_task_package_attestation(&database, &attestation).unwrap(),
        TaskPackageWriteOutcome::Replay(_)
    ));

    let approval = approval(&proposal, &attestation, &attested.receipt().artifact_sha256);
    let approved = store_task_package_approval(&database, &approval).unwrap();
    assert!(matches!(approved, TaskPackageWriteOutcome::Inserted(_)));
    assert!(matches!(
        store_task_package_approval(&database, &approval).unwrap(),
        TaskPackageWriteOutcome::Replay(_)
    ));

    for outcome in [&prepared, &attested, &approved] {
        assert_eq!(
            outcome.receipt().previous_state_revision,
            outcome.receipt().resulting_state_revision
        );
        let receipt = serde_json::to_string(outcome.receipt()).unwrap();
        assert!(!receipt.contains("/home/"));
        assert!(!receipt.contains("state.db"));
    }
    let counts = database
        .with_immediate_transaction(|transaction| {
            Ok((
                transaction.query_row(
                    "SELECT count(*) FROM task_package_proposals",
                    [],
                    |row| row.get::<_, i64>(0),
                )?,
                transaction.query_row(
                    "SELECT count(*) FROM task_package_attestations",
                    [],
                    |row| row.get::<_, i64>(0),
                )?,
                transaction.query_row(
                    "SELECT count(*) FROM task_package_approvals",
                    [],
                    |row| row.get::<_, i64>(0),
                )?,
                transaction.query_row(
                    "SELECT state_revision FROM projects WHERE project_id = ?1",
                    AC_params![PROJECT_ID],
                    |row| row.get::<_, i64>(0),
                )?,
            ))
        })
        .unwrap();
    assert_eq!(counts, (1, 1, 1, 0));
    assert!(database
        .with_immediate_transaction(|transaction| {
            transaction.execute(
                "UPDATE task_package_proposals SET state = 'prepared' WHERE proposal_id = ?1",
                AC_params![proposal.proposal_id],
            )?;
            Ok(())
        })
        .is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn store_rejects_missing_predecessors_stale_scope_digest_changes_and_role_reuse() {
    let root = unique_test_root("reject");
    let database = initialize_database(&root);
    let proposal = generate_task_package_proposal(&source(0)).unwrap();
    let attestation = attestation(&proposal);
    assert_eq!(
        store_task_package_attestation(&database, &attestation),
        Err(TaskPackageStoreError::Conflict)
    );
    assert_eq!(
        store_task_package_proposal(
            &database,
            &generate_task_package_proposal(&source(1)).unwrap()
        ),
        Err(TaskPackageStoreError::Scope)
    );

    store_task_package_proposal(&database, &proposal).unwrap();
    let mut wrong_digest = attestation.clone();
    wrong_digest.proposal_sha256 = digest('f');
    assert_eq!(
        store_task_package_attestation(&database, &wrong_digest),
        Err(TaskPackageStoreError::Conflict)
    );
    let attested = store_task_package_attestation(&database, &attestation).unwrap();
    let mut reused_role = approval(&proposal, &attestation, &attested.receipt().artifact_sha256);
    reused_role.operator_subject_sha256 = attestation.verifier_subject_sha256.clone();
    assert_eq!(
        store_task_package_approval(&database, &reused_role),
        Err(TaskPackageStoreError::Conflict)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_identical_prepare_inserts_once_and_replays_the_same_receipt() {
    const WRITERS: usize = 6;
    let root = unique_test_root("concurrent");
    let database = Arc::new(initialize_database(&root));
    let proposal = Arc::new(generate_task_package_proposal(&source(0)).unwrap());
    let barrier = Arc::new(Barrier::new(WRITERS));
    let mut writers = Vec::new();
    for _ in 0..WRITERS {
        let database = Arc::clone(&database);
        let proposal = Arc::clone(&proposal);
        let barrier = Arc::clone(&barrier);
        writers.push(thread::spawn(move || {
            barrier.wait();
            store_task_package_proposal(&database, &proposal).unwrap()
        }));
    }
    let outcomes: Vec<_> = writers
        .into_iter()
        .map(|writer| writer.join().unwrap())
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, TaskPackageWriteOutcome::Inserted(_)))
            .count(),
        1
    );
    let first_receipt = outcomes[0].receipt();
    assert!(outcomes
        .iter()
        .all(|outcome| outcome.receipt() == first_receipt));
    fs::remove_dir_all(root).unwrap();
}
