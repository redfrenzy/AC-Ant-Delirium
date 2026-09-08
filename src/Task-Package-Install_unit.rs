use ac_ant_delirium::libres::Rusqlite_unit::{AC_OptionalExtension, AC_Transaction, AC_params};
use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::crockford_id;
use ac_ant_delirium::structs::{
    encode_canonical_task_projection_payload, CanonicalTaskProjectionPayload, NewAuditEvent,
    ProjectDatabase, ProjectedTaskAction, TransactionalStorageError, TransactionalStorageResult,
    CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION,
};
use serde::de::DeserializeOwned;

use crate::task_package_contracts::{
    validate_approval, validate_attestation, validate_governance_package_v2,
    validate_operation_receipt, validate_proposal, GovernanceTaskPackageV2, TaskPackageApproval,
    TaskPackageAttestation, TaskPackageOperationReceipt, TaskPackageProposal,
    TASK_PACKAGE_OPERATION_RECEIPT_V1,
};
use crate::task_package_generator::proposal_payload_sha256;
use crate::task_package_store::TaskPackageWriteOutcome;

pub(crate) fn installation_receipt(
    package: &GovernanceTaskPackageV2,
) -> TransactionalStorageResult<TaskPackageOperationReceipt> {
    validate_governance_package_v2(package).map_err(|_| invalid())?;
    let bytes = serde_json::to_vec(package).map_err(|_| invalid())?;
    if bytes.len() > 4 * 1024 * 1024 - 64 * 1024 {
        return Err(invalid());
    }
    let digest = sha256(&bytes);
    let mut material = b"task-package-installation-v2\0".to_vec();
    material.extend_from_slice(&digest);
    let id_digest = sha256(&material);
    let receipt = TaskPackageOperationReceipt {
        contract_version: TASK_PACKAGE_OPERATION_RECEIPT_V1.to_string(),
        request_id: package.request_id.clone(),
        operation: "install".to_string(),
        project_id: package.project_id.clone(),
        workspace_id: package.workspace_id.clone(),
        artifact_id: crockford_id("rcp_", &id_digest[..16].try_into().map_err(|_| invalid())?),
        artifact_sha256: crate::legacy_staging::lowercase_hex(&digest),
        previous_state_revision: package.expected_state_revision,
        resulting_state_revision: package
            .expected_state_revision
            .checked_add(1)
            .ok_or_else(invalid)?,
        state: "installed".to_string(),
    };
    validate_operation_receipt(&receipt).map_err(|_| invalid())?;
    Ok(receipt)
}

