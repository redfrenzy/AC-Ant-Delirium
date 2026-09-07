use std::collections::HashSet;

use ac_ant_delirium::structs::{
    validate_project_relative_path, CapabilityActor, CapabilityOperation,
};
use serde::{Deserialize, Serialize};

pub(crate) const TASK_GENERATION_SOURCE_V1: &str = "task-generation-source-v1";
pub(crate) const TASK_PACKAGE_PROPOSAL_V1: &str = "task-package-proposal-v1";
pub(crate) const TASK_PACKAGE_ATTESTATION_V1: &str = "task-package-attestation-v1";
pub(crate) const TASK_PACKAGE_APPROVAL_V1: &str = "task-package-approval-v1";
pub(crate) const GOVERNANCE_TASK_PACKAGE_V2: &str = "governance-task-package-v2";
pub(crate) const MIGRATION_IDENTITY_ASSOCIATION_V1: &str = "migration-identity-association-v1";
pub(crate) const TASK_PACKAGE_OPERATION_RECEIPT_V1: &str = "task-package-operation-receipt-v1";
pub(crate) const MIGRATION_IDENTITY_ASSOCIATION_RECEIPT_V1: &str =
    "migration-identity-association-receipt-v1";

const MAX_PLANS: usize = 64;
const MAX_TASKS: usize = 1_024;
const MAX_CRITERIA: usize = 128;
const MAX_PATHS: usize = 256;
const MAX_WARNINGS: usize = 256;
const MAX_OBJECTIVE_BYTES: usize = 16_384;
const MAX_ITEM_BYTES: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskPackageContractError {
    Contract,
    Identity,
    Digest,
    Scope,
    Content,
    Order,
    Authority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskPackageAdministrativeAction {
    Prepare,
    Inspect,
    Attest,
    Approve,
    Install,
    IdentityAssociate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TaskPackageAuthorityPolicy {
    pub(crate) actor: CapabilityActor,
    pub(crate) operation: CapabilityOperation,
    pub(crate) exact_project: bool,
    pub(crate) exact_workspace: bool,
    pub(crate) task_scope_forbidden: bool,
}

pub(crate) fn authority_policy(
    action: TaskPackageAdministrativeAction,
) -> TaskPackageAuthorityPolicy {
    use TaskPackageAdministrativeAction as Action;
    let (actor, operation) = match action {
        Action::Prepare | Action::Approve | Action::IdentityAssociate => {
            (CapabilityActor::Operator, CapabilityOperation::StateAdmin)
        }
        Action::Inspect => (
            CapabilityActor::Operator,
            CapabilityOperation::DiagnosticPrivate,
        ),
        Action::Attest => (CapabilityActor::Ci, CapabilityOperation::AttestationCreate),
        Action::Install => (CapabilityActor::Ci, CapabilityOperation::InstallationAdmin),
    };
    TaskPackageAuthorityPolicy {
        actor,
        operation,
        exact_project: true,
        exact_workspace: true,
        task_scope_forbidden: true,
    }
}

pub(crate) fn authority_matches_exact_scope(
    action: TaskPackageAdministrativeAction,
    actor: CapabilityActor,
    operation: CapabilityOperation,
    capability_project_id: Option<&str>,
    capability_workspace_id: Option<&str>,
    capability_task_id: Option<&str>,
    requested_project_id: &str,
    requested_workspace_id: &str,
) -> bool {
    let policy = authority_policy(action);
    policy.actor == actor
        && policy.operation == operation
        && capability_project_id == Some(requested_project_id)
        && capability_workspace_id == Some(requested_workspace_id)
        && capability_task_id.is_none()
        && require_id(requested_project_id, "prj_").is_ok()
        && require_id(requested_workspace_id, "wsp_").is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskGenerationSource {
    pub(crate) contract_version: String,
    pub(crate) request_id: String,
    pub(crate) source_format: String,
    pub(crate) source_sha256: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) expected_state_revision: i64,
    pub(crate) plans: Vec<TaskGenerationSourcePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskGenerationSourcePlan {
    pub(crate) source_key_sha256: String,
    pub(crate) objective: String,
    pub(crate) tasks: Vec<TaskGenerationSourceTask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskGenerationSourceTask {
    pub(crate) source_key_sha256: String,
    pub(crate) objective: String,
    pub(crate) acceptance_criteria: Vec<String>,
    pub(crate) relevant_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskPackageProposal {
    pub(crate) contract_version: String,
    pub(crate) proposal_id: String,
    pub(crate) request_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) expected_state_revision: i64,
    pub(crate) source_sha256: String,
    pub(crate) payload_sha256: String,
    pub(crate) source_plan_count: usize,
    pub(crate) source_task_count: usize,
    pub(crate) converted_plan_count: usize,
    pub(crate) converted_task_count: usize,
    pub(crate) unconverted_source_keys_sha256: Vec<String>,
    pub(crate) differences: Vec<TaskPackageDifference>,
    pub(crate) plans: Vec<TaskPackagePlan>,
    pub(crate) warnings: Vec<String>,
    pub(crate) state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskPackagePlan {
    pub(crate) plan_id: String,
    pub(crate) plan_ordinal: i64,
    pub(crate) source_key_sha256: String,
    pub(crate) objective: String,
    pub(crate) tasks: Vec<TaskPackageTask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskPackageDifference {
    pub(crate) source_key_sha256: String,
    pub(crate) kind: String,
    pub(crate) summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskPackageTask {
    pub(crate) task_id: String,
    pub(crate) task_ordinal: i64,
    pub(crate) task_revision: i64,
    pub(crate) source_key_sha256: String,
    pub(crate) objective: String,
    pub(crate) acceptance_criteria: Vec<String>,
    pub(crate) relevant_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskPackageAttestation {
    pub(crate) contract_version: String,
    pub(crate) attestation_id: String,
    pub(crate) proposal_id: String,
    pub(crate) proposal_sha256: String,
    pub(crate) source_sha256: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) expected_state_revision: i64,
    pub(crate) verifier_subject_sha256: String,
    pub(crate) decision: String,
    pub(crate) attested_at: String,
    pub(crate) state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskPackageApproval {
    pub(crate) contract_version: String,
    pub(crate) approval_id: String,
    pub(crate) proposal_id: String,
    pub(crate) proposal_sha256: String,
    pub(crate) attestation_id: String,
    pub(crate) attestation_sha256: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) expected_state_revision: i64,
    pub(crate) operator_subject_sha256: String,
    pub(crate) decision: String,
    pub(crate) approved_at: String,
    pub(crate) state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GovernanceTaskPackageV2 {
    pub(crate) contract_version: String,
    pub(crate) request_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) expected_state_revision: i64,
    pub(crate) proposal_id: String,
    pub(crate) proposal_sha256: String,
    pub(crate) attestation_id: String,
    pub(crate) attestation_sha256: String,
    pub(crate) approval_id: String,
    pub(crate) approval_sha256: String,
    pub(crate) active_plan_id: String,
    pub(crate) current_task_id: String,
    pub(crate) plans: Vec<TaskPackagePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MigrationIdentityAssociation {
    pub(crate) contract_version: String,
    pub(crate) request_id: String,
    pub(crate) migration_id: String,
    pub(crate) cutover_id: String,
    pub(crate) approval_id: String,
    pub(crate) provisional_project_id: String,
    pub(crate) provisional_workspace_id: String,
    pub(crate) candidate_project_id: String,
    pub(crate) candidate_workspace_id: String,
    pub(crate) project_identity_sha256: String,
    pub(crate) workspace_identity_sha256: String,
    pub(crate) candidate_logical_state_sha256: String,
    pub(crate) cutover_binding_sha256: String,
    pub(crate) provisional_state_policy: String,
    pub(crate) expected_registry_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskPackageOperationReceipt {
    pub(crate) contract_version: String,
    pub(crate) request_id: String,
    pub(crate) operation: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) artifact_id: String,
    pub(crate) artifact_sha256: String,
    pub(crate) previous_state_revision: i64,
    pub(crate) resulting_state_revision: i64,
    pub(crate) state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MigrationIdentityAssociationReceipt {
    pub(crate) contract_version: String,
    pub(crate) request_id: String,
    pub(crate) association_id: String,
    pub(crate) migration_id: String,
    pub(crate) cutover_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) previous_registry_revision: i64,
    pub(crate) resulting_registry_revision: i64,
    pub(crate) association_sha256: String,
    pub(crate) state: String,
}

pub(crate) fn validate_generation_source(
    source: &TaskGenerationSource,
) -> Result<(), TaskPackageContractError> {
    if source.contract_version != TASK_GENERATION_SOURCE_V1
        || source.source_format != "ordered-objectives-v1"
    {
        return Err(TaskPackageContractError::Contract);
    }
    validate_request_scope(
        &source.request_id,
        &source.project_id,
        &source.workspace_id,
        source.expected_state_revision,
    )?;
    require_sha256(&source.source_sha256)?;
    validate_source_plans(&source.plans)
}

pub(crate) fn validate_proposal(
    proposal: &TaskPackageProposal,
) -> Result<(), TaskPackageContractError> {
    if proposal.contract_version != TASK_PACKAGE_PROPOSAL_V1 || proposal.state != "prepared" {
        return Err(TaskPackageContractError::Contract);
    }
    require_id(&proposal.proposal_id, "prp_")?;
    validate_request_scope(
        &proposal.request_id,
        &proposal.project_id,
        &proposal.workspace_id,
        proposal.expected_state_revision,
    )?;
    require_sha256(&proposal.source_sha256)?;
    require_sha256(&proposal.payload_sha256)?;
    validate_package_plans(&proposal.plans)?;
    let converted_task_count: usize = proposal.plans.iter().map(|plan| plan.tasks.len()).sum();
    if proposal.source_plan_count > MAX_PLANS
        || proposal.source_task_count > MAX_TASKS
        || proposal.source_plan_count < proposal.converted_plan_count
        || proposal.source_task_count < proposal.converted_task_count
        || proposal.converted_plan_count != proposal.plans.len()
        || proposal.converted_task_count != converted_task_count
        || proposal.unconverted_source_keys_sha256.len() > MAX_TASKS
        || proposal.differences.len() > MAX_WARNINGS
    {
        return Err(TaskPackageContractError::Content);
    }
    let mut unconverted_source_keys = HashSet::new();
    for source_key in &proposal.unconverted_source_keys_sha256 {
        require_sha256(source_key)?;
        if !unconverted_source_keys.insert(source_key) {
            return Err(TaskPackageContractError::Content);
        }
    }
    for difference in &proposal.differences {
        require_sha256(&difference.source_key_sha256)?;
        if !matches!(
            difference.kind.as_str(),
            "normalized" | "omitted" | "unsupported"
        ) || difference.summary.trim().is_empty()
            || difference.summary.len() > MAX_ITEM_BYTES
        {
            return Err(TaskPackageContractError::Content);
        }
    }
    validate_bounded_texts(&proposal.warnings, MAX_WARNINGS, MAX_ITEM_BYTES)
}

pub(crate) fn validate_attestation(
    attestation: &TaskPackageAttestation,
) -> Result<(), TaskPackageContractError> {
    if attestation.contract_version != TASK_PACKAGE_ATTESTATION_V1
        || attestation.decision != "accepted"
        || attestation.state != "attested"
    {
        return Err(TaskPackageContractError::Contract);
    }
    require_id(&attestation.attestation_id, "att_")?;
    require_id(&attestation.proposal_id, "prp_")?;
    validate_scope(
        &attestation.project_id,
        &attestation.workspace_id,
        attestation.expected_state_revision,
    )?;
    for digest in [
        &attestation.proposal_sha256,
        &attestation.source_sha256,
        &attestation.verifier_subject_sha256,
    ] {
        require_sha256(digest)?;
    }
    require_timestamp(&attestation.attested_at)
}

pub(crate) fn validate_approval(
    approval: &TaskPackageApproval,
) -> Result<(), TaskPackageContractError> {
    if approval.contract_version != TASK_PACKAGE_APPROVAL_V1
        || approval.decision != "approved"
        || approval.state != "approved"
    {
        return Err(TaskPackageContractError::Contract);
    }
    require_id(&approval.approval_id, "apr_")?;
    require_id(&approval.proposal_id, "prp_")?;
    require_id(&approval.attestation_id, "att_")?;
    validate_scope(
        &approval.project_id,
        &approval.workspace_id,
        approval.expected_state_revision,
    )?;
    for digest in [
        &approval.proposal_sha256,
        &approval.attestation_sha256,
        &approval.operator_subject_sha256,
    ] {
        require_sha256(digest)?;
    }
    require_timestamp(&approval.approved_at)
}

pub(crate) fn validate_governance_package_v2(
    package: &GovernanceTaskPackageV2,
) -> Result<(), TaskPackageContractError> {
    if package.contract_version != GOVERNANCE_TASK_PACKAGE_V2 {
        return Err(TaskPackageContractError::Contract);
    }
    validate_request_scope(
        &package.request_id,
        &package.project_id,
        &package.workspace_id,
        package.expected_state_revision,
    )?;
    require_id(&package.proposal_id, "prp_")?;
    require_id(&package.attestation_id, "att_")?;
    require_id(&package.approval_id, "apr_")?;
    require_id(&package.active_plan_id, "pln_")?;
    require_id(&package.current_task_id, "tsk_")?;
    for digest in [
        &package.proposal_sha256,
        &package.attestation_sha256,
        &package.approval_sha256,
    ] {
        require_sha256(digest)?;
    }
    validate_package_plans(&package.plans)?;
    let active = package
        .plans
        .iter()
        .find(|plan| plan.plan_id == package.active_plan_id)
        .ok_or(TaskPackageContractError::Scope)?;
    if !active
        .tasks
        .iter()
        .any(|task| task.task_id == package.current_task_id)
    {
        return Err(TaskPackageContractError::Scope);
    }
    Ok(())
}

pub(crate) fn validate_identity_association(
    association: &MigrationIdentityAssociation,
) -> Result<(), TaskPackageContractError> {
    if association.contract_version != MIGRATION_IDENTITY_ASSOCIATION_V1
        || !matches!(
            association.provisional_state_policy.as_str(),
            "require-empty" | "quarantine-preserve"
        )
    {
        return Err(TaskPackageContractError::Contract);
    }
    require_id(&association.request_id, "req_")?;
    require_id(&association.migration_id, "mig_")?;
    require_id(&association.cutover_id, "cut_")?;
    require_id(&association.approval_id, "apr_")?;
    require_id(&association.provisional_project_id, "prj_")?;
    require_id(&association.provisional_workspace_id, "wsp_")?;
    require_id(&association.candidate_project_id, "prj_")?;
    require_id(&association.candidate_workspace_id, "wsp_")?;
    if association.provisional_project_id == association.candidate_project_id
        || association.provisional_workspace_id == association.candidate_workspace_id
        || association.expected_registry_revision < 0
    {
        return Err(TaskPackageContractError::Scope);
    }
    for digest in [
        &association.project_identity_sha256,
        &association.workspace_identity_sha256,
        &association.candidate_logical_state_sha256,
        &association.cutover_binding_sha256,
    ] {
        require_sha256(digest)?;
    }
    Ok(())
}

pub(crate) fn validate_operation_receipt(
    receipt: &TaskPackageOperationReceipt,
) -> Result<(), TaskPackageContractError> {
    if receipt.contract_version != TASK_PACKAGE_OPERATION_RECEIPT_V1 {
        return Err(TaskPackageContractError::Contract);
    }
    validate_request_scope(
        &receipt.request_id,
        &receipt.project_id,
        &receipt.workspace_id,
        receipt.previous_state_revision,
    )?;
    require_sha256(&receipt.artifact_sha256)?;
    let (artifact_prefix, expected_state) = match receipt.operation.as_str() {
        "prepare" => ("prp_", "prepared"),
        "inspect" => ("rcp_", "inspected"),
        "attest" => ("att_", "attested"),
        "approve" => ("apr_", "approved"),
        "install" => ("rcp_", "installed"),
        _ => return Err(TaskPackageContractError::Contract),
    };
    require_id(&receipt.artifact_id, artifact_prefix)?;
    if receipt.state != expected_state
        || receipt.resulting_state_revision < receipt.previous_state_revision
    {
        return Err(TaskPackageContractError::Scope);
    }
    Ok(())
}

pub(crate) fn validate_identity_association_receipt(
    receipt: &MigrationIdentityAssociationReceipt,
) -> Result<(), TaskPackageContractError> {
    if receipt.contract_version != MIGRATION_IDENTITY_ASSOCIATION_RECEIPT_V1
        || receipt.state != "associated"
    {
        return Err(TaskPackageContractError::Contract);
    }
    require_id(&receipt.request_id, "req_")?;
    require_id(&receipt.association_id, "asn_")?;
    require_id(&receipt.migration_id, "mig_")?;
    require_id(&receipt.cutover_id, "cut_")?;
    require_id(&receipt.project_id, "prj_")?;
    require_id(&receipt.workspace_id, "wsp_")?;
    require_sha256(&receipt.association_sha256)?;
    if receipt.previous_registry_revision < 0
        || receipt.resulting_registry_revision
            != receipt
                .previous_registry_revision
                .checked_add(1)
                .ok_or(TaskPackageContractError::Scope)?
    {
        return Err(TaskPackageContractError::Scope);
    }
    Ok(())
}

fn validate_request_scope(
    request_id: &str,
    project_id: &str,
    workspace_id: &str,
    expected_state_revision: i64,
) -> Result<(), TaskPackageContractError> {
    require_id(request_id, "req_")?;
    validate_scope(project_id, workspace_id, expected_state_revision)
}

fn validate_scope(
    project_id: &str,
    workspace_id: &str,
    expected_state_revision: i64,
) -> Result<(), TaskPackageContractError> {
    require_id(project_id, "prj_")?;
    require_id(workspace_id, "wsp_")?;
    if expected_state_revision < 0 {
        return Err(TaskPackageContractError::Scope);
    }
    Ok(())
}

fn validate_source_plans(
    plans: &[TaskGenerationSourcePlan],
) -> Result<(), TaskPackageContractError> {
    if plans.is_empty() || plans.len() > MAX_PLANS {
        return Err(TaskPackageContractError::Content);
    }
    let mut plan_keys = HashSet::new();
    let mut task_keys = HashSet::new();
    let mut task_count = 0_usize;
    for plan in plans {
        require_sha256(&plan.source_key_sha256)?;
        require_objective(&plan.objective)?;
        if !plan_keys.insert(&plan.source_key_sha256) || plan.tasks.is_empty() {
            return Err(TaskPackageContractError::Order);
        }
        task_count = task_count
            .checked_add(plan.tasks.len())
            .ok_or(TaskPackageContractError::Content)?;
        for task in &plan.tasks {
            require_sha256(&task.source_key_sha256)?;
            if !task_keys.insert(&task.source_key_sha256) {
                return Err(TaskPackageContractError::Order);
            }
            validate_task_content(
                &task.objective,
                &task.acceptance_criteria,
                &task.relevant_paths,
            )?;
        }
    }
    if task_count > MAX_TASKS {
        return Err(TaskPackageContractError::Content);
    }
    Ok(())
}

fn validate_package_plans(plans: &[TaskPackagePlan]) -> Result<(), TaskPackageContractError> {
    if plans.is_empty() || plans.len() > MAX_PLANS {
        return Err(TaskPackageContractError::Content);
    }
    let mut plan_ids = HashSet::new();
    let mut task_ids = HashSet::new();
    let mut task_count = 0_usize;
    for (plan_index, plan) in plans.iter().enumerate() {
        require_id(&plan.plan_id, "pln_")?;
        require_sha256(&plan.source_key_sha256)?;
        require_objective(&plan.objective)?;
        if plan.plan_ordinal
            != i64::try_from(plan_index).map_err(|_| TaskPackageContractError::Order)?
            || !plan_ids.insert(&plan.plan_id)
            || plan.tasks.is_empty()
        {
            return Err(TaskPackageContractError::Order);
        }
        task_count = task_count
            .checked_add(plan.tasks.len())
            .ok_or(TaskPackageContractError::Content)?;
        for (task_index, task) in plan.tasks.iter().enumerate() {
            require_id(&task.task_id, "tsk_")?;
            require_sha256(&task.source_key_sha256)?;
            if task.task_ordinal
                != i64::try_from(task_index).map_err(|_| TaskPackageContractError::Order)?
                || task.task_revision != 1
                || !task_ids.insert(&task.task_id)
            {
                return Err(TaskPackageContractError::Order);
            }
            validate_task_content(
                &task.objective,
                &task.acceptance_criteria,
                &task.relevant_paths,
            )?;
        }
    }
    if task_count > MAX_TASKS {
        return Err(TaskPackageContractError::Content);
    }
    Ok(())
}

fn validate_task_content(
    objective: &str,
    criteria: &[String],
    paths: &[String],
) -> Result<(), TaskPackageContractError> {
    require_objective(objective)?;
    validate_bounded_texts(criteria, MAX_CRITERIA, MAX_ITEM_BYTES)?;
    if paths.len() > MAX_PATHS {
        return Err(TaskPackageContractError::Content);
    }
    let mut unique_paths = HashSet::new();
    for path in paths {
        validate_project_relative_path(path, "task package relevant path")
            .map_err(|_| TaskPackageContractError::Content)?;
        if !unique_paths.insert(path) {
            return Err(TaskPackageContractError::Content);
        }
    }
    Ok(())
}

fn validate_bounded_texts(
    values: &[String],
    maximum_items: usize,
    maximum_bytes: usize,
) -> Result<(), TaskPackageContractError> {
    if values.len() > maximum_items
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > maximum_bytes)
    {
        return Err(TaskPackageContractError::Content);
    }
    Ok(())
}

fn require_objective(value: &str) -> Result<(), TaskPackageContractError> {
    if value.trim().is_empty() || value.len() > MAX_OBJECTIVE_BYTES {
        return Err(TaskPackageContractError::Content);
    }
    Ok(())
}

fn require_id(value: &str, prefix: &str) -> Result<(), TaskPackageContractError> {
    if crate::limited_protocol::valid_opaque_id(value, prefix) {
        Ok(())
    } else {
        Err(TaskPackageContractError::Identity)
    }
}

fn require_sha256(value: &str) -> Result<(), TaskPackageContractError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(TaskPackageContractError::Digest)
    }
}

fn require_timestamp(value: &str) -> Result<(), TaskPackageContractError> {
    if value.strip_prefix("unix:").is_some_and(|seconds| {
        !seconds.is_empty() && seconds.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        Ok(())
    } else {
        Err(TaskPackageContractError::Content)
    }
}

#[cfg(test)]
#[path = "tests/Task_Package_Contracts_test.rs"]
mod task_package_contract_tests;
