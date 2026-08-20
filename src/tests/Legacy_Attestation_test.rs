use super::*;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ant_delirium::structs::{CanonicalStateStore, StorageProject, StorageWorkspace};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn attestation_request_binds_one_exact_candidate_scope() {
    let request = attestation_request(
        "mig_00000000000000000000000000".to_string(),
        "prj_00000000000000000000000000".to_string(),
        "wsp_00000000000000000000000000".to_string(),
        "a".repeat(64),
    );
    assert_eq!(request.contract_version, "migration-attestation-request-v1");
    assert_eq!(request.expected_conversion_report_sha256, "a".repeat(64));
}

#[test]
fn logical_candidate_fingerprint_is_stable_and_covers_protected_records() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/legacy-attestation-tests/logical-{}-{nonce}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    let database = ProjectDatabase::initialize_at(&root).unwrap();
    let project_id = "prj_00000000000000000000000000";
    let workspace_id = "wsp_00000000000000000000000000";
    let migration_id = "mig_00000000000000000000000000";
    database
        .ensure_project(&StorageProject {
            project_id: project_id.to_string(),
            project_key: "attestation-project".to_string(),
            created_at: "fixture".to_string(),
        })
        .unwrap();
    CanonicalStateStore::register_workspace(
        &database,
        &StorageWorkspace {
            workspace_id: workspace_id.to_string(),
            project_id: project_id.to_string(),
            workspace_key: "attestation-workspace".to_string(),
            created_at: "fixture".to_string(),
        },
    )
    .unwrap();
    database
        .with_immediate_transaction(|transaction| {
            transaction.execute(
                "INSERT INTO plans(plan_id, project_id, payload, created_at) VALUES (?1, ?2, ?3, 'fixture')",
                AC_params!["pln_00000000000000000000000000", project_id, b"plan".as_slice()],
            )?;
            transaction.execute(
                "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at) VALUES (?1, ?2, ?3, 1, 'available', 'not_required', ?4, 'fixture')",
                AC_params!["tsk_00000000000000000000000000", project_id, "pln_00000000000000000000000000", b"task".as_slice()],
            )?;
            transaction.execute(
                "INSERT INTO legacy_migration_records(project_id, migration_id, record_kind, ordinal, source_key_sha256, payload_sha256, payload) VALUES (?1, ?2, 'history', 0, ?3, ?4, ?5)",
                AC_params![project_id, migration_id, "a".repeat(64), lowercase_hex(&sha256(b"history")), b"history".as_slice()],
            )?;
            transaction.execute(
                "UPDATE projects SET active_plan_id = ?2, state_revision = 1 WHERE project_id = ?1",
                AC_params![project_id, "pln_00000000000000000000000000"],
            )?;
            Ok(())
        })
        .unwrap();
    database
        .initialize_empty_governance_snapshot(project_id, workspace_id, "fixture")
        .unwrap();
    let counts = LegacyConversionCounts {
        plans: 1,
        tasks: 1,
        submissions: 0,
        reviews: 0,
        history_events: 1,
        problems: 0,
        transitions: 0,
    };
    let first = database
        .with_immediate_transaction(|transaction| {
            logical_state_sha256(transaction, project_id, workspace_id, migration_id, &counts)
        })
        .unwrap();
    let second = database
        .with_immediate_transaction(|transaction| {
            logical_state_sha256(transaction, project_id, workspace_id, migration_id, &counts)
        })
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
    database
        .with_immediate_transaction(|transaction| {
            let changed_payload = b"changed history";
            transaction.execute(
                "UPDATE legacy_migration_records SET payload_sha256 = ?3, payload = ?4 WHERE project_id = ?1 AND migration_id = ?2",
                AC_params![project_id, migration_id, lowercase_hex(&sha256(changed_payload)), changed_payload],
            )?;
            Ok(())
        })
        .unwrap();
    let changed = database
        .with_immediate_transaction(|transaction| {
            logical_state_sha256(transaction, project_id, workspace_id, migration_id, &counts)
        })
        .unwrap();
    assert_ne!(first, changed);
    database
        .with_immediate_transaction(|transaction| {
            transaction.execute(
                "UPDATE legacy_migration_records SET payload = ?3 WHERE project_id = ?1 AND migration_id = ?2",
                AC_params![project_id, migration_id, b"tampered without digest".as_slice()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(database
        .with_immediate_transaction(|transaction| {
            logical_state_sha256(transaction, project_id, workspace_id, migration_id, &counts)
        })
        .is_err());
}

#[test]
#[ignore = "requires the operator-provided T8.6 converted candidate locator"]
fn attests_the_real_t86_candidate_without_approval_or_cutover() {
    let staging_parent = PathBuf::from(
        std::env::var_os("ANTDELIRIUM_T86_STAGING_PARENT").expect("T8.6 staging parent"),
    );
    let request = attestation_request(
        std::env::var("ANTDELIRIUM_T86_MIGRATION_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_PROJECT_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_WORKSPACE_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_CONVERSION_REPORT_SHA256").unwrap(),
    );
    let receipt = attest_legacy_candidate(
        &staging_parent,
        &request,
        &std::env::var("ANTDELIRIUM_T86_VERIFIER_SUBJECT_SHA256").unwrap(),
        1_787_207_200,
    )
    .unwrap();
    assert_eq!(receipt.attestation.decision, "attested");
    assert_eq!(receipt.attestation.signature, "absent");
    assert_eq!(receipt.attestation.manifest_state, "staged");
    assert!(!receipt.attestation.approval_created);
    assert!(!receipt.attestation.cutover_performed);
    println!("{}", serde_json::to_string(&receipt).unwrap());
}