pub(crate) fn install_approved_task_package(
    database: &ProjectDatabase,
    package: &GovernanceTaskPackageV2,
    event: &NewAuditEvent,
) -> TransactionalStorageResult<TaskPackageWriteOutcome> {
    let receipt = installation_receipt(package)?;
    if event.project_id != package.project_id
        || event.workspace_id.as_deref() != Some(package.workspace_id.as_str())
        || event.actor_kind != "ci"
        || event.event_kind != "state_transitioned"
        || event.outcome != "succeeded"
        || event.resource_id.as_deref() != Some(receipt.artifact_id.as_str())
        || event.fingerprint != receipt.artifact_sha256
    {
        return Err(invalid());
    }
    let payload = serde_json::to_vec(package).map_err(|_| invalid())?;
    let receipt_bytes = serde_json::to_vec(&receipt).map_err(|_| invalid())?;
    // Encode through the same public-projection validator as v1 before writing.
    let encoded_tasks = package
        .plans
        .iter()
        .flat_map(|plan| &plan.tasks)
        .map(|task| {
            encode_canonical_task_projection_payload(&CanonicalTaskProjectionPayload {
                contract_version: CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION.to_string(),
                objective: task.objective.clone(),
                allowed_action: ProjectedTaskAction::Implement,
                acceptance_criteria: task.acceptance_criteria.clone(),
                relevant_paths: task.relevant_paths.clone(),
            })
        })
        .collect::<TransactionalStorageResult<Vec<_>>>()?;

    database.with_immediate_transaction(|transaction| {
        let active_workspace: i64 = transaction.query_row(
            "SELECT count(*) FROM workspaces WHERE project_id = ?1 AND workspace_id = ?2 AND status = 'active'",
            AC_params![package.project_id, package.workspace_id], |row| row.get(0))?;
        if active_workspace != 1 { return Err(invalid()); }
        let previous: Option<(Vec<u8>, Vec<u8>)> = transaction.query_row(
            "SELECT payload, receipt FROM task_package_installations WHERE project_id = ?1 OR proposal_id = ?2 OR receipt_id = ?3",
            AC_params![package.project_id, package.proposal_id, receipt.artifact_id],
            |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
        if let Some(previous) = previous {
            if previous.0 != payload || previous.1 != receipt_bytes { return Err(invalid()); }
            // A retry never rewinds state, even if tasks have since advanced.
            return Ok(TaskPackageWriteOutcome::Replay(receipt));
        }
        require_approved_proposal(transaction, package)?;
        let current: (i64, Option<String>, Option<String>) = transaction.query_row(
            "SELECT state_revision, active_plan_id, current_task_id FROM projects WHERE project_id = ?1",
            AC_params![package.project_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        let occupied: i64 = transaction.query_row(
            "SELECT (SELECT count(*) FROM plans WHERE project_id = ?1) + (SELECT count(*) FROM tasks WHERE project_id = ?1) + (SELECT count(*) FROM submissions WHERE project_id = ?1) + (SELECT count(*) FROM reviews WHERE project_id = ?1)",
            AC_params![package.project_id], |row| row.get(0))?;
        if current.0 != package.expected_state_revision || current.1.is_some()
            || current.2.is_some() || occupied != 0 { return Err(invalid()); }

        let mut encoded = encoded_tasks.iter();
        for plan in &package.plans {
            // 'active' is the existing non-completed lifecycle status; only
            // projects.active_plan_id selects the single running plan.
            transaction.execute(
                "INSERT INTO plans(plan_id, project_id, plan_revision, status, payload, created_at, plan_ordinal) VALUES (?1, ?2, 1, 'active', ?3, ?4, ?5)",
                AC_params![plan.plan_id, package.project_id, serde_json::to_vec(plan).map_err(|_| invalid())?, event.occurred_at, plan.plan_ordinal])?;
            for task in &plan.tasks {
                transaction.execute(
                    "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at, task_ordinal) VALUES (?1, ?2, ?3, ?4, 'available', 'not_required', ?5, ?6, ?7)",
                    AC_params![task.task_id, package.project_id, plan.plan_id, task.task_revision,
                        encoded.next().ok_or_else(invalid)?, event.occurred_at, task.task_ordinal])?;
            }
        }
        let updated = transaction.execute(
            "UPDATE projects SET active_plan_id = ?2, current_task_id = ?3, state_revision = ?4 WHERE project_id = ?1 AND state_revision = ?5 AND active_plan_id IS NULL AND current_task_id IS NULL",
            AC_params![package.project_id, package.active_plan_id, package.current_task_id,
                receipt.resulting_state_revision, package.expected_state_revision])?;
        if updated != 1 { return Err(invalid()); }
        ac_ant_delirium::structs::Governance_Errors_Projection_structcore::carry_forward_canonical_governance_snapshot(
            transaction, &package.project_id, &package.workspace_id, package.expected_state_revision,
            receipt.resulting_state_revision, &event.occurred_at)?;
        if !database.record_event_in_transaction(transaction, event)?.inserted() { return Err(invalid()); }
        transaction.execute(
            "INSERT INTO task_package_installations(receipt_id, request_id, project_id, workspace_id, proposal_id, attestation_id, approval_id, package_sha256, resulting_state_revision, event_id, payload, receipt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            AC_params![receipt.artifact_id, package.request_id, package.project_id, package.workspace_id,
                package.proposal_id, package.attestation_id, package.approval_id, receipt.artifact_sha256,
                receipt.resulting_state_revision, event.event_id, payload, receipt_bytes])?;
        Ok(TaskPackageWriteOutcome::Inserted(receipt))
    })
}

fn require_approved_proposal(
    transaction: &AC_Transaction<'_>,
    package: &GovernanceTaskPackageV2,
) -> TransactionalStorageResult<()> {
    let proposal_bytes: Vec<u8> = transaction.query_row(
        "SELECT payload FROM task_package_proposals WHERE proposal_id = ?1",
        AC_params![package.proposal_id],
        |row| row.get(0),
    )?;
    let attestation_bytes: Vec<u8> = transaction.query_row(
        "SELECT payload FROM task_package_attestations WHERE attestation_id = ?1",
        AC_params![package.attestation_id],
        |row| row.get(0),
    )?;
    let approval_bytes: Vec<u8> = transaction.query_row(
        "SELECT payload FROM task_package_approvals WHERE approval_id = ?1",
        AC_params![package.approval_id],
        |row| row.get(0),
    )?;
    let proposal: TaskPackageProposal = decode(&proposal_bytes)?;
    let attestation: TaskPackageAttestation = decode(&attestation_bytes)?;
    let approval: TaskPackageApproval = decode(&approval_bytes)?;
    validate_proposal(&proposal).map_err(|_| invalid())?;
    validate_attestation(&attestation).map_err(|_| invalid())?;
    validate_approval(&approval).map_err(|_| invalid())?;
    for (project, workspace, revision) in [
        (
            &proposal.project_id,
            &proposal.workspace_id,
            proposal.expected_state_revision,
        ),
        (
            &attestation.project_id,
            &attestation.workspace_id,
            attestation.expected_state_revision,
        ),
        (
            &approval.project_id,
            &approval.workspace_id,
            approval.expected_state_revision,
        ),
    ] {
        if project != &package.project_id
            || workspace != &package.workspace_id
            || revision != package.expected_state_revision
        {
            return Err(invalid());
        }
    }
    if proposal.proposal_id != package.proposal_id
        || proposal.payload_sha256 != package.proposal_sha256
        || proposal_payload_sha256(&proposal).map_err(|_| invalid())? != package.proposal_sha256
        || proposal.plans != package.plans
        || !proposal.differences.is_empty()
        || !proposal.unconverted_source_keys_sha256.is_empty()
        || attestation.attestation_id != package.attestation_id
        || attestation.proposal_id != package.proposal_id
        || attestation.proposal_sha256 != package.proposal_sha256
        || attestation.source_sha256 != proposal.source_sha256
        || crate::legacy_staging::lowercase_hex(&sha256(&attestation_bytes))
            != package.attestation_sha256
        || approval.approval_id != package.approval_id
        || approval.proposal_id != package.proposal_id
        || approval.proposal_sha256 != package.proposal_sha256
        || approval.attestation_id != package.attestation_id
        || approval.attestation_sha256 != package.attestation_sha256
        || crate::legacy_staging::lowercase_hex(&sha256(&approval_bytes)) != package.approval_sha256
        || approval.operator_subject_sha256 == attestation.verifier_subject_sha256
    {
        return Err(invalid());
    }
    Ok(())
}

fn decode<T: DeserializeOwned>(payload: &[u8]) -> TransactionalStorageResult<T> {
    serde_json::from_slice(payload).map_err(|_| invalid())
}

fn invalid() -> TransactionalStorageError {
    TransactionalStorageError::Integrity(
        "task package installation binding or state conflicts".to_string(),
    )
}
