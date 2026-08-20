use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use ac_ant_delirium::libres::Rusqlite_unit::AC_params;
use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::random_crockford_id;
use ac_ant_delirium::structs::{
    encode_canonical_task_projection_payload, validate_serialized_agent_projection_document,
    CanonicalStateStore, CanonicalTaskProjectionPayload, ProjectDatabase, ProjectedTaskAction,
    StorageProject, StorageWorkspace, CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION,
    STORAGE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::legacy_normalization::{
    validate_existing_normalization, LegacyNormalizationError, ValidatedLegacyNormalization,
};
use crate::legacy_staging::{
    lowercase_hex, require_private_directory, sync_directory, valid_sha256, write_private_bytes,
    write_private_json, SemanticCounts,
};
use crate::limited_protocol::valid_opaque_id;

const CONVERSION_REQUEST_CONTRACT: &str = "legacy-conversion-request-v1";
const IDENTITY_BINDINGS_CONTRACT: &str = "legacy-identity-bindings-v1";
const CONVERSION_REPORT_CONTRACT: &str = "legacy-conversion-report-v1";
const MAX_CONTROL_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyConversionError {
    Request,
    Normalization,
    DryRun,
    Source,
    Target,
    Destination,
}

impl From<LegacyNormalizationError> for LegacyConversionError {
    fn from(_error: LegacyNormalizationError) -> Self {
        Self::Normalization
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConversionRequest {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) expected_source_snapshot_sha256: String,
    pub(crate) expected_staging_snapshot_sha256: String,
    pub(crate) expected_normalized_snapshot_sha256: String,
    pub(crate) expected_semantic_snapshot_sha256: String,
    pub(crate) expected_dry_run_report_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyIdentityBinding {
    pub(crate) source_key_sha256: String,
    pub(crate) canonical_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyIdentityBindings {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) plans: Vec<LegacyIdentityBinding>,
    pub(crate) tasks: Vec<LegacyIdentityBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConversionCounts {
    pub(crate) plans: u64,
    pub(crate) tasks: u64,
    pub(crate) submissions: u64,
    pub(crate) reviews: u64,
    pub(crate) history_events: u64,
    pub(crate) problems: u64,
    pub(crate) transitions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConversionReport {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) source_snapshot_sha256: String,
    pub(crate) staging_snapshot_sha256: String,
    pub(crate) normalized_snapshot_sha256: String,
    pub(crate) semantic_snapshot_sha256: String,
    pub(crate) dry_run_report_sha256: String,
    pub(crate) manifest_snapshot_sha256: String,
    pub(crate) identity_bindings_sha256: String,
    pub(crate) current_task_projection_sha256: String,
    pub(crate) governance_errors_projection_sha256: String,
    pub(crate) target_storage_schema_version: i32,
    pub(crate) source_counts: LegacyConversionCounts,
    pub(crate) imported_counts: LegacyConversionCounts,
    pub(crate) active_plan_resolved: bool,
    pub(crate) current_task_resolved: bool,
    pub(crate) task_objectives_from_source: u64,
    pub(crate) task_objectives_fallback: u64,
    pub(crate) current_task_projection: String,
    pub(crate) governance_errors_projection: String,
    pub(crate) protected_problem_disposition: String,
    pub(crate) comparison: String,
    pub(crate) approval_eligible: bool,
    pub(crate) approval_created: bool,
    pub(crate) cutover_performed: bool,
    pub(crate) manifest_state_before: String,
    pub(crate) manifest_state_after: String,
    pub(crate) state: String,
}

struct ParsedPlan {
    source_key: String,
    canonical_id: String,
    payload: Vec<u8>,
    complete: bool,
    tasks: Vec<ParsedTask>,
}

struct ParsedTask {
    source_key: String,
    canonical_id: String,
    payload: Vec<u8>,
}

struct ProtectedRecord {
    kind: &'static str,
    ordinal: u64,
    source_key: String,
    payload: Vec<u8>,
}

pub(crate) fn conversion_request(
    migration_id: String,
    expected_source_snapshot_sha256: String,
    expected_staging_snapshot_sha256: String,
    expected_normalized_snapshot_sha256: String,
    expected_semantic_snapshot_sha256: String,
    expected_dry_run_report_sha256: String,
) -> LegacyConversionRequest {
    LegacyConversionRequest {
        contract_version: CONVERSION_REQUEST_CONTRACT.to_string(),
        migration_id,
        expected_source_snapshot_sha256,
        expected_staging_snapshot_sha256,
        expected_normalized_snapshot_sha256,
        expected_semantic_snapshot_sha256,
        expected_dry_run_report_sha256,
    }
}

pub(crate) fn convert_legacy_migration(
    staging_parent: &Path,
    request: &LegacyConversionRequest,
) -> Result<LegacyConversionReport, LegacyConversionError> {
    validate_request(request)?;
    require_private_directory(staging_parent).map_err(|_| LegacyConversionError::Destination)?;
    let migration_root = staging_parent.join(&request.migration_id);
    require_private_directory(&migration_root).map_err(|_| LegacyConversionError::Destination)?;

    let manifest_path = migration_root.join("migration-manifest-v1.json");
    let manifest_before = read_control_file(&manifest_path)?;
    validate_staged_manifest(&manifest_before, request)?;
    let dry_run_bytes = read_control_file(
        &migration_root
            .join("dry-run")
            .join("legacy-dry-run-report-v1.json"),
    )?;
    if lowercase_hex(&sha256(&dry_run_bytes)) != request.expected_dry_run_report_sha256 {
        return Err(LegacyConversionError::DryRun);
    }
    validate_dry_run(&dry_run_bytes, request)?;

    let normalized = validate_existing_normalization(
        staging_parent,
        &request.migration_id,
        &request.expected_source_snapshot_sha256,
        &request.expected_staging_snapshot_sha256,
        &request.expected_normalized_snapshot_sha256,
        &request.expected_semantic_snapshot_sha256,
    )?;
    let final_root = migration_root.join("converted");
    if final_root.exists() {
        return validate_existing_conversion(&final_root, request, &manifest_before);
    }
    let partial_root = migration_root.join("converted.partial");
    if partial_root.exists() {
        return Err(LegacyConversionError::Destination);
    }
    create_private_directory(&partial_root)?;
    let candidate_root = partial_root.join("candidate");
    create_private_directory(&candidate_root)?;

    let files = normalized
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.bytes.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let (plans, bindings, source_objectives, fallback_objectives) =
        parse_plans(&normalized, &files)?;
    let protected_records = parse_protected_records(&files)?;
    let orchestrator = parse_json(required_file(&files, "task_orchestrator.json")?)?;
    let active_plan_id = resolve_active_plan(&orchestrator, &plans)?;
    let current_task_id = resolve_current_task(&orchestrator, &plans, active_plan_id.as_deref())?;
    let workspace_id = random_crockford_id("wsp_").map_err(|_| LegacyConversionError::Target)?;
    let identity_bindings = LegacyIdentityBindings {
        contract_version: IDENTITY_BINDINGS_CONTRACT.to_string(),
        migration_id: request.migration_id.clone(),
        project_id: normalized.receipt.project_id.clone(),
        workspace_id: workspace_id.clone(),
        plans: bindings.0,
        tasks: bindings.1,
    };
    let identity_bindings_bytes =
        serde_json::to_vec_pretty(&identity_bindings).map_err(|_| LegacyConversionError::Target)?;

    let database = ProjectDatabase::initialize_at(&candidate_root)
        .map_err(|_| LegacyConversionError::Target)?;
    materialize_candidate(
        &database,
        &normalized,
        &workspace_id,
        &plans,
        &protected_records,
        active_plan_id.as_deref(),
        current_task_id.as_deref(),
    )?;
    database
        .initialize_empty_governance_snapshot(
            &normalized.receipt.project_id,
            &workspace_id,
            "migration-candidate",
        )
        .map_err(|_| LegacyConversionError::Target)?;
    let projections_root = partial_root.join("simulated-projections");
    create_private_directory(&projections_root)?;
    let current_projection = database
        .render_current_task_projection(
            &normalized.receipt.project_id,
            &workspace_id,
            "migration-candidate",
        )
        .map_err(|_| LegacyConversionError::Target)?;
    let governance_projection = database
        .render_governance_errors_projection(
            &normalized.receipt.project_id,
            &workspace_id,
            "migration-candidate",
        )
        .map_err(|_| LegacyConversionError::Target)?;
    validate_simulated_projections(
        &current_projection,
        &governance_projection,
        current_task_id.is_some(),
    )?;
    write_private_bytes(
        &projections_root.join("current_task.json"),
        &current_projection,
    )
    .map_err(|_| LegacyConversionError::Destination)?;
    write_private_bytes(
        &projections_root.join("governance_error.json"),
        &governance_projection,
    )
    .map_err(|_| LegacyConversionError::Destination)?;

    let source_counts = counts(&normalized.semantic_counts);
    let imported_counts = LegacyConversionCounts {
        plans: plans.len() as u64,
        tasks: plans.iter().map(|plan| plan.tasks.len() as u64).sum(),
        submissions: 0,
        reviews: 0,
        history_events: protected_records
            .iter()
            .filter(|r| r.kind == "history")
            .count() as u64,
        problems: protected_records
            .iter()
            .filter(|r| r.kind == "problem")
            .count() as u64,
        transitions: protected_records
            .iter()
            .filter(|r| r.kind == "transition")
            .count() as u64,
    };
    if source_counts != imported_counts {
        return Err(LegacyConversionError::Source);
    }
    let health = database
        .health()
        .map_err(|_| LegacyConversionError::Target)?;
    if health.schema_version != STORAGE_SCHEMA_VERSION
        || health.quick_check != "ok"
        || health.foreign_key_violations != 0
        || health.trusted_schema
        || !health.foreign_keys
    {
        return Err(LegacyConversionError::Target);
    }
    let report = LegacyConversionReport {
        contract_version: CONVERSION_REPORT_CONTRACT.to_string(),
        migration_id: request.migration_id.clone(),
        project_id: normalized.receipt.project_id.clone(),
        workspace_id,
        source_snapshot_sha256: request.expected_source_snapshot_sha256.clone(),
        staging_snapshot_sha256: request.expected_staging_snapshot_sha256.clone(),
        normalized_snapshot_sha256: request.expected_normalized_snapshot_sha256.clone(),
        semantic_snapshot_sha256: request.expected_semantic_snapshot_sha256.clone(),
        dry_run_report_sha256: request.expected_dry_run_report_sha256.clone(),
        manifest_snapshot_sha256: lowercase_hex(&sha256(&manifest_before)),
        identity_bindings_sha256: lowercase_hex(&sha256(&identity_bindings_bytes)),
        current_task_projection_sha256: lowercase_hex(&sha256(&current_projection)),
        governance_errors_projection_sha256: lowercase_hex(&sha256(&governance_projection)),
        target_storage_schema_version: STORAGE_SCHEMA_VERSION,
        source_counts,
        imported_counts,
        active_plan_resolved: active_plan_id.is_some(),
        current_task_resolved: current_task_id.is_some(),
        task_objectives_from_source: source_objectives,
        task_objectives_fallback: fallback_objectives,
        current_task_projection: "validated_private_simulation".to_string(),
        governance_errors_projection: "validated_empty_agent_correctable_view".to_string(),
        protected_problem_disposition: "retained_for_operator_not_agent_projected".to_string(),
        comparison: "equivalent".to_string(),
        approval_eligible: false,
        approval_created: false,
        cutover_performed: false,
        manifest_state_before: "staged".to_string(),
        manifest_state_after: "staged".to_string(),
        state: "candidate_ready_for_independent_attestation".to_string(),
    };
    write_private_bytes(
        &partial_root.join("legacy-identity-bindings-v1.json"),
        &identity_bindings_bytes,
    )
    .map_err(|_| LegacyConversionError::Destination)?;
    write_private_json(
        &partial_root.join("legacy-conversion-report-v1.json"),
        &report,
    )
    .map_err(|_| LegacyConversionError::Destination)?;
    drop(database);
    sync_directory(&candidate_root.join("state"))
        .map_err(|_| LegacyConversionError::Destination)?;
    sync_directory(&candidate_root).map_err(|_| LegacyConversionError::Destination)?;
    sync_directory(&projections_root).map_err(|_| LegacyConversionError::Destination)?;
    sync_directory(&partial_root).map_err(|_| LegacyConversionError::Destination)?;
    if read_control_file(&manifest_path)? != manifest_before {
        return Err(LegacyConversionError::Destination);
    }
    fs::rename(&partial_root, &final_root).map_err(|_| LegacyConversionError::Destination)?;
    sync_directory(&migration_root).map_err(|_| LegacyConversionError::Destination)?;
    Ok(report)
}

fn validate_request(request: &LegacyConversionRequest) -> Result<(), LegacyConversionError> {
    if request.contract_version != CONVERSION_REQUEST_CONTRACT
        || !valid_opaque_id(&request.migration_id, "mig_")
        || !valid_sha256(&request.expected_source_snapshot_sha256)
        || !valid_sha256(&request.expected_staging_snapshot_sha256)
        || !valid_sha256(&request.expected_normalized_snapshot_sha256)
        || !valid_sha256(&request.expected_semantic_snapshot_sha256)
        || !valid_sha256(&request.expected_dry_run_report_sha256)
    {
        return Err(LegacyConversionError::Request);
    }
    Ok(())
}

fn validate_staged_manifest(
    bytes: &[u8],
    request: &LegacyConversionRequest,
) -> Result<(), LegacyConversionError> {
    let value = parse_json(bytes)?;
    if value["contract_version"] != "migration-manifest-v1"
        || value["migration_id"] != request.migration_id
        || value["source_snapshot_sha256"] != request.expected_source_snapshot_sha256
        || value["staging_snapshot_sha256"] != request.expected_staging_snapshot_sha256
        || value["state"] != "staged"
        || value["approved_at"] != Value::Null
        || value["cutover_at"] != Value::Null
        || value["rollback_deadline"] != Value::Null
        || value["approvals"]
            .as_array()
            .is_none_or(|approvals| !approvals.is_empty())
    {
        return Err(LegacyConversionError::DryRun);
    }
    Ok(())
}

fn validate_dry_run(
    bytes: &[u8],
    request: &LegacyConversionRequest,
) -> Result<(), LegacyConversionError> {
    let value = parse_json(bytes)?;
    if value["contract_version"] != "legacy-dry-run-report-v1"
        || value["migration_id"] != request.migration_id
        || value["source_snapshot_sha256"] != request.expected_source_snapshot_sha256
        || value["staging_snapshot_sha256"] != request.expected_staging_snapshot_sha256
        || value["normalized_snapshot_sha256"] != request.expected_normalized_snapshot_sha256
        || value["semantic_snapshot_sha256"] != request.expected_semantic_snapshot_sha256
        || value["comparison"] != "differences_found"
        || value["cutover_readiness"] != "blocked"
        || value["approval_eligible"] != false
        || value["approval_created"] != false
        || value["cutover_performed"] != false
        || value["manifest_state_after"] != "staged"
    {
        return Err(LegacyConversionError::DryRun);
    }
    Ok(())
}

fn parse_plans(
    normalized: &ValidatedLegacyNormalization,
    files: &BTreeMap<&str, &[u8]>,
) -> Result<
    (
        Vec<ParsedPlan>,
        (Vec<LegacyIdentityBinding>, Vec<LegacyIdentityBinding>),
        u64,
        u64,
    ),
    LegacyConversionError,
> {
    let mut plans = Vec::new();
    let mut plan_bindings = Vec::new();
    let mut task_bindings = Vec::new();
    let mut source_objectives = 0_u64;
    let mut fallback_objectives = 0_u64;
    let mut seen_steps = BTreeSet::new();
    for (path, bytes) in files {
        if !path.starts_with("plan/") || !path.ends_with(".json") {
            continue;
        }
        let value = parse_json(bytes)?;
        let legacy_plan_id = required_text(&value, "plan_id")?;
        let plan_id = random_crockford_id("pln_").map_err(|_| LegacyConversionError::Target)?;
        let plan_source_key = format!("{path}\0{legacy_plan_id}");
        let plan_source_hash = digest_text(&plan_source_key);
        let tasks = value["tasks"]
            .as_array()
            .ok_or(LegacyConversionError::Source)?;
        let mut parsed_tasks = Vec::new();
        for task in tasks {
            let step_id = required_text(task, "step_id")?;
            if !seen_steps.insert(step_id.to_string()) {
                return Err(LegacyConversionError::Source);
            }
            if task["implemented"].as_bool() != Some(false)
                || task["task_reviewed"].as_bool() != Some(false)
                || task["review_status"].as_str() != Some("none")
            {
                return Err(LegacyConversionError::Source);
            }
            let task_id = random_crockford_id("tsk_").map_err(|_| LegacyConversionError::Target)?;
            let task_source_key = format!("{plan_source_hash}\0{step_id}");
            let objective = source_objective(task).unwrap_or_else(default_objective);
            let encoded =
                encode_canonical_task_projection_payload(&CanonicalTaskProjectionPayload {
                    contract_version: CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION.to_string(),
                    objective: objective.clone(),
                    allowed_action: ProjectedTaskAction::Implement,
                    acceptance_criteria: Vec::new(),
                    relevant_paths: Vec::new(),
                });
            let payload = match encoded {
                Ok(payload) => {
                    source_objectives += 1;
                    payload
                }
                Err(_) => {
                    fallback_objectives += 1;
                    encode_canonical_task_projection_payload(&CanonicalTaskProjectionPayload {
                        contract_version: CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION.to_string(),
                        objective: default_objective(),
                        allowed_action: ProjectedTaskAction::Implement,
                        acceptance_criteria: Vec::new(),
                        relevant_paths: Vec::new(),
                    })
                    .map_err(|_| LegacyConversionError::Target)?
                }
            };
            task_bindings.push(LegacyIdentityBinding {
                source_key_sha256: digest_text(&task_source_key),
                canonical_id: task_id.clone(),
            });
            parsed_tasks.push(ParsedTask {
                source_key: task_source_key,
                canonical_id: task_id,
                payload,
            });
        }
        plan_bindings.push(LegacyIdentityBinding {
            source_key_sha256: plan_source_hash,
            canonical_id: plan_id.clone(),
        });
        plans.push(ParsedPlan {
            source_key: plan_source_key,
            canonical_id: plan_id,
            payload: (*bytes).to_vec(),
            complete: value["complete_plan"].as_bool().unwrap_or(false),
            tasks: parsed_tasks,
        });
    }
    plans.sort_by(|left, right| left.source_key.cmp(&right.source_key));
    plan_bindings.sort_by(|left, right| left.source_key_sha256.cmp(&right.source_key_sha256));
    task_bindings.sort_by(|left, right| left.source_key_sha256.cmp(&right.source_key_sha256));
    if plans.len() as u64 != normalized.semantic_counts.plans
        || seen_steps.len() as u64 != normalized.semantic_counts.tasks
    {
        return Err(LegacyConversionError::Source);
    }
    Ok((
        plans,
        (plan_bindings, task_bindings),
        source_objectives,
        fallback_objectives,
    ))
}

fn parse_protected_records(
    files: &BTreeMap<&str, &[u8]>,
) -> Result<Vec<ProtectedRecord>, LegacyConversionError> {
    let mut records = Vec::new();
    let history = parse_json(required_file(files, "governance_history.json")?)?;
    for (ordinal, entry) in history["entries"]
        .as_array()
        .ok_or(LegacyConversionError::Source)?
        .iter()
        .enumerate()
    {
        records.push(record(
            "history",
            ordinal,
            entry["id"].as_str().unwrap_or("history"),
            entry,
        )?);
    }
    for (path, bytes) in files {
        if path.starts_with("problems/") && path.ends_with(".json") {
            let value = parse_json(bytes)?;
            records.push(record(
                "problem",
                records.iter().filter(|r| r.kind == "problem").count(),
                path,
                &value,
            )?);
        }
    }
    let transitions = parse_json(required_file(files, "global/transition_task.json")?)?;
    for (ordinal, entry) in transitions["transition_tasks"]
        .as_array()
        .ok_or(LegacyConversionError::Source)?
        .iter()
        .enumerate()
    {
        records.push(record(
            "transition",
            ordinal,
            entry["transition_task_id"].as_str().unwrap_or("transition"),
            entry,
        )?);
    }
    Ok(records)
}

fn record(
    kind: &'static str,
    ordinal: usize,
    source_key: &str,
    value: &Value,
) -> Result<ProtectedRecord, LegacyConversionError> {
    Ok(ProtectedRecord {
        kind,
        ordinal: ordinal as u64,
        source_key: source_key.to_string(),
        payload: serde_json::to_vec(value).map_err(|_| LegacyConversionError::Source)?,
    })
}

fn resolve_active_plan(
    orchestrator: &Value,
    plans: &[ParsedPlan],
) -> Result<Option<String>, LegacyConversionError> {
    let active = orchestrator["orchestration"]["active_plan"]
        .as_str()
        .unwrap_or("");
    if active.is_empty() {
        return Ok(None);
    }
    let matches = plans
        .iter()
        .filter(|plan| {
            let path = plan.source_key.split('\0').next().unwrap_or("");
            active.ends_with(path)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(LegacyConversionError::Source);
    }
    Ok(Some(matches[0].canonical_id.clone()))
}

fn resolve_current_task(
    orchestrator: &Value,
    plans: &[ParsedPlan],
    active_plan_id: Option<&str>,
) -> Result<Option<String>, LegacyConversionError> {
    let step = orchestrator["orchestration"]["current_step_id"]
        .as_str()
        .unwrap_or("");
    if step.is_empty() {
        return Ok(None);
    }
    let plan = plans
        .iter()
        .find(|plan| Some(plan.canonical_id.as_str()) == active_plan_id)
        .ok_or(LegacyConversionError::Source)?;
    let source_suffix = format!("\0{step}");
    let matches = plan
        .tasks
        .iter()
        .filter(|task| task.source_key.ends_with(&source_suffix))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(LegacyConversionError::Source);
    }
    Ok(Some(matches[0].canonical_id.clone()))
}

fn materialize_candidate(
    database: &ProjectDatabase,
    normalized: &ValidatedLegacyNormalization,
    workspace_id: &str,
    plans: &[ParsedPlan],
    records: &[ProtectedRecord],
    active_plan_id: Option<&str>,
    current_task_id: Option<&str>,
) -> Result<(), LegacyConversionError> {
    let project_id = &normalized.receipt.project_id;
    let project_token = project_id
        .strip_prefix("prj_")
        .ok_or(LegacyConversionError::Target)?;
    let workspace_token = workspace_id
        .strip_prefix("wsp_")
        .ok_or(LegacyConversionError::Target)?;
    database
        .ensure_project(&StorageProject {
            project_id: project_id.clone(),
            project_key: format!("legacy-candidate--{project_token}"),
            created_at: "migration-candidate".to_string(),
        })
        .map_err(|_| LegacyConversionError::Target)?;
    CanonicalStateStore::register_workspace(
        database,
        &StorageWorkspace {
            workspace_id: workspace_id.to_string(),
            project_id: project_id.clone(),
            workspace_key: format!("workspace--{workspace_token}"),
            created_at: "migration-candidate".to_string(),
        },
    )
    .map_err(|_| LegacyConversionError::Target)?;
    database.with_immediate_transaction(|transaction| {
        for plan in plans {
            transaction.execute(
                "INSERT INTO plans(plan_id, project_id, status, payload, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                AC_params![plan.canonical_id, project_id, if plan.complete { "completed" } else { "active" }, plan.payload, "migration-candidate"],
            )?;
            for task in &plan.tasks {
                let status = if Some(plan.canonical_id.as_str()) == active_plan_id { "available" } else { "blocked" };
                transaction.execute(
                    "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at) VALUES (?1, ?2, ?3, 1, ?4, 'not_required', ?5, ?6)",
                    AC_params![task.canonical_id, project_id, plan.canonical_id, status, task.payload, "migration-candidate"],
                )?;
            }
        }
        for record in records {
            transaction.execute(
                "INSERT INTO legacy_migration_records(project_id, migration_id, record_kind, ordinal, source_key_sha256, payload_sha256, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                AC_params![project_id, normalized.receipt.migration_id, record.kind, record.ordinal as i64, digest_text(&record.source_key), lowercase_hex(&sha256(&record.payload)), record.payload],
            )?;
        }
        transaction.execute(
            "UPDATE projects SET active_plan_id = ?2, current_task_id = ?3, state_revision = 1 WHERE project_id = ?1",
            AC_params![project_id, active_plan_id, current_task_id],
        )?;
        Ok(())
    }).map_err(|_| LegacyConversionError::Target)
}

fn validate_simulated_projections(
    current: &[u8],
    governance: &[u8],
    has_current: bool,
) -> Result<(), LegacyConversionError> {
    let current_value = parse_json(current)?;
    let governance_value = parse_json(governance)?;
    if current_value["contract_version"] != "agent-projection-v1"
        || current_value["artifact_kind"] != "current_task"
        || current_value["task"].is_null() == has_current
        || governance_value["contract_version"] != "agent-projection-v1"
        || governance_value["artifact_kind"] != "governance_errors"
        || governance_value["violations"]
            .as_array()
            .is_none_or(|values| !values.is_empty())
    {
        return Err(LegacyConversionError::Target);
    }
    validate_serialized_agent_projection_document(current, "migration current task projection")
        .map_err(|_| LegacyConversionError::Target)?;
    validate_serialized_agent_projection_document(
        governance,
        "migration governance errors projection",
    )
    .map_err(|_| LegacyConversionError::Target)?;
    Ok(())
}

fn validate_existing_conversion(
    final_root: &Path,
    request: &LegacyConversionRequest,
    manifest_before: &[u8],
) -> Result<LegacyConversionReport, LegacyConversionError> {
    require_private_directory(final_root).map_err(|_| LegacyConversionError::Destination)?;
    let report: LegacyConversionReport = serde_json::from_slice(&read_control_file(
        &final_root.join("legacy-conversion-report-v1.json"),
    )?)
    .map_err(|_| LegacyConversionError::Destination)?;
    if report.contract_version != CONVERSION_REPORT_CONTRACT
        || report.migration_id != request.migration_id
        || report.source_snapshot_sha256 != request.expected_source_snapshot_sha256
        || report.staging_snapshot_sha256 != request.expected_staging_snapshot_sha256
        || report.normalized_snapshot_sha256 != request.expected_normalized_snapshot_sha256
        || report.semantic_snapshot_sha256 != request.expected_semantic_snapshot_sha256
        || report.dry_run_report_sha256 != request.expected_dry_run_report_sha256
        || report.manifest_snapshot_sha256 != lowercase_hex(&sha256(manifest_before))
        || report.approval_created
        || report.cutover_performed
        || report.manifest_state_after != "staged"
    {
        return Err(LegacyConversionError::Destination);
    }
    let bindings_bytes = read_control_file(&final_root.join("legacy-identity-bindings-v1.json"))?;
    let current_projection =
        read_control_file(&final_root.join("simulated-projections/current_task.json"))?;
    let governance_projection =
        read_control_file(&final_root.join("simulated-projections/governance_error.json"))?;
    if report.identity_bindings_sha256 != lowercase_hex(&sha256(&bindings_bytes))
        || report.current_task_projection_sha256 != lowercase_hex(&sha256(&current_projection))
        || report.governance_errors_projection_sha256
            != lowercase_hex(&sha256(&governance_projection))
    {
        return Err(LegacyConversionError::Destination);
    }
    let bindings: LegacyIdentityBindings =
        serde_json::from_slice(&bindings_bytes).map_err(|_| LegacyConversionError::Destination)?;
    if bindings.contract_version != IDENTITY_BINDINGS_CONTRACT
        || bindings.migration_id != report.migration_id
        || bindings.project_id != report.project_id
        || bindings.workspace_id != report.workspace_id
        || bindings.plans.len() as u64 != report.imported_counts.plans
        || bindings.tasks.len() as u64 != report.imported_counts.tasks
    {
        return Err(LegacyConversionError::Destination);
    }
    validate_simulated_projections(
        &current_projection,
        &governance_projection,
        report.current_task_resolved,
    )?;
    let database = ProjectDatabase::open_existing_at(&final_root.join("candidate"))
        .map_err(|_| LegacyConversionError::Target)?;
    if database
        .health()
        .map_err(|_| LegacyConversionError::Target)?
        .schema_version
        != STORAGE_SCHEMA_VERSION
    {
        return Err(LegacyConversionError::Target);
    }
    database
        .with_immediate_transaction(|transaction| {
            let plans: i64 = transaction.query_row(
                "SELECT count(*) FROM plans WHERE project_id = ?1",
                AC_params![report.project_id],
                |row| row.get(0),
            )?;
            let tasks: i64 = transaction.query_row(
                "SELECT count(*) FROM tasks WHERE project_id = ?1",
                AC_params![report.project_id],
                |row| row.get(0),
            )?;
            let records: i64 = transaction.query_row(
                "SELECT count(*) FROM legacy_migration_records WHERE project_id = ?1 AND migration_id = ?2",
                AC_params![report.project_id, report.migration_id],
                |row| row.get(0),
            )?;
            let expected_records = report.imported_counts.history_events
                + report.imported_counts.problems
                + report.imported_counts.transitions;
            if plans != report.imported_counts.plans as i64
                || tasks != report.imported_counts.tasks as i64
                || records != expected_records as i64
            {
                return Err(ac_ant_delirium::structs::TransactionalStorageError::Integrity(
                    "legacy conversion candidate count mismatch".to_string(),
                ));
            }
            Ok(())
        })
        .map_err(|_| LegacyConversionError::Target)?;
    let migration_root = final_root
        .parent()
        .ok_or(LegacyConversionError::Destination)?;
    if read_control_file(&migration_root.join("migration-manifest-v1.json"))? != manifest_before {
        return Err(LegacyConversionError::Destination);
    }
    Ok(report)
}

fn source_objective(task: &Value) -> Option<String> {
    let prompt = task["task_prompt"].as_str()?;
    let remainder = prompt.split_once("Concrete goal:")?.1.trim();
    let objective = remainder
        .split(". ")
        .next()
        .unwrap_or(remainder)
        .trim()
        .trim_end_matches('.');
    (!objective.is_empty()).then(|| objective.to_string())
}

fn default_objective() -> String {
    "Implement the assigned project change and provide project-visible evidence".to_string()
}

fn counts(counts: &SemanticCounts) -> LegacyConversionCounts {
    LegacyConversionCounts {
        plans: counts.plans,
        tasks: counts.tasks,
        submissions: counts.submissions,
        reviews: counts.reviews,
        history_events: counts.history_events,
        problems: counts.problems,
        transitions: counts.transitions,
    }
}

fn required_file<'a>(
    files: &'a BTreeMap<&str, &[u8]>,
    path: &str,
) -> Result<&'a [u8], LegacyConversionError> {
    files
        .get(path)
        .copied()
        .ok_or(LegacyConversionError::Source)
}

fn required_text<'a>(value: &'a Value, key: &str) -> Result<&'a str, LegacyConversionError> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or(LegacyConversionError::Source)
}

