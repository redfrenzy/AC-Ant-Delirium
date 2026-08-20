use std::fs;
use std::path::Path;

use ac_ant_delirium::libres::Rusqlite_unit::{AC_OptionalExtension, AC_params};
use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::crockford_id;
use ac_ant_delirium::structs::{
    NewAuditEvent, ProjectDatabase, TransactionalStorageError, AUDIT_EVENT_CONTRACT_VERSION,
    STORAGE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::legacy_attestation::{logical_state_sha256, LegacyVerifierAttestation};
use crate::legacy_conversion::{read_control_file, LegacyConversionReport};
use crate::legacy_staging::{
    lowercase_hex, require_private_directory, rfc3339_utc_from_unix, sync_directory, valid_sha256,
    write_private_bytes,
};
use crate::limited_protocol::valid_opaque_id;

const APPROVAL_REQUEST_CONTRACT: &str = "migration-approval-request-v1";
const APPROVAL_RECEIPT_CONTRACT: &str = "migration-approval-receipt-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyApprovalError {
    Request,
    Candidate,
    Storage,
    Manifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyApprovalRequest {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) expected_manifest_snapshot_sha256: String,
    pub(crate) expected_source_snapshot_sha256: String,
    pub(crate) expected_staging_snapshot_sha256: String,
    pub(crate) expected_conversion_report_sha256: String,
    pub(crate) attestation_id: String,
    pub(crate) expected_attestation_sha256: String,
    pub(crate) expected_candidate_logical_state_sha256: String,
    pub(crate) rollback_legacy_snapshot_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyApprovalReceipt {
    pub(crate) contract_version: String,
    pub(crate) approval_id: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) operator_subject_sha256: String,
    pub(crate) approval_request_sha256: String,
    pub(crate) attestation_id: String,
    pub(crate) attestation_sha256: String,
    pub(crate) candidate_logical_state_sha256: String,
    pub(crate) manifest_before_sha256: String,
    pub(crate) manifest_after_sha256: String,
    pub(crate) source_snapshot_sha256: String,
    pub(crate) staging_snapshot_sha256: String,
    pub(crate) rollback_legacy_snapshot_sha256: String,
    pub(crate) approved_at: String,
    pub(crate) operator_decision: String,
    pub(crate) verifier_decision: String,
    pub(crate) rollback_status: String,
    pub(crate) rollback_deadline: Option<String>,
    pub(crate) manifest_state: String,
    pub(crate) cutover_performed: bool,
    pub(crate) state: String,
    pub(crate) approval_sha256: String,
}

pub(crate) fn approval_request(
    migration_id: String,
    project_id: String,
    workspace_id: String,
    expected_manifest_snapshot_sha256: String,
    expected_source_snapshot_sha256: String,
    expected_staging_snapshot_sha256: String,
    expected_conversion_report_sha256: String,
    attestation_id: String,
    expected_attestation_sha256: String,
    expected_candidate_logical_state_sha256: String,
    rollback_legacy_snapshot_sha256: String,
) -> LegacyApprovalRequest {
    LegacyApprovalRequest {
        contract_version: APPROVAL_REQUEST_CONTRACT.to_string(),
        migration_id,
        project_id,
        workspace_id,
        expected_manifest_snapshot_sha256,
        expected_source_snapshot_sha256,
        expected_staging_snapshot_sha256,
        expected_conversion_report_sha256,
        attestation_id,
        expected_attestation_sha256,
        expected_candidate_logical_state_sha256,
        rollback_legacy_snapshot_sha256,
    }
}

