use super::*;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ant_delirium::structs::CanonicalStateStore;
use serde_json::json;

#[test]
fn cutover_request_requires_at_least_thirty_days_of_rollback() {
    let mut request = fixture_request();
    assert_eq!(
        validate_request(&request, &"9".repeat(64), 1_787_209_000),
        Ok(())
    );
    request.rollback_window_seconds = MINIMUM_ROLLBACK_WINDOW_SECONDS - 1;
    assert_eq!(
        validate_request(&request, &"9".repeat(64), 1_787_209_000),
        Err(LegacyCutoverError::Request)
    );
}

#[test]
fn cutover_manifest_opens_rollback_without_changing_approval_evidence() {
    let request = fixture_request();
    let approved = fixture_manifest(&request);
    let binding = fixture_binding(&request);
    let cutover = cutover_manifest_value(&approved, &request, &binding).unwrap();
    assert_eq!(cutover["state"], "cutover");
    assert_eq!(cutover["cutover_at"], binding.cutover_at);
    assert_eq!(cutover["rollback_deadline"], binding.rollback_deadline);
    assert_eq!(
        cutover["rollback"]["post_cutover_first_sequence"],
        binding.post_cutover_first_sequence
    );
    assert_eq!(cutover["rollback"]["status"], "available");
    assert_eq!(cutover["approvals"], approved["approvals"]);
    assert_eq!(
        cutover_manifest_value(&cutover, &request, &binding).unwrap(),
        cutover
    );
}

#[test]
fn cutover_binding_and_receipt_are_path_free() {
    let request = fixture_request();
    let binding = fixture_binding(&request);
    let serialized = serde_json::to_vec(&binding).unwrap();
    ac_ant_delirium::structs::validate_serialized_public_document(
        &serialized,
        "migration cutover binding",
    )
    .unwrap();
    let text = String::from_utf8(serialized).unwrap();
    for forbidden in ["/home/", "/var/", "/tmp/", "state.db", "project_key"] {
        assert!(!text.contains(forbidden));
    }
}

#[test]
fn preexisting_registry_project_cannot_be_claimed_without_the_exact_binding() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/legacy-cutover-tests/unbound-registry-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let registry = ProjectDatabase::initialize_at(&root).unwrap();
    let request = fixture_request();
    let project = StorageProject {
        project_id: request.project_id.clone(),
        project_key: "fixture--00000000000000000000000000".to_string(),
        created_at: "migration-candidate".to_string(),
    };
    let workspace = StorageWorkspace {
        workspace_id: request.workspace_id.clone(),
        project_id: request.project_id.clone(),
        workspace_key: "workspace--00000000000000000000000000".to_string(),
        created_at: "migration-candidate".to_string(),
    };
    registry.ensure_project(&project).unwrap();
    CanonicalStateStore::register_workspace(&registry, &workspace).unwrap();
    let binding = fixture_binding(&request);
    let binding_sha256 = lowercase_hex(&sha256(&serde_json::to_vec(&binding).unwrap()));
    assert_eq!(
        register_canonical_authority(&registry, &project, &workspace, &binding, &binding_sha256,),
        Err(LegacyCutoverError::Storage)
    );
    assert_eq!(registry.event_count(&request.project_id).unwrap(), 0);
}

#[test]
#[ignore = "requires the operator-provided T8.6 approved candidate and controlled service storage"]
fn cuts_over_the_real_t86_candidate_in_controlled_storage() {
    let service_root = PathBuf::from(
        std::env::var_os("ANTDELIRIUM_T86_SERVICE_ROOT").expect("controlled service root"),
    );
    let staging_parent = service_root.join("migration-staging");
    let projects_parent = service_root.join("projects");
    let registry = ProjectDatabase::open_existing_at(&service_root.join("registry")).unwrap();
    let request = cutover_request(
        std::env::var("ANTDELIRIUM_T86_MIGRATION_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_PROJECT_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_WORKSPACE_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_APPROVED_MANIFEST_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_SOURCE_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_STAGING_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_CONVERSION_REPORT_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_ATTESTATION_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_ATTESTATION_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_APPROVAL_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T86_APPROVAL_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_CANDIDATE_LOGICAL_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T86_SOURCE_SHA256").unwrap(),
        MINIMUM_ROLLBACK_WINDOW_SECONDS,
    );
    let receipt = cutover_legacy_candidate(
        &staging_parent,
        &projects_parent,
        &registry,
        &request,
        &std::env::var("ANTDELIRIUM_T86_SERVICE_SUBJECT_SHA256").unwrap(),
        1_787_209_100,
    )
    .unwrap();
    assert!(receipt.cutover_performed);
    assert!(receipt.origin_preserved);
    assert_eq!(receipt.rollback_status, "available");
    assert_eq!(receipt.manifest_state, "cutover");
    println!("{}", serde_json::to_string(&receipt).unwrap());
}

