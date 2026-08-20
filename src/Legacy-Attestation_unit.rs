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

use crate::legacy_conversion::{
    conversion_request, convert_legacy_migration, read_control_file, LegacyConversionCounts,
    LegacyConversionReport,
};
use crate::legacy_staging::{lowercase_hex, require_private_directory, valid_sha256};
use crate::limited_protocol::valid_opaque_id;

const ATTESTATION_REQUEST_CONTRACT: &str = "migration-attestation-request-v1";
const ATTESTATION_CONTRACT: &str = "migration-verifier-attestation-v1";
const ATTESTATION_RECEIPT_CONTRACT: &str = "migration-attestation-receipt-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyAttestationError {
    Request,
    Candidate,
    Storage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyAttestationRequest {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) expected_conversion_report_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyAttestationChecks {
    pub(crate) conversion_revalidation: String,
    pub(crate) storage_integrity: String,
    pub(crate) logical_state_fingerprint: String,
    pub(crate) opaque_identity_bindings: String,
    pub(crate) projection_nondisclosure: String,
    pub(crate) domain_count_parity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyVerifierAttestation {
    pub(crate) contract_version: String,
    pub(crate) attestation_id: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) verifier_subject_sha256: String,
    pub(crate) conversion_report_sha256: String,
    pub(crate) manifest_snapshot_sha256: String,
    pub(crate) source_snapshot_sha256: String,
    pub(crate) staging_snapshot_sha256: String,
    pub(crate) normalized_snapshot_sha256: String,
    pub(crate) semantic_snapshot_sha256: String,
    pub(crate) dry_run_report_sha256: String,
    pub(crate) identity_bindings_sha256: String,
    pub(crate) current_task_projection_sha256: String,
    pub(crate) governance_errors_projection_sha256: String,
    pub(crate) candidate_logical_state_sha256: String,
    pub(crate) target_storage_schema_version: i32,
    pub(crate) verified_counts: LegacyConversionCounts,
    pub(crate) checks: LegacyAttestationChecks,
    pub(crate) authentication: String,
    pub(crate) signature: String,
    pub(crate) decision: String,
    pub(crate) manifest_state: String,
    pub(crate) approval_created: bool,
    pub(crate) cutover_performed: bool,
    pub(crate) attested_at: String,
    pub(crate) state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyAttestationReceipt {
    pub(crate) contract_version: String,
    pub(crate) attestation: LegacyVerifierAttestation,
    pub(crate) attestation_sha256: String,
}

pub(crate) fn attestation_request(
    migration_id: String,
    project_id: String,
    workspace_id: String,
    expected_conversion_report_sha256: String,
) -> LegacyAttestationRequest {
    LegacyAttestationRequest {
        contract_version: ATTESTATION_REQUEST_CONTRACT.to_string(),
        migration_id,
        project_id,
        workspace_id,
        expected_conversion_report_sha256,
    }
}

pub(crate) fn attest_legacy_candidate(
    staging_parent: &Path,
    request: &LegacyAttestationRequest,
    verifier_subject_sha256: &str,
    trusted_now_unix: i64,
) -> Result<LegacyAttestationReceipt, LegacyAttestationError> {
    validate_request(request, verifier_subject_sha256, trusted_now_unix)?;
    require_private_directory(staging_parent).map_err(|_| LegacyAttestationError::Candidate)?;
    let migration_root = staging_parent.join(&request.migration_id);
    require_private_directory(&migration_root).map_err(|_| LegacyAttestationError::Candidate)?;
    let converted_root = migration_root.join("converted");
    require_private_directory(&converted_root).map_err(|_| LegacyAttestationError::Candidate)?;
    let report_bytes = read_control_file(&converted_root.join("legacy-conversion-report-v1.json"))
        .map_err(|_| LegacyAttestationError::Candidate)?;
    if lowercase_hex(&sha256(&report_bytes)) != request.expected_conversion_report_sha256 {
        return Err(LegacyAttestationError::Candidate);
    }
    let report: LegacyConversionReport =
        serde_json::from_slice(&report_bytes).map_err(|_| LegacyAttestationError::Candidate)?;
    if report.migration_id != request.migration_id
        || report.project_id != request.project_id
        || report.workspace_id != request.workspace_id
        || report.source_counts != report.imported_counts
        || report.comparison != "equivalent"
        || report.state != "candidate_ready_for_independent_attestation"
        || report.manifest_state_after != "staged"
        || report.approval_created
        || report.cutover_performed
    {
        return Err(LegacyAttestationError::Candidate);
    }
    let revalidated = convert_legacy_migration(
        staging_parent,
        &conversion_request(
            report.migration_id.clone(),
            report.source_snapshot_sha256.clone(),
            report.staging_snapshot_sha256.clone(),
            report.normalized_snapshot_sha256.clone(),
            report.semantic_snapshot_sha256.clone(),
            report.dry_run_report_sha256.clone(),
        ),
    )
    .map_err(|_| LegacyAttestationError::Candidate)?;
    if revalidated != report {
        return Err(LegacyAttestationError::Candidate);
    }

    let database = ProjectDatabase::open_existing_at(&converted_root.join("candidate"))
        .map_err(|_| LegacyAttestationError::Storage)?;
    let health = database
        .health()
        .map_err(|_| LegacyAttestationError::Storage)?;
    if health.schema_version != STORAGE_SCHEMA_VERSION
        || health.quick_check != "ok"
        || health.foreign_key_violations != 0
        || health.trusted_schema
        || !health.foreign_keys
    {
        return Err(LegacyAttestationError::Storage);
    }
    persist_attestation(
        &database,
        request,
        &report,
        verifier_subject_sha256,
        trusted_now_unix,
    )
}

fn validate_request(
    request: &LegacyAttestationRequest,
    verifier_subject_sha256: &str,
    trusted_now_unix: i64,
) -> Result<(), LegacyAttestationError> {
    if request.contract_version != ATTESTATION_REQUEST_CONTRACT
        || !valid_opaque_id(&request.migration_id, "mig_")
        || !valid_opaque_id(&request.project_id, "prj_")
        || !valid_opaque_id(&request.workspace_id, "wsp_")
        || !valid_sha256(&request.expected_conversion_report_sha256)
        || !valid_sha256(verifier_subject_sha256)
        || trusted_now_unix < 0
    {
        return Err(LegacyAttestationError::Request);
    }
    Ok(())
}

fn persist_attestation(
    database: &ProjectDatabase,
    request: &LegacyAttestationRequest,
    report: &LegacyConversionReport,
    verifier_subject_sha256: &str,
    trusted_now_unix: i64,
) -> Result<LegacyAttestationReceipt, LegacyAttestationError> {
    database
        .with_immediate_transaction(|transaction| {
            let logical_state_sha256 = logical_state_sha256(
                transaction,
                &request.project_id,
                &request.workspace_id,
                &request.migration_id,
                &report.imported_counts,
            )?;
            let attestation_id = deterministic_id(
                "att_",
                &request.expected_conversion_report_sha256,
                verifier_subject_sha256,
            );
            let existing = transaction
                .query_row(
                    "SELECT payload FROM attestations WHERE attestation_id = ?1 AND project_id = ?2",
                    AC_params![attestation_id, request.project_id],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?;
            if let Some(existing) = existing {
                let attestation: LegacyVerifierAttestation = serde_json::from_slice(&existing)
                    .map_err(|_| integrity("stored migration attestation is invalid"))?;
                let expected = build_attestation(
                    attestation.attested_at.clone(),
                    attestation_id,
                    request,
                    report,
                    verifier_subject_sha256,
                    logical_state_sha256,
                );
                if attestation != expected {
                    return Err(integrity("stored migration attestation does not match candidate"));
                }
                return receipt(attestation);
            }
            let attestation = build_attestation(
                format!("unix:{trusted_now_unix}"),
                attestation_id.clone(),
                request,
                report,
                verifier_subject_sha256,
                logical_state_sha256,
            );
            let payload = serde_json::to_vec(&attestation)
                .map_err(|_| integrity("migration attestation serialization failed"))?;
            transaction.execute(
                "INSERT INTO attestations(attestation_id, project_id, observation_id, payload, created_at) VALUES (?1, ?2, NULL, ?3, ?4)",
                AC_params![attestation_id, request.project_id, payload, attestation.attested_at],
            )?;
            let attestation_sha256 = lowercase_hex(&sha256(&payload));
            let event_id = deterministic_id("evt_", &attestation_sha256, "migration-attestation");
            let correlation_id = deterministic_id("ref_", &attestation_sha256, "migration-correlation");
            database.record_event_in_transaction(
                transaction,
                &NewAuditEvent {
                    contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
                    event_id,
                    project_id: request.project_id.clone(),
                    workspace_id: Some(request.workspace_id.clone()),
                    actor_kind: "ci".to_string(),
                    actor_subject_id: verifier_subject_sha256.to_string(),
                    event_kind: "migration_applied".to_string(),
                    outcome: "succeeded".to_string(),
                    occurred_at: attestation.attested_at.clone(),
                    fingerprint: attestation_sha256,
                    correlation_id,
                    resource_id: Some(request.migration_id.clone()),
                    payload: b"migration candidate independently attested".to_vec(),
                },
            )?;
            receipt(attestation)
        })
        .map_err(|_| LegacyAttestationError::Storage)
}

fn build_attestation(
    attested_at: String,
    attestation_id: String,
    request: &LegacyAttestationRequest,
    report: &LegacyConversionReport,
    verifier_subject_sha256: &str,
    logical_state_sha256: String,
) -> LegacyVerifierAttestation {
    LegacyVerifierAttestation {
        contract_version: ATTESTATION_CONTRACT.to_string(),
        attestation_id,
        migration_id: request.migration_id.clone(),
        project_id: request.project_id.clone(),
        workspace_id: request.workspace_id.clone(),
        verifier_subject_sha256: verifier_subject_sha256.to_string(),
        conversion_report_sha256: request.expected_conversion_report_sha256.clone(),
        manifest_snapshot_sha256: report.manifest_snapshot_sha256.clone(),
        source_snapshot_sha256: report.source_snapshot_sha256.clone(),
        staging_snapshot_sha256: report.staging_snapshot_sha256.clone(),
        normalized_snapshot_sha256: report.normalized_snapshot_sha256.clone(),
        semantic_snapshot_sha256: report.semantic_snapshot_sha256.clone(),
        dry_run_report_sha256: report.dry_run_report_sha256.clone(),
        identity_bindings_sha256: report.identity_bindings_sha256.clone(),
        current_task_projection_sha256: report.current_task_projection_sha256.clone(),
        governance_errors_projection_sha256: report.governance_errors_projection_sha256.clone(),
        candidate_logical_state_sha256: logical_state_sha256,
        target_storage_schema_version: STORAGE_SCHEMA_VERSION,
        verified_counts: report.imported_counts,
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
        attested_at,
        state: "verifier_attested".to_string(),
    }
}

fn receipt(
    attestation: LegacyVerifierAttestation,
) -> Result<LegacyAttestationReceipt, TransactionalStorageError> {
    let bytes = serde_json::to_vec(&attestation)
        .map_err(|_| integrity("migration attestation serialization failed"))?;
    Ok(LegacyAttestationReceipt {
        contract_version: ATTESTATION_RECEIPT_CONTRACT.to_string(),
        attestation,
        attestation_sha256: lowercase_hex(&sha256(&bytes)),
    })
}

pub(crate) fn logical_state_sha256(
    transaction: &ac_ant_delirium::libres::Rusqlite_unit::AC_Transaction<'_>,
    project_id: &str,
    workspace_id: &str,
    migration_id: &str,
    expected_counts: &LegacyConversionCounts,
) -> Result<String, TransactionalStorageError> {
    let project = transaction.query_row(
        "SELECT project_id, logical_contract_version, state_revision, active_plan_id, current_task_id FROM projects WHERE project_id = ?1",
        AC_params![project_id],
        |row| Ok(json!([row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, Option<String>>(4)?])),
    )?;
    let workspace = transaction.query_row(
        "SELECT workspace_id, project_id, status FROM workspaces WHERE workspace_id = ?1 AND project_id = ?2",
        AC_params![workspace_id, project_id],
        |row| Ok(json!([row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?])),
    )?;
    let plans = query_plan_rows(transaction, project_id)?;
    let tasks = query_task_rows(transaction, project_id)?;
    let records = query_migration_rows(transaction, project_id, migration_id)?;
    let governance = transaction.query_row(
        "SELECT contract_version, source_revision, fingerprint, hex(payload) FROM governance_snapshots WHERE project_id = ?1 AND workspace_id = ?2",
        AC_params![project_id, workspace_id],
        |row| Ok(json!([row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?])),
    )?;
    if plans.len() as u64 != expected_counts.plans
        || tasks.len() as u64 != expected_counts.tasks
        || records.iter().filter(|row| row[0] == "history").count() as u64
            != expected_counts.history_events
        || records.iter().filter(|row| row[0] == "problem").count() as u64
            != expected_counts.problems
        || records.iter().filter(|row| row[0] == "transition").count() as u64
            != expected_counts.transitions
    {
        return Err(integrity("migration candidate logical counts differ"));
    }
    let logical = serde_json::to_vec(&json!({
        "contract_version": "migration-candidate-logical-state-v1",
        "project": project,
        "workspace": workspace,
        "plans": plans,
        "tasks": tasks,
        "legacy_records": records,
        "governance": governance
    }))
    .map_err(|_| integrity("migration candidate logical state serialization failed"))?;
    Ok(lowercase_hex(&sha256(&logical)))
}

fn query_plan_rows(
    transaction: &ac_ant_delirium::libres::Rusqlite_unit::AC_Transaction<'_>,
    project_id: &str,
) -> Result<Vec<Value>, TransactionalStorageError> {
    let mut statement = transaction.prepare(
        "SELECT plan_id, plan_revision, status, hex(payload) FROM plans WHERE project_id = ?1 ORDER BY plan_id",
    )?;
    let rows = statement.query_map(AC_params![project_id], |row| {
        Ok(json!([
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?
        ]))
    })?;
    let mut output = Vec::new();
    for row in rows {
        output.push(row?);
    }
    Ok(output)
}

fn query_task_rows(
    transaction: &ac_ant_delirium::libres::Rusqlite_unit::AC_Transaction<'_>,
    project_id: &str,
) -> Result<Vec<Value>, TransactionalStorageError> {
    let mut statement = transaction.prepare(
        "SELECT task_id, plan_id, task_revision, status, review_status, hex(payload) FROM tasks WHERE project_id = ?1 ORDER BY task_id",
    )?;
    let rows = statement.query_map(AC_params![project_id], |row| {
        Ok(json!([
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?
        ]))
    })?;
    let mut output = Vec::new();
    for row in rows {
        output.push(row?);
    }
    Ok(output)
}

fn query_migration_rows(
    transaction: &ac_ant_delirium::libres::Rusqlite_unit::AC_Transaction<'_>,
    project_id: &str,
    migration_id: &str,
) -> Result<Vec<Value>, TransactionalStorageError> {
    let mut statement = transaction.prepare(
        "SELECT record_kind, ordinal, source_key_sha256, payload_sha256, payload FROM legacy_migration_records WHERE project_id = ?1 AND migration_id = ?2 ORDER BY record_kind, ordinal",
    )?;
    let rows = statement.query_map(AC_params![project_id, migration_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Vec<u8>>(4)?,
        ))
    })?;
    let mut output = Vec::new();
    for row in rows {
        let (record_kind, ordinal, source_key_sha256, payload_sha256, payload) = row?;
        if !valid_sha256(&source_key_sha256)
            || !valid_sha256(&payload_sha256)
            || lowercase_hex(&sha256(&payload)) != payload_sha256
        {
            return Err(integrity(
                "migration candidate protected record digest differs",
            ));
        }
        output.push(json!([
            record_kind,
            ordinal,
            source_key_sha256,
            payload_sha256
        ]));
    }
    Ok(output)
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
#[path = "tests/Legacy_Attestation_test.rs"]
mod legacy_attestation_tests;