pub(crate) fn approve_legacy_candidate(
    staging_parent: &Path,
    request: &LegacyApprovalRequest,
    operator_subject_sha256: &str,
    trusted_now_unix: i64,
) -> Result<LegacyApprovalReceipt, LegacyApprovalError> {
    validate_request(request, operator_subject_sha256, trusted_now_unix)?;
    require_private_directory(staging_parent).map_err(|_| LegacyApprovalError::Candidate)?;
    let migration_root = staging_parent.join(&request.migration_id);
    require_private_directory(&migration_root).map_err(|_| LegacyApprovalError::Candidate)?;
    let manifest_path = migration_root.join("migration-manifest-v1.json");
    let manifest_bytes =
        read_control_file(&manifest_path).map_err(|_| LegacyApprovalError::Manifest)?;
    let manifest: Value =
        serde_json::from_slice(&manifest_bytes).map_err(|_| LegacyApprovalError::Manifest)?;
    validate_manifest_generation(&manifest, request)?;
    if manifest["state"] == "staged"
        && lowercase_hex(&sha256(&manifest_bytes)) != request.expected_manifest_snapshot_sha256
    {
        return Err(LegacyApprovalError::Manifest);
    }

    let converted_root = migration_root.join("converted");
    require_private_directory(&converted_root).map_err(|_| LegacyApprovalError::Candidate)?;
    let report_bytes = read_control_file(&converted_root.join("legacy-conversion-report-v1.json"))
        .map_err(|_| LegacyApprovalError::Candidate)?;
    if lowercase_hex(&sha256(&report_bytes)) != request.expected_conversion_report_sha256 {
        return Err(LegacyApprovalError::Candidate);
    }
    let report: LegacyConversionReport =
        serde_json::from_slice(&report_bytes).map_err(|_| LegacyApprovalError::Candidate)?;
    validate_report(&report, request)?;

    let database = ProjectDatabase::open_existing_at(&converted_root.join("candidate"))
        .map_err(|_| LegacyApprovalError::Storage)?;
    let health = database
        .health()
        .map_err(|_| LegacyApprovalError::Storage)?;
    if health.schema_version != STORAGE_SCHEMA_VERSION
        || health.quick_check != "ok"
        || health.foreign_key_violations != 0
        || health.trusted_schema
        || !health.foreign_keys
    {
        return Err(LegacyApprovalError::Storage);
    }
    let attestation = read_and_validate_attestation(&database, request, &report)?;
    let attested_at_unix = attestation
        .attested_at
        .strip_prefix("unix:")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(LegacyApprovalError::Candidate)?;
    if trusted_now_unix < attested_at_unix {
        return Err(LegacyApprovalError::Candidate);
    }
    let request_sha256 = lowercase_hex(&sha256(
        &serde_json::to_vec(request).map_err(|_| LegacyApprovalError::Request)?,
    ));
    let approval_id = deterministic_id(
        "apr_",
        &request.expected_attestation_sha256,
        operator_subject_sha256,
    );
    let event_id = deterministic_id("evt_", &approval_id, "migration-approval");
    let current_state = manifest["state"]
        .as_str()
        .ok_or(LegacyApprovalError::Manifest)?;
    let (receipt, approved_manifest_bytes) = database
        .with_immediate_transaction(|transaction| {
            let current_logical_sha256 = logical_state_sha256(
                transaction,
                &request.project_id,
                &request.workspace_id,
                &request.migration_id,
                &attestation.verified_counts,
            )?;
            if current_logical_sha256 != request.expected_candidate_logical_state_sha256 {
                return Err(integrity("migration approval candidate logical state differs"));
            }
            let existing = transaction
                .query_row(
                    "SELECT actor_kind, actor_subject_id, event_kind, outcome, occurred_at, fingerprint, workspace_id, resource_id, payload FROM events WHERE project_id = ?1 AND event_id = ?2",
                    AC_params![request.project_id, event_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, Option<String>>(6)?,
                            row.get::<_, Option<String>>(7)?,
                            row.get::<_, Vec<u8>>(8)?,
                        ))
                    },
                )
                .optional()?;
            if current_state == "approved" && existing.is_none() {
                return Err(integrity("approved migration manifest has no operator audit event"));
            }
            let approved_at = existing
                .as_ref()
                .map(|event| event.4.clone())
                .unwrap_or(rfc3339_utc_from_unix(trusted_now_unix).map_err(|_| {
                    integrity("migration approval trusted time is not representable")
                })?);
            let approved_manifest = approved_manifest_value(
                &manifest,
                request,
                &attestation,
                operator_subject_sha256,
                &request_sha256,
                &approved_at,
            )
            .map_err(|_| integrity("migration approval manifest transition is invalid"))?;
            let approved_bytes = serde_json::to_vec_pretty(&approved_manifest)
                .map_err(|_| integrity("migration approval manifest serialization failed"))?;
            let manifest_after_sha256 = lowercase_hex(&sha256(&approved_bytes));
            let mut receipt = build_receipt(
                approval_id.clone(),
                request,
                operator_subject_sha256,
                request_sha256.clone(),
                manifest_after_sha256,
                approved_at.clone(),
            );
            receipt.approval_sha256 = approval_receipt_sha256(&receipt)?;
            let event_payload = format!("migration candidate approved by operator {}", approval_id);
            if let Some(existing) = existing {
                if existing.0 != "operator"
                    || existing.1 != operator_subject_sha256
                    || existing.2 != "migration_applied"
                    || existing.3 != "succeeded"
                    || existing.5 != receipt.approval_sha256
                    || existing.6.as_deref() != Some(request.workspace_id.as_str())
                    || existing.7.as_deref() != Some(request.migration_id.as_str())
                    || existing.8 != event_payload.as_bytes()
                {
                    return Err(integrity("stored migration approval event differs"));
                }
            } else {
                let correlation_id =
                    deterministic_id("ref_", &receipt.approval_sha256, "migration-approval");
                database.record_event_in_transaction(
                    transaction,
                    &NewAuditEvent {
                        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
                        event_id,
                        project_id: request.project_id.clone(),
                        workspace_id: Some(request.workspace_id.clone()),
                        actor_kind: "operator".to_string(),
                        actor_subject_id: operator_subject_sha256.to_string(),
                        event_kind: "migration_applied".to_string(),
                        outcome: "succeeded".to_string(),
                        occurred_at: approved_at,
                        fingerprint: receipt.approval_sha256.clone(),
                        correlation_id,
                        resource_id: Some(request.migration_id.clone()),
                        payload: event_payload.into_bytes(),
                    },
                )?;
            }
            Ok((receipt, approved_bytes))
        })
        .map_err(|_| LegacyApprovalError::Storage)?;

    match current_state {
        "staged" => {
            publish_approved_manifest(&migration_root, &manifest_path, &approved_manifest_bytes)?
        }
        "approved" if manifest_bytes == approved_manifest_bytes => {}
        _ => return Err(LegacyApprovalError::Manifest),
    }
    Ok(receipt)
}

