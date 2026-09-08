use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::crockford_id;

use crate::task_package_contracts::{
    validate_generation_source, validate_proposal, TaskGenerationSource, TaskPackageContractError,
    TaskPackagePlan, TaskPackageProposal, TaskPackageTask, TASK_PACKAGE_PROPOSAL_V1,
};

const EMPTY_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";
// Reserve space in ADA1 for the receipt and envelope before any store write.
const MAX_PROPOSAL_BYTES: usize = 4 * 1024 * 1024 - 128 * 1024;

pub(crate) fn generate_task_package_proposal(
    source: &TaskGenerationSource,
) -> Result<TaskPackageProposal, TaskPackageContractError> {
    validate_generation_source(source)?;
    let proposal_id = deterministic_id(
        "prp_",
        "task-package-proposal-v1",
        &[
            &source.request_id,
            &source.source_sha256,
            &source.project_id,
            &source.workspace_id,
            &source.expected_state_revision.to_string(),
        ],
    );
    let mut task_count = 0_usize;
    let plans = source
        .plans
        .iter()
        .enumerate()
        .map(|(plan_ordinal, plan)| {
            let plan_id = deterministic_id(
                "pln_",
                "task-package-plan-v1",
                &[
                    &proposal_id,
                    &plan.source_key_sha256,
                    &plan_ordinal.to_string(),
                ],
            );
            let tasks = plan
                .tasks
                .iter()
                .enumerate()
                .map(|(task_ordinal, task)| {
                    task_count += 1;
                    Ok(TaskPackageTask {
                        task_id: deterministic_id(
                            "tsk_",
                            "task-package-task-v1",
                            &[
                                &proposal_id,
                                &plan_id,
                                &task.source_key_sha256,
                                &task_ordinal.to_string(),
                            ],
                        ),
                        task_ordinal: i64::try_from(task_ordinal)
                            .map_err(|_| TaskPackageContractError::Order)?,
                        task_revision: 1,
                        source_key_sha256: task.source_key_sha256.clone(),
                        objective: task.objective.clone(),
                        acceptance_criteria: task.acceptance_criteria.clone(),
                        relevant_paths: task.relevant_paths.clone(),
                    })
                })
                .collect::<Result<Vec<_>, TaskPackageContractError>>()?;
            Ok(TaskPackagePlan {
                plan_id,
                plan_ordinal: i64::try_from(plan_ordinal)
                    .map_err(|_| TaskPackageContractError::Order)?,
                source_key_sha256: plan.source_key_sha256.clone(),
                objective: plan.objective.clone(),
                tasks,
            })
        })
        .collect::<Result<Vec<_>, TaskPackageContractError>>()?;
    let mut proposal = TaskPackageProposal {
        contract_version: TASK_PACKAGE_PROPOSAL_V1.to_string(),
        proposal_id,
        request_id: source.request_id.clone(),
        project_id: source.project_id.clone(),
        workspace_id: source.workspace_id.clone(),
        expected_state_revision: source.expected_state_revision,
        source_sha256: source.source_sha256.clone(),
        payload_sha256: EMPTY_DIGEST.to_string(),
        source_plan_count: source.plans.len(),
        source_task_count: task_count,
        converted_plan_count: source.plans.len(),
        converted_task_count: task_count,
        unconverted_source_keys_sha256: Vec::new(),
        differences: Vec::new(),
        plans,
        warnings: Vec::new(),
        state: "prepared".to_string(),
    };
    proposal.payload_sha256 = proposal_payload_sha256(&proposal)?;
    validate_proposal(&proposal)?;
    Ok(proposal)
}

pub(crate) fn proposal_payload_sha256(
    proposal: &TaskPackageProposal,
) -> Result<String, TaskPackageContractError> {
    let mut unsigned = proposal.clone();
    unsigned.payload_sha256 = EMPTY_DIGEST.to_string();
    let bytes = serde_json::to_vec(&unsigned).map_err(|_| TaskPackageContractError::Content)?;
    if bytes.len() > MAX_PROPOSAL_BYTES {
        return Err(TaskPackageContractError::Content);
    }
    Ok(lowercase_hex(&sha256(&bytes)))
}

fn deterministic_id(prefix: &str, domain: &str, parts: &[&str]) -> String {
    let mut material = domain.as_bytes().to_vec();
    for part in parts {
        material.push(0);
        material.extend_from_slice(&(part.len() as u64).to_be_bytes());
        material.extend_from_slice(part.as_bytes());
    }
    let digest = sha256(&material);
    let mut opaque = [0_u8; 16];
    opaque.copy_from_slice(&digest[..16]);
    crockford_id(prefix, &opaque)
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
#[path = "tests/Task_Package_Generator_test.rs"]
mod task_package_generator_tests;
