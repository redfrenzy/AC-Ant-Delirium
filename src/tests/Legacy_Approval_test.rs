use super::*;

use std::path::PathBuf;

use crate::legacy_attestation::LegacyAttestationChecks;
use crate::legacy_conversion::LegacyConversionCounts;

#[test]
fn approval_request_binds_attestation_candidate_manifest_and_rollback() {
    let request = fixture_request();
    assert_eq!(request.contract_version, "migration-approval-request-v1");
    assert_eq!(
        request.rollback_legacy_snapshot_sha256,
        request.expected_source_snapshot_sha256
    );
    assert_ne!(
        request.expected_manifest_snapshot_sha256,
        request.expected_attestation_sha256
    );
}

#[test]
fn manifest_transition_records_attestation_and_operator_without_cutover_deadline() {
    let request = fixture_request();
    let attestation = fixture_attestation(&request);
    let operator = "8".repeat(64);
    let request_sha = "9".repeat(64);
    let manifest = fixture_manifest(&request);
    let approved = approved_manifest_value(
        &manifest,
        &request,
        &attestation,
        &operator,
        &request_sha,
        "2026-08-20T22:16:40Z",
    )
    .unwrap();
    assert_eq!(approved["state"], "approved");
    assert_eq!(approved["approved_at"], "2026-08-20T22:16:40Z");
    assert!(approved["cutover_at"].is_null());
    assert!(approved["rollback_deadline"].is_null());
    assert_eq!(approved["rollback"]["status"], "prepared");
    assert_eq!(approved["approvals"][0]["role"], "verifier");
    assert_eq!(approved["approvals"][0]["decision"], "attested");
    assert_eq!(approved["approvals"][1]["role"], "operator");
    assert_eq!(approved["approvals"][1]["decision"], "approved");
    assert_eq!(
        approved_manifest_value(
            &approved,
            &request,
            &attestation,
            &operator,
            &request_sha,
            "2026-08-20T22:16:40Z",
        )
        .unwrap(),
        approved
    );
    let mut tampered = approved.clone();
    tampered["approvals"][1]["evidence_sha256"] = Value::String("0".repeat(64));
    assert!(approved_manifest_value(
        &tampered,
        &request,
        &attestation,
        &operator,
        &request_sha,
        "2026-08-20T22:16:40Z",
    )
    .is_err());
}

#[test]
#[ignore = "requires the operator-provided T8.6 attested candidate locator"]
fn approves_the_real_t86_candidate_without_cutover() {
    let staging_parent = PathBuf::from(
        std::env::var_os("ANTDELIRIUM_T86_STAGING_PARENT").expect("T8.6 staging parent"),
    );
    let request = approval_request(
        std::env::var("ANTDELIRIUM_T86_MIGRATION_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_PROJECT_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_WORKSPACE_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_MANIFEST_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_SOURCE_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_STAGING_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_CONVERSION_REPORT_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_ATTESTATION_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_ATTESTATION_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_CANDIDATE_LOGICAL_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_SOURCE_SHA256").unwrap(),
    );
    let receipt = approve_legacy_candidate(
        &staging_parent,
        &request,
        &std::env::var("ANTDELIRIUM_T86_OPERATOR_SUBJECT_SHA256").unwrap(),
        1_787_209_000,
    )
    .unwrap();
    assert_eq!(receipt.operator_decision, "approved");
    assert_eq!(receipt.verifier_decision, "attested");
    assert_eq!(receipt.manifest_state, "approved");
    assert!(receipt.rollback_deadline.is_none());
    assert!(!receipt.cutover_performed);
    println!("{}", serde_json::to_string(&receipt).unwrap());
}

fn fixture_request() -> LegacyApprovalRequest {
    approval_request(
        "mig_00000000000000000000000000".to_string(),
        "prj_00000000000000000000000000".to_string(),
        "wsp_00000000000000000000000000".to_string(),
        "1".repeat(64),
        "2".repeat(64),
        "3".repeat(64),
        "4".repeat(64),
        "att_00000000000000000000000000".to_string(),
        "5".repeat(64),
        "6".repeat(64),
        "2".repeat(64),
    )
}

fn fixture_counts() -> LegacyConversionCounts {
    LegacyConversionCounts {
        plans: 1,
        tasks: 2,
        submissions: 0,
        reviews: 0,
        history_events: 3,
        problems: 1,
        transitions: 1,
    }
}

fn fixture_attestation(request: &LegacyApprovalRequest) -> LegacyVerifierAttestation {
    LegacyVerifierAttestation {
        contract_version: "migration-verifier-attestation-v1".to_string(),
        attestation_id: request.attestation_id.clone(),
        migration_id: request.migration_id.clone(),
        project_id: request.project_id.clone(),
        workspace_id: request.workspace_id.clone(),
        verifier_subject_sha256: "7".repeat(64),
        conversion_report_sha256: request.expected_conversion_report_sha256.clone(),
        manifest_snapshot_sha256: request.expected_manifest_snapshot_sha256.clone(),
        source_snapshot_sha256: request.expected_source_snapshot_sha256.clone(),
        staging_snapshot_sha256: request.expected_staging_snapshot_sha256.clone(),
        normalized_snapshot_sha256: "a".repeat(64),
        semantic_snapshot_sha256: "b".repeat(64),
        dry_run_report_sha256: "c".repeat(64),
        identity_bindings_sha256: "d".repeat(64),
        current_task_projection_sha256: "e".repeat(64),
        governance_errors_projection_sha256: "f".repeat(64),
        candidate_logical_state_sha256: request.expected_candidate_logical_state_sha256.clone(),
        target_storage_schema_version: STORAGE_SCHEMA_VERSION,
        verified_counts: fixture_counts(),
        checks: LegacyAttestationChecks {
            conversion_revalidation: "passed".to_string(),
            storage_integrity: "passed".to_string(),
            logical_state_fingerprint: "passed".to_string(),
            opaque_identity_bindings: "passed".to_string(),
            projection_nondisclosure: "passed".to_string(),
            domain_count_parity: "passed".to_string(),
        },
        authentication: "ci_capability".to_string(),
        signature: "absent".to_string(),
        decision: "attested".to_string(),
        manifest_state: "staged".to_string(),
        approval_created: false,
        cutover_performed: false,
        attested_at: "unix:1787207200".to_string(),
        state: "verifier_attested".to_string(),
    }
}

fn fixture_manifest(request: &LegacyApprovalRequest) -> Value {
    json!({
        "contract_version": "migration-manifest-v1",
        "migration_id": request.migration_id,
        "project_id": request.project_id,
        "source_kind": "legacy_plugin",
        "source_snapshot_sha256": request.expected_source_snapshot_sha256,
        "staging_snapshot_sha256": request.expected_staging_snapshot_sha256,
        "source_schema_version": "legacy-source-inventory-v1",
        "target_schema_version": "canonical-state-v1",
        "counts": fixture_counts(),
        "state": "staged",
        "created_at": "2026-08-20T18:53:03Z",
        "approved_at": null,
        "cutover_at": null,
        "rollback_deadline": null,
        "approvals": [],
        "rollback": {
            "legacy_snapshot_sha256": request.rollback_legacy_snapshot_sha256,
            "post_cutover_first_sequence": null,
            "status": "prepared"
        }
    })
}