fn validate_request(
    request: &LegacyApprovalRequest,
    operator_subject_sha256: &str,
    trusted_now_unix: i64,
) -> Result<(), LegacyApprovalError> {
    let hashes = [
        request.expected_manifest_snapshot_sha256.as_str(),
        request.expected_source_snapshot_sha256.as_str(),
        request.expected_staging_snapshot_sha256.as_str(),
        request.expected_conversion_report_sha256.as_str(),
        request.expected_attestation_sha256.as_str(),
        request.expected_candidate_logical_state_sha256.as_str(),
        request.rollback_legacy_snapshot_sha256.as_str(),
        operator_subject_sha256,
    ];
    if request.contract_version != APPROVAL_REQUEST_CONTRACT
        || !valid_opaque_id(&request.migration_id, "mig_")
        || !valid_opaque_id(&request.project_id, "prj_")
        || !valid_opaque_id(&request.workspace_id, "wsp_")
        || !valid_opaque_id(&request.attestation_id, "att_")
        || hashes.iter().any(|value| !valid_sha256(value))
        || request.rollback_legacy_snapshot_sha256 != request.expected_source_snapshot_sha256
        || trusted_now_unix < 0
    {
        return Err(LegacyApprovalError::Request);
    }
    Ok(())
}

fn validate_manifest_generation(
    manifest: &Value,
    request: &LegacyApprovalRequest,
) -> Result<(), LegacyApprovalError> {
    if manifest["contract_version"] != "migration-manifest-v1"
        || manifest["migration_id"] != request.migration_id
        || manifest["project_id"] != request.project_id
        || manifest["source_snapshot_sha256"] != request.expected_source_snapshot_sha256
        || manifest["staging_snapshot_sha256"] != request.expected_staging_snapshot_sha256
        || !matches!(manifest["state"].as_str(), Some("staged" | "approved"))
        || !manifest["cutover_at"].is_null()
        || !manifest["rollback_deadline"].is_null()
        || manifest["rollback"]["legacy_snapshot_sha256"] != request.rollback_legacy_snapshot_sha256
        || !manifest["rollback"]["post_cutover_first_sequence"].is_null()
        || manifest["rollback"]["status"] != "prepared"
    {
        return Err(LegacyApprovalError::Manifest);
    }
    if manifest["state"] == "staged"
        && (!manifest["approved_at"].is_null()
            || manifest["approvals"]
                .as_array()
                .is_none_or(|approvals| !approvals.is_empty()))
    {
        return Err(LegacyApprovalError::Manifest);
    }
    Ok(())
}