fn fixture_request() -> LegacyCutoverRequest {
    cutover_request(
        "mig_00000000000000000000000000".to_string(),
        "prj_00000000000000000000000000".to_string(),
        "wsp_00000000000000000000000000".to_string(),
        "1".repeat(64),
        "2".repeat(64),
        "3".repeat(64),
        "4".repeat(64),
        "att_00000000000000000000000000".to_string(),
        "5".repeat(64),
        "apr_00000000000000000000000000".to_string(),
        "6".repeat(64),
        "7".repeat(64),
        "2".repeat(64),
        MINIMUM_ROLLBACK_WINDOW_SECONDS,
    )
}

fn fixture_binding(request: &LegacyCutoverRequest) -> MigrationCutoverBinding {
    MigrationCutoverBinding {
        contract_version: CUTOVER_BINDING_CONTRACT.to_string(),
        cutover_id: deterministic_id("cut_", &request.approval_id, &request.migration_id),
        migration_id: request.migration_id.clone(),
        project_id: request.project_id.clone(),
        workspace_id: request.workspace_id.clone(),
        service_subject_sha256: "9".repeat(64),
        cutover_request_sha256: "a".repeat(64),
        approved_manifest_sha256: request.expected_approved_manifest_sha256.clone(),
        approval_id: request.approval_id.clone(),
        approval_sha256: request.expected_approval_sha256.clone(),
        attestation_id: request.attestation_id.clone(),
        attestation_sha256: request.expected_attestation_sha256.clone(),
        candidate_logical_state_sha256: request.expected_candidate_logical_state_sha256.clone(),
        source_snapshot_sha256: request.expected_source_snapshot_sha256.clone(),
        staging_snapshot_sha256: request.expected_staging_snapshot_sha256.clone(),
        rollback_legacy_snapshot_sha256: request.rollback_legacy_snapshot_sha256.clone(),
        cutover_at_unix: 1_787_209_100,
        cutover_at: "2026-08-20T07:25:00Z".to_string(),
        rollback_deadline_unix: 1_789_801_100,
        rollback_deadline: "2026-09-19T07:25:00Z".to_string(),
        post_cutover_first_sequence: 3,
    }
}

fn fixture_manifest(request: &LegacyCutoverRequest) -> Value {
    json!({
        "contract_version": "migration-manifest-v1",
        "migration_id": request.migration_id,
        "project_id": request.project_id,
        "source_kind": "legacy_plugin",
        "source_snapshot_sha256": request.expected_source_snapshot_sha256,
        "staging_snapshot_sha256": request.expected_staging_snapshot_sha256,
        "source_schema_version": "legacy-source-inventory-v1",
        "target_schema_version": "canonical-state-v1",
        "counts": {
            "plans": 1, "tasks": 2, "submissions": 0, "reviews": 0,
            "history_events": 3, "problems": 1, "transitions": 1
        },
        "state": "approved",
        "created_at": "2026-08-20T05:00:00Z",
        "approved_at": "2026-08-20T06:00:00Z",
        "cutover_at": null,
        "rollback_deadline": null,
        "approvals": [
            {
                "role": "verifier", "subject_id": "8".repeat(64),
                "decision": "attested", "evidence_sha256": request.expected_attestation_sha256,
                "decided_at": "2026-08-20T05:30:00Z"
            },
            {
                "role": "operator", "subject_id": "9".repeat(64),
                "decision": "approved", "evidence_sha256": "a".repeat(64),
                "decided_at": "2026-08-20T06:00:00Z"
            }
        ],
        "rollback": {
            "legacy_snapshot_sha256": request.rollback_legacy_snapshot_sha256,
            "post_cutover_first_sequence": null,
            "status": "prepared"
        }
    })
}
