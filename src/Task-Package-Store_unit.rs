use ac_ant_delirium::libres::Rusqlite_unit::{AC_OptionalExtension, AC_Transaction, AC_params};
use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::crockford_id;
use ac_ant_delirium::structs::{
    ProjectDatabase, TransactionalStorageError, TransactionalStorageResult,
};
use serde::Serialize;

use crate::task_package_contracts::{
    validate_approval, validate_attestation, validate_operation_receipt, validate_proposal,
    TaskPackageApproval, TaskPackageAttestation, TaskPackageOperationReceipt, TaskPackageProposal,
    TASK_PACKAGE_OPERATION_RECEIPT_V1,
};
use crate::task_package_generator::proposal_payload_sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskPackageStoreError {
    Contract,
    Scope,
    Conflict,
    Storage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TaskPackageWriteOutcome {
    Inserted(TaskPackageOperationReceipt),
    Replay(TaskPackageOperationReceipt),
}

impl TaskPackageWriteOutcome {
    pub(crate) fn receipt(&self) -> &TaskPackageOperationReceipt {
        match self {
            Self::Inserted(receipt) | Self::Replay(receipt) => receipt,
        }
    }
}

struct ProposalBinding {
    request_id: String,
    project_id: String,
    workspace_id: String,
    expected_state_revision: i64,
    source_sha256: String,
    payload_sha256: String,
}

pub(crate) fn store_task_package_proposal(
    database: &ProjectDatabase,
    proposal: &TaskPackageProposal,
) -> Result<TaskPackageWriteOutcome, TaskPackageStoreError> {
    validate_proposal(proposal).map_err(|_| TaskPackageStoreError::Contract)?;
    if proposal_payload_sha256(proposal).map_err(|_| TaskPackageStoreError::Contract)?
        != proposal.payload_sha256
    {
        return Err(TaskPackageStoreError::Contract);
    }
    let payload = canonical_bytes(proposal)?;
    database
        .with_immediate_transaction(|transaction| {
            require_exact_scope_revision(
                transaction,
                &proposal.project_id,
                &proposal.workspace_id,
                proposal.expected_state_revision,
            )?;
            let existing: Option<(String, Vec<u8>)> = transaction
                .query_row(
                    "SELECT proposal_id, payload FROM task_package_proposals WHERE proposal_id = ?1 OR (project_id = ?2 AND workspace_id = ?3 AND (request_id = ?4 OR payload_sha256 = ?5))",
                    AC_params![
                        proposal.proposal_id,
                        proposal.project_id,
                        proposal.workspace_id,
                        proposal.request_id,
                        proposal.payload_sha256,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let inserted = match existing {
                Some((proposal_id, existing_payload))
                    if proposal_id == proposal.proposal_id && existing_payload == payload =>
                {
                    false
                }
                Some(_) => return Err(conflict("task package proposal binding conflicts")),
                None => {
                    transaction.execute(
                        "INSERT INTO task_package_proposals(proposal_id, request_id, project_id, workspace_id, expected_state_revision, source_sha256, payload_sha256, state, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'prepared', ?8)",
                        AC_params![
                            proposal.proposal_id,
                            proposal.request_id,
                            proposal.project_id,
                            proposal.workspace_id,
                            proposal.expected_state_revision,
                            proposal.source_sha256,
                            proposal.payload_sha256,
                            payload,
                        ],
                    )?;
                    true
                }
            };
            let receipt = unchanged_revision_receipt(
                &proposal.request_id,
                "prepare",
                &proposal.project_id,
                &proposal.workspace_id,
                &proposal.proposal_id,
                &proposal.payload_sha256,
                proposal.expected_state_revision,
                "prepared",
            )?;
            Ok(if inserted {
                TaskPackageWriteOutcome::Inserted(receipt)
            } else {
                TaskPackageWriteOutcome::Replay(receipt)
            })
        })
        .map_err(map_storage_error)
}

pub(crate) fn store_task_package_attestation(
    database: &ProjectDatabase,
    attestation: &TaskPackageAttestation,
) -> Result<TaskPackageWriteOutcome, TaskPackageStoreError> {
    validate_attestation(attestation).map_err(|_| TaskPackageStoreError::Contract)?;
    let payload = canonical_bytes(attestation)?;
    let payload_sha256 = artifact_sha256(&payload);
    database
        .with_immediate_transaction(|transaction| {
            require_exact_scope_revision(
                transaction,
                &attestation.project_id,
                &attestation.workspace_id,
                attestation.expected_state_revision,
            )?;
            let proposal = proposal_binding(transaction, &attestation.proposal_id)?;
            require_matching_proposal(
                &proposal,
                &attestation.project_id,
                &attestation.workspace_id,
                attestation.expected_state_revision,
                &attestation.source_sha256,
                &attestation.proposal_sha256,
            )?;
            let existing: Option<(String, Vec<u8>)> = transaction
                .query_row(
                    "SELECT attestation_id, payload FROM task_package_attestations WHERE attestation_id = ?1 OR proposal_id = ?2 OR payload_sha256 = ?3",
                    AC_params![
                        attestation.attestation_id,
                        attestation.proposal_id,
                        payload_sha256,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let inserted = match existing {
                Some((attestation_id, existing_payload))
                    if attestation_id == attestation.attestation_id
                        && existing_payload == payload =>
                {
                    false
                }
                Some(_) => return Err(conflict("task package attestation binding conflicts")),
                None => {
                    transaction.execute(
                        "INSERT INTO task_package_attestations(attestation_id, proposal_id, proposal_sha256, source_sha256, project_id, workspace_id, expected_state_revision, verifier_subject_sha256, decision, attested_at, state, payload_sha256, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'accepted', ?9, 'attested', ?10, ?11)",
                        AC_params![
                            attestation.attestation_id,
                            attestation.proposal_id,
                            attestation.proposal_sha256,
                            attestation.source_sha256,
                            attestation.project_id,
                            attestation.workspace_id,
                            attestation.expected_state_revision,
                            attestation.verifier_subject_sha256,
                            attestation.attested_at,
                            payload_sha256,
                            payload,
                        ],
                    )?;
                    true
                }
            };
            let receipt = unchanged_revision_receipt(
                &proposal.request_id,
                "attest",
                &attestation.project_id,
                &attestation.workspace_id,
                &attestation.attestation_id,
                &payload_sha256,
                attestation.expected_state_revision,
                "attested",
            )?;
            Ok(if inserted {
                TaskPackageWriteOutcome::Inserted(receipt)
            } else {
                TaskPackageWriteOutcome::Replay(receipt)
            })
        })
        .map_err(map_storage_error)
}

pub(crate) fn store_task_package_approval(
    database: &ProjectDatabase,
    approval: &TaskPackageApproval,
) -> Result<TaskPackageWriteOutcome, TaskPackageStoreError> {
    validate_approval(approval).map_err(|_| TaskPackageStoreError::Contract)?;
    let payload = canonical_bytes(approval)?;
    let payload_sha256 = artifact_sha256(&payload);
    database
        .with_immediate_transaction(|transaction| {
            require_exact_scope_revision(
                transaction,
                &approval.project_id,
                &approval.workspace_id,
                approval.expected_state_revision,
            )?;
            let proposal = proposal_binding(transaction, &approval.proposal_id)?;
            require_matching_proposal(
                &proposal,
                &approval.project_id,
                &approval.workspace_id,
                approval.expected_state_revision,
                &proposal.source_sha256,
                &approval.proposal_sha256,
            )?;
            let attestation: Option<(String, String, String, String, i64, String, Vec<u8>)> =
                transaction
                    .query_row(
                        "SELECT proposal_id, proposal_sha256, project_id, workspace_id, expected_state_revision, verifier_subject_sha256, payload FROM task_package_attestations WHERE attestation_id = ?1",
                        AC_params![approval.attestation_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                                row.get(6)?,
                            ))
                        },
                    )
                    .optional()?;
            let Some(attestation) = attestation else {
                return Err(conflict("task package attestation is unavailable"));
            };
            if attestation.0 != approval.proposal_id
                || attestation.1 != approval.proposal_sha256
                || attestation.2 != approval.project_id
                || attestation.3 != approval.workspace_id
                || attestation.4 != approval.expected_state_revision
                || artifact_sha256(&attestation.6) != approval.attestation_sha256
                || attestation.5 == approval.operator_subject_sha256
            {
                return Err(conflict("task package approval binding conflicts"));
            }
            let existing: Option<(String, Vec<u8>)> = transaction
                .query_row(
                    "SELECT approval_id, payload FROM task_package_approvals WHERE approval_id = ?1 OR proposal_id = ?2 OR attestation_id = ?3 OR payload_sha256 = ?4",
                    AC_params![
                        approval.approval_id,
                        approval.proposal_id,
                        approval.attestation_id,
                        payload_sha256,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let inserted = match existing {
                Some((approval_id, existing_payload))
                    if approval_id == approval.approval_id && existing_payload == payload =>
                {
                    false
                }
                Some(_) => return Err(conflict("task package approval binding conflicts")),
                None => {
                    transaction.execute(
                        "INSERT INTO task_package_approvals(approval_id, proposal_id, proposal_sha256, attestation_id, attestation_sha256, project_id, workspace_id, expected_state_revision, operator_subject_sha256, decision, approved_at, state, payload_sha256, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'approved', ?10, 'approved', ?11, ?12)",
                        AC_params![
                            approval.approval_id,
                            approval.proposal_id,
                            approval.proposal_sha256,
                            approval.attestation_id,
                            approval.attestation_sha256,
                            approval.project_id,
                            approval.workspace_id,
                            approval.expected_state_revision,
                            approval.operator_subject_sha256,
                            approval.approved_at,
                            payload_sha256,
                            payload,
                        ],
                    )?;
                    true
                }
            };
            let receipt = unchanged_revision_receipt(
                &proposal.request_id,
                "approve",
                &approval.project_id,
                &approval.workspace_id,
                &approval.approval_id,
                &payload_sha256,
                approval.expected_state_revision,
                "approved",
            )?;
            Ok(if inserted {
                TaskPackageWriteOutcome::Inserted(receipt)
            } else {
                TaskPackageWriteOutcome::Replay(receipt)
            })
        })
        .map_err(map_storage_error)
}

pub(crate) fn inspect_task_package_proposal(
    database: &ProjectDatabase,
    request_id: &str,
    project_id: &str,
    workspace_id: &str,
    expected_state_revision: i64,
    proposal_id: &str,
) -> Result<(TaskPackageProposal, TaskPackageOperationReceipt), TaskPackageStoreError> {
    database
        .with_immediate_transaction(|transaction| {
            require_exact_scope_revision(
                transaction,
                project_id,
                workspace_id,
                expected_state_revision,
            )?;
            let payload: Option<Vec<u8>> = transaction
                .query_row(
                    "SELECT payload FROM task_package_proposals WHERE proposal_id = ?1 AND project_id = ?2 AND workspace_id = ?3 AND expected_state_revision = ?4",
                    AC_params![proposal_id, project_id, workspace_id, expected_state_revision],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(payload) = payload else {
                return Err(conflict("task package proposal is unavailable"));
            };
            let proposal: TaskPackageProposal = serde_json::from_slice(&payload).map_err(|_| {
                TransactionalStorageError::Integrity(
                    "task package proposal payload is invalid".to_string(),
                )
            })?;
            validate_proposal(&proposal).map_err(|_| {
                TransactionalStorageError::Integrity(
                    "task package proposal contract is invalid".to_string(),
                )
            })?;
            if proposal.proposal_id != proposal_id
                || proposal.project_id != project_id
                || proposal.workspace_id != workspace_id
                || proposal.expected_state_revision != expected_state_revision
                || proposal_payload_sha256(&proposal).map_err(|_| {
                    TransactionalStorageError::Integrity(
                        "task package proposal digest is invalid".to_string(),
                    )
                })? != proposal.payload_sha256
            {
                return Err(conflict("task package proposal binding conflicts"));
            }
            let receipt = unchanged_revision_receipt(
                request_id,
                "inspect",
                project_id,
                workspace_id,
                &deterministic_receipt_id(request_id, proposal_id),
                &proposal.payload_sha256,
                expected_state_revision,
                "inspected",
            )?;
            Ok((proposal, receipt))
        })
        .map_err(map_storage_error)
}

fn proposal_binding(
    transaction: &AC_Transaction<'_>,
    proposal_id: &str,
) -> TransactionalStorageResult<ProposalBinding> {
    transaction
        .query_row(
            "SELECT request_id, project_id, workspace_id, expected_state_revision, source_sha256, payload_sha256 FROM task_package_proposals WHERE proposal_id = ?1",
            AC_params![proposal_id],
            |row| {
                Ok(ProposalBinding {
                    request_id: row.get(0)?,
                    project_id: row.get(1)?,
                    workspace_id: row.get(2)?,
                    expected_state_revision: row.get(3)?,
                    source_sha256: row.get(4)?,
                    payload_sha256: row.get(5)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| conflict("task package proposal is unavailable"))
}

fn require_matching_proposal(
    proposal: &ProposalBinding,
    project_id: &str,
    workspace_id: &str,
    expected_state_revision: i64,
    source_sha256: &str,
    proposal_sha256: &str,
) -> TransactionalStorageResult<()> {
    if proposal.project_id != project_id
        || proposal.workspace_id != workspace_id
        || proposal.expected_state_revision != expected_state_revision
        || proposal.source_sha256 != source_sha256
        || proposal.payload_sha256 != proposal_sha256
    {
        return Err(conflict("task package proposal scope or digest conflicts"));
    }
    Ok(())
}

fn require_exact_scope_revision(
    transaction: &AC_Transaction<'_>,
    project_id: &str,
    workspace_id: &str,
    expected_state_revision: i64,
) -> TransactionalStorageResult<()> {
    let revision: Option<i64> = transaction
        .query_row(
            "SELECT projects.state_revision FROM projects JOIN workspaces ON workspaces.project_id = projects.project_id WHERE projects.project_id = ?1 AND workspaces.workspace_id = ?2 AND workspaces.status = 'active'",
            AC_params![project_id, workspace_id],
            |row| row.get(0),
        )
        .optional()?;
    if revision != Some(expected_state_revision) {
        return Err(TransactionalStorageError::InvalidInput(
            "task package scope or state revision is unavailable".to_string(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn unchanged_revision_receipt(
    request_id: &str,
    operation: &str,
    project_id: &str,
    workspace_id: &str,
    artifact_id: &str,
    artifact_sha256: &str,
    state_revision: i64,
    state: &str,
) -> TransactionalStorageResult<TaskPackageOperationReceipt> {
    let receipt = TaskPackageOperationReceipt {
        contract_version: TASK_PACKAGE_OPERATION_RECEIPT_V1.to_string(),
        request_id: request_id.to_string(),
        operation: operation.to_string(),
        project_id: project_id.to_string(),
        workspace_id: workspace_id.to_string(),
        artifact_id: artifact_id.to_string(),
        artifact_sha256: artifact_sha256.to_string(),
        previous_state_revision: state_revision,
        resulting_state_revision: state_revision,
        state: state.to_string(),
    };
    validate_operation_receipt(&receipt).map_err(|_| {
        TransactionalStorageError::Integrity(
            "task package operation receipt is invalid".to_string(),
        )
    })?;
    Ok(receipt)
}

fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, TaskPackageStoreError> {
    serde_json::to_vec(value).map_err(|_| TaskPackageStoreError::Contract)
}

fn artifact_sha256(payload: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = sha256(payload);
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn deterministic_receipt_id(request_id: &str, artifact_id: &str) -> String {
    let mut material = b"task-package-inspection-receipt-v1".to_vec();
    material.push(0);
    material.extend_from_slice(request_id.as_bytes());
    material.push(0);
    material.extend_from_slice(artifact_id.as_bytes());
    let digest = sha256(&material);
    let mut opaque = [0_u8; 16];
    opaque.copy_from_slice(&digest[..16]);
    crockford_id("rcp_", &opaque)
}

fn conflict(message: &str) -> TransactionalStorageError {
    TransactionalStorageError::Integrity(message.to_string())
}

fn map_storage_error(error: TransactionalStorageError) -> TaskPackageStoreError {
    match error {
        TransactionalStorageError::InvalidInput(_) => TaskPackageStoreError::Scope,
        TransactionalStorageError::Integrity(message) if message.starts_with("task package") => {
            TaskPackageStoreError::Conflict
        }
        _ => TaskPackageStoreError::Storage,
    }
}

#[cfg(test)]
#[path = "tests/Task_Package_Store_test.rs"]
mod task_package_store_tests;