fn validate_report(
    report: &LegacyConversionReport,
    request: &LegacyApprovalRequest,
) -> Result<(), LegacyApprovalError> {
    if report.contract_version != "legacy-conversion-report-v1"
        || report.migration_id != request.migration_id
        || report.project_id != request.project_id
        || report.workspace_id != request.workspace_id
        || report.source_snapshot_sha256 != request.expected_source_snapshot_sha256
        || report.staging_snapshot_sha256 != request.expected_staging_snapshot_sha256
        || report.manifest_snapshot_sha256 != request.expected_manifest_snapshot_sha256
        || report.source_counts != report.imported_counts
        || report.comparison != "equivalent"
        || report.state != "candidate_ready_for_independent_attestation"
        || report.approval_created
        || report.cutover_performed
    {
        return Err(LegacyApprovalError::Candidate);
    }
    Ok(())
}

fn read_and_validate_attestation(
    database: &ProjectDatabase,
    request: &LegacyApprovalRequest,
    report: &LegacyConversionReport,
) -> Result<LegacyVerifierAttestation, LegacyApprovalError> {
    database
        .with_immediate_transaction(|transaction| {
            let payload: Vec<u8> = transaction.query_row(
                "SELECT payload FROM attestations WHERE attestation_id = ?1 AND project_id = ?2",
                AC_params![request.attestation_id, request.project_id],
                |row| row.get(0),
            )?;
            if lowercase_hex(&sha256(&payload)) != request.expected_attestation_sha256 {
                return Err(integrity("migration approval attestation digest differs"));
            }
            let attestation: LegacyVerifierAttestation = serde_json::from_slice(&payload)
                .map_err(|_| integrity("migration approval attestation payload is invalid"))?;
            let audit_count: i64 = transaction.query_row(
                "SELECT count(*) FROM events WHERE project_id = ?1 AND event_kind = 'migration_applied' AND actor_kind = 'ci' AND outcome = 'succeeded' AND fingerprint = ?2 AND resource_id = ?3",
                AC_params![request.project_id, request.expected_attestation_sha256, request.migration_id],
                |row| row.get(0),
            )?;
            if audit_count != 1
                || attestation.contract_version != "migration-verifier-attestation-v1"
                || attestation.attestation_id != request.attestation_id
                || attestation.migration_id != request.migration_id
                || attestation.project_id != request.project_id
                || attestation.workspace_id != request.workspace_id
                || attestation.conversion_report_sha256
                    != request.expected_conversion_report_sha256
                || attestation.manifest_snapshot_sha256
                    != request.expected_manifest_snapshot_sha256
                || attestation.source_snapshot_sha256 != request.expected_source_snapshot_sha256
                || attestation.staging_snapshot_sha256
                    != request.expected_staging_snapshot_sha256
                || attestation.candidate_logical_state_sha256
                    != request.expected_candidate_logical_state_sha256
                || attestation.target_storage_schema_version != STORAGE_SCHEMA_VERSION
                || attestation.verified_counts != report.imported_counts
                || attestation.authentication != "ci_capability"
                || attestation.signature != "absent"
                || attestation.decision != "attested"
                || attestation.manifest_state != "staged"
                || attestation.approval_created
                || attestation.cutover_performed
                || attestation.state != "verifier_attested"
                || !valid_sha256(&attestation.verifier_subject_sha256)
                || attestation.checks.conversion_revalidation != "passed"
                || attestation.checks.storage_integrity != "passed"
                || attestation.checks.logical_state_fingerprint != "passed"
                || attestation.checks.opaque_identity_bindings != "passed"
                || attestation.checks.projection_nondisclosure != "passed"
                || attestation.checks.domain_count_parity != "passed"
            {
                return Err(integrity("migration approval attestation binding differs"));
            }
            Ok(attestation)
        })
        .map_err(|_| LegacyApprovalError::Storage)
}

fn approved_manifest_value(
    current: &Value,
    request: &LegacyApprovalRequest,
    attestation: &LegacyVerifierAttestation,
    operator_subject_sha256: &str,
    request_sha256: &str,
    approved_at: &str,
) -> Result<Value, LegacyApprovalError> {
    let mut base = current.clone();
    if current["state"] == "approved" {
        base["state"] = Value::String("staged".to_string());
        base["approved_at"] = Value::Null;
        base["approvals"] = json!([]);
    }
    if base["state"] != "staged"
        || !base["approved_at"].is_null()
        || base["approvals"]
            .as_array()
            .is_none_or(|approvals| !approvals.is_empty())
        || base["counts"]
            != serde_json::to_value(attestation.verified_counts)
                .map_err(|_| LegacyApprovalError::Manifest)?
    {
        return Err(LegacyApprovalError::Manifest);
    }
    let verifier_decided_at = attestation
        .attested_at
        .strip_prefix("unix:")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(LegacyApprovalError::Candidate)
        .and_then(|value| {
            rfc3339_utc_from_unix(value).map_err(|_| LegacyApprovalError::Candidate)
        })?;
    base["state"] = Value::String("approved".to_string());
    base["approved_at"] = Value::String(approved_at.to_string());
    base["approvals"] = json!([
        {
            "role": "verifier",
            "subject_id": attestation.verifier_subject_sha256,
            "decision": "attested",
            "evidence_sha256": request.expected_attestation_sha256,
            "decided_at": verifier_decided_at
        },
        {
            "role": "operator",
            "subject_id": operator_subject_sha256,
            "decision": "approved",
            "evidence_sha256": request_sha256,
            "decided_at": approved_at
        }
    ]);
    if current["state"] == "approved" && base != *current {
        return Err(LegacyApprovalError::Manifest);
    }
    Ok(base)
}