fn parse_json(bytes: &[u8]) -> Result<Value, LegacyConversionError> {
    serde_json::from_slice(bytes).map_err(|_| LegacyConversionError::Source)
}

fn digest_text(value: &str) -> String {
    lowercase_hex(&sha256(value.as_bytes()))
}

fn create_private_directory(path: &Path) -> Result<(), LegacyConversionError> {
    fs::create_dir(path).map_err(|_| LegacyConversionError::Destination)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| LegacyConversionError::Destination)
}

pub(crate) fn read_control_file(path: &Path) -> Result<Vec<u8>, LegacyConversionError> {
    let parent = path.parent().ok_or(LegacyConversionError::Destination)?;
    let parent_metadata =
        fs::symlink_metadata(parent).map_err(|_| LegacyConversionError::Destination)?;
    if parent_metadata.file_type().is_symlink()
        || !parent_metadata.is_dir()
        || parent_metadata.mode() & 0o777 != 0o700
    {
        return Err(LegacyConversionError::Destination);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(path)
        .map_err(|_| LegacyConversionError::Destination)?;
    let metadata = file
        .metadata()
        .map_err(|_| LegacyConversionError::Destination)?;
    if !metadata.is_file()
        || metadata.uid() != parent_metadata.uid()
        || metadata.mode() & 0o777 != 0o600
        || metadata.len() == 0
        || metadata.len() as usize > MAX_CONTROL_DOCUMENT_BYTES
    {
        return Err(LegacyConversionError::Destination);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| LegacyConversionError::Destination)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(LegacyConversionError::Destination);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "tests/Legacy_Conversion_test.rs"]
mod legacy_conversion_tests;