fn build_receipt(
    approval_id: String,
    request: &LegacyApprovalRequest,
    operator_subject_sha256: &str,
    request_sha256: String,
    manifest_after_sha256: String,
    approved_at: String,
) -> LegacyApprovalReceipt {
    LegacyApprovalReceipt {
        contract_version: APPROVAL_RECEIPT_CONTRACT.to_string(),
        approval_id,
        migration_id: request.migration_id.clone(),
        project_id: request.project_id.clone(),
        workspace_id: request.workspace_id.clone(),
        operator_subject_sha256: operator_subject_sha256.to_string(),
        approval_request_sha256: request_sha256,
        attestation_id: request.attestation_id.clone(),
        attestation_sha256: request.expected_attestation_sha256.clone(),
        candidate_logical_state_sha256: request.expected_candidate_logical_state_sha256.clone(),
        manifest_before_sha256: request.expected_manifest_snapshot_sha256.clone(),
        manifest_after_sha256,
        source_snapshot_sha256: request.expected_source_snapshot_sha256.clone(),
        staging_snapshot_sha256: request.expected_staging_snapshot_sha256.clone(),
        rollback_legacy_snapshot_sha256: request.rollback_legacy_snapshot_sha256.clone(),
        approved_at,
        operator_decision: "approved".to_string(),
        verifier_decision: "attested".to_string(),
        rollback_status: "prepared".to_string(),
        rollback_deadline: None,
        manifest_state: "approved".to_string(),
        cutover_performed: false,
        state: "approved_awaiting_cutover".to_string(),
        approval_sha256: String::new(),
    }
}

fn approval_receipt_sha256(
    receipt: &LegacyApprovalReceipt,
) -> Result<String, TransactionalStorageError> {
    let mut unsigned = receipt.clone();
    unsigned.approval_sha256.clear();
    let bytes = serde_json::to_vec(&unsigned)
        .map_err(|_| integrity("migration approval receipt serialization failed"))?;
    Ok(lowercase_hex(&sha256(&bytes)))
}

fn publish_approved_manifest(
    migration_root: &Path,
    manifest_path: &Path,
    approved_bytes: &[u8],
) -> Result<(), LegacyApprovalError> {
    let partial_path = migration_root.join("migration-manifest-v1.approval.partial");
    match fs::symlink_metadata(&partial_path) {
        Ok(_) => {
            if read_control_file(&partial_path).map_err(|_| LegacyApprovalError::Manifest)?
                != approved_bytes
            {
                return Err(LegacyApprovalError::Manifest);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_private_bytes(&partial_path, approved_bytes)
                .map_err(|_| LegacyApprovalError::Manifest)?;
        }
        Err(_) => return Err(LegacyApprovalError::Manifest),
    }
    fs::rename(&partial_path, manifest_path).map_err(|_| LegacyApprovalError::Manifest)?;
    sync_directory(migration_root).map_err(|_| LegacyApprovalError::Manifest)?;
    if read_control_file(manifest_path).map_err(|_| LegacyApprovalError::Manifest)?
        != approved_bytes
    {
        return Err(LegacyApprovalError::Manifest);
    }
    Ok(())
}

fn deterministic_id(prefix: &str, left: &str, right: &str) -> String {
    let mut material = left.as_bytes().to_vec();
    material.push(0);
    material.extend_from_slice(right.as_bytes());
    let digest = sha256(&material);
    let mut opaque = [0_u8; 16];
    opaque.copy_from_slice(&digest[..16]);
    crockford_id(prefix, &opaque)
}

fn integrity(message: &str) -> TransactionalStorageError {
    TransactionalStorageError::Integrity(message.to_string())
}

#[cfg(test)]
#[path = "tests/Legacy_Approval_test.rs"]
mod legacy_approval_tests;
