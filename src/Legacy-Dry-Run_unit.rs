use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::structs::{ProjectDatabase, STORAGE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::legacy_normalization::{
    validate_existing_normalization, LegacyNormalizationError, ValidatedLegacyNormalization,
};
use crate::legacy_staging::{
    lowercase_hex, require_private_directory, sync_directory, valid_sha256, write_private_json,
    SemanticCounts,
};
use crate::limited_protocol::valid_opaque_id;

const DRY_RUN_REQUEST_CONTRACT: &str = "legacy-dry-run-request-v1";
const DRY_RUN_REPORT_CONTRACT: &str = "legacy-dry-run-report-v1";
const TARGET_CONTRACT: &str = "canonical-state-v1";
const MAX_CONTROL_DOCUMENT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyDryRunError {
    Request,
    Normalization,
    Manifest,
    Target,
    Destination,
}

impl From<LegacyNormalizationError> for LegacyDryRunError {
    fn from(_error: LegacyNormalizationError) -> Self {
        Self::Normalization
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyDryRunRequest {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) expected_source_snapshot_sha256: String,
    pub(crate) expected_staging_snapshot_sha256: String,
    pub(crate) expected_normalized_snapshot_sha256: String,
    pub(crate) expected_semantic_snapshot_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyDomainCounts {
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
pub(crate) struct LegacyDryRunDifference {
    pub(crate) code: String,
    pub(crate) severity: String,
    pub(crate) domain: String,
    pub(crate) source_count: u64,
    pub(crate) projected_count: u64,
    pub(crate) decision_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyDryRunChecks {
    pub(crate) normalized_equivalence: String,
    pub(crate) target_schema_migrations: String,
    pub(crate) target_integrity: String,
    pub(crate) canonical_import: String,
    pub(crate) target_projection: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyDryRunReport {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) source_snapshot_sha256: String,
    pub(crate) staging_snapshot_sha256: String,
    pub(crate) normalized_snapshot_sha256: String,
    pub(crate) semantic_snapshot_sha256: String,
    pub(crate) manifest_snapshot_sha256: String,
    pub(crate) target_contract_version: String,
    pub(crate) target_storage_schema_version: i32,
    pub(crate) source_counts: LegacyDomainCounts,
    pub(crate) projected_counts: LegacyDomainCounts,
    pub(crate) differences: Vec<LegacyDryRunDifference>,
    pub(crate) checks: LegacyDryRunChecks,
    pub(crate) target_project_preexisting: bool,
    pub(crate) comparison: String,
    pub(crate) cutover_readiness: String,
    pub(crate) approval_eligible: bool,
    pub(crate) approval_created: bool,
    pub(crate) cutover_performed: bool,
    pub(crate) manifest_state_before: String,
    pub(crate) manifest_state_after: String,
    pub(crate) state: String,
}

pub(crate) fn dry_run_request(
    migration_id: String,
    expected_source_snapshot_sha256: String,
    expected_staging_snapshot_sha256: String,
    expected_normalized_snapshot_sha256: String,
    expected_semantic_snapshot_sha256: String,
) -> LegacyDryRunRequest {
    LegacyDryRunRequest {
        contract_version: DRY_RUN_REQUEST_CONTRACT.to_string(),
        migration_id,
        expected_source_snapshot_sha256,
        expected_staging_snapshot_sha256,
        expected_normalized_snapshot_sha256,
        expected_semantic_snapshot_sha256,
    }
}

pub(crate) fn dry_run_legacy_migration(
    staging_parent: &Path,
    registry: &ProjectDatabase,
    request: &LegacyDryRunRequest,
) -> Result<LegacyDryRunReport, LegacyDryRunError> {
    validate_request(request)?;
    require_private_directory(staging_parent).map_err(|_| LegacyDryRunError::Destination)?;
    let migration_root = staging_parent.join(&request.migration_id);
    require_private_directory(&migration_root).map_err(|_| LegacyDryRunError::Destination)?;

    let manifest_path = migration_root.join("migration-manifest-v1.json");
    let manifest_before = read_private_file(&manifest_path, &migration_root)?;
    validate_staged_manifest(&manifest_before, request)?;
    let manifest_snapshot_sha256 = lowercase_hex(&sha256(&manifest_before));

    let normalized = validate_existing_normalization(
        staging_parent,
        &request.migration_id,
        &request.expected_source_snapshot_sha256,
        &request.expected_staging_snapshot_sha256,
        &request.expected_normalized_snapshot_sha256,
        &request.expected_semantic_snapshot_sha256,
    )?;
    let target_project_preexisting = registry
        .contains_resource_scope(&normalized.receipt.project_id, None, None)
        .map_err(|_| LegacyDryRunError::Target)?;

    let final_root = migration_root.join("dry-run");
    if final_root.exists() {
        require_private_directory(&final_root).map_err(|_| LegacyDryRunError::Destination)?;
        let expected = build_report(
            &normalized,
            &manifest_snapshot_sha256,
            target_project_preexisting,
        );
        let existing: LegacyDryRunReport = read_private_json(
            &final_root.join("legacy-dry-run-report-v1.json"),
            &final_root,
        )?;
        validate_shadow_database(&final_root.join("shadow"))?;
        let manifest_after = read_private_file(&manifest_path, &migration_root)?;
        if existing != expected || manifest_after != manifest_before {
            return Err(LegacyDryRunError::Destination);
        }
        return Ok(existing);
    }

    let partial_root = migration_root.join("dry-run.partial");
    if partial_root.exists() {
        return Err(LegacyDryRunError::Destination);
    }
    create_private_directory(&partial_root)?;
    let shadow_root = partial_root.join("shadow");
    create_private_directory(&shadow_root)?;
    let shadow =
        ProjectDatabase::initialize_at(&shadow_root).map_err(|_| LegacyDryRunError::Target)?;
    validate_shadow(&shadow)?;
    drop(shadow);

    let report = build_report(
        &normalized,
        &manifest_snapshot_sha256,
        target_project_preexisting,
    );
    write_private_json(&partial_root.join("legacy-dry-run-report-v1.json"), &report)
        .map_err(|_| LegacyDryRunError::Destination)?;
    let manifest_after = read_private_file(&manifest_path, &migration_root)?;
    if manifest_after != manifest_before {
        return Err(LegacyDryRunError::Manifest);
    }
    sync_directory(&shadow_root.join("state")).map_err(|_| LegacyDryRunError::Destination)?;
    sync_directory(&shadow_root).map_err(|_| LegacyDryRunError::Destination)?;
    sync_directory(&partial_root).map_err(|_| LegacyDryRunError::Destination)?;
    fs::rename(&partial_root, &final_root).map_err(|_| LegacyDryRunError::Destination)?;
    sync_directory(&migration_root).map_err(|_| LegacyDryRunError::Destination)?;
    if read_private_file(&manifest_path, &migration_root)? != manifest_before {
        return Err(LegacyDryRunError::Manifest);
    }
    Ok(report)
}

fn validate_request(request: &LegacyDryRunRequest) -> Result<(), LegacyDryRunError> {
    if request.contract_version != DRY_RUN_REQUEST_CONTRACT
        || !valid_opaque_id(&request.migration_id, "mig_")
        || !valid_sha256(&request.expected_source_snapshot_sha256)
        || !valid_sha256(&request.expected_staging_snapshot_sha256)
        || !valid_sha256(&request.expected_normalized_snapshot_sha256)
        || !valid_sha256(&request.expected_semantic_snapshot_sha256)
    {
        return Err(LegacyDryRunError::Request);
    }
    Ok(())
}

fn validate_staged_manifest(
    bytes: &[u8],
    request: &LegacyDryRunRequest,
) -> Result<(), LegacyDryRunError> {
    let manifest: Value = serde_json::from_slice(bytes).map_err(|_| LegacyDryRunError::Manifest)?;
    if manifest["contract_version"] != "migration-manifest-v1"
        || manifest["migration_id"] != request.migration_id
        || manifest["source_snapshot_sha256"] != request.expected_source_snapshot_sha256
        || manifest["staging_snapshot_sha256"] != request.expected_staging_snapshot_sha256
        || manifest["target_schema_version"] != TARGET_CONTRACT
        || manifest["state"] != "staged"
        || manifest["approved_at"] != Value::Null
        || manifest["cutover_at"] != Value::Null
        || manifest["rollback_deadline"] != Value::Null
        || manifest["approvals"]
            .as_array()
            .is_none_or(|items| !items.is_empty())
        || manifest["rollback"]["status"] != "prepared"
        || manifest["rollback"]["post_cutover_first_sequence"] != Value::Null
    {
        return Err(LegacyDryRunError::Manifest);
    }
    Ok(())
}

fn build_report(
    normalized: &ValidatedLegacyNormalization,
    manifest_snapshot_sha256: &str,
    target_project_preexisting: bool,
) -> LegacyDryRunReport {
    let source_counts = domain_counts(&normalized.semantic_counts);
    let projected_counts = LegacyDomainCounts::zero();
    let mut differences = Vec::new();
    if target_project_preexisting {
        push_difference(
            &mut differences,
            "target_project_identity_conflict",
            "project",
            1,
        );
    } else {
        push_difference(
            &mut differences,
            "workspace_identity_mapping_missing",
            "workspace",
            1,
        );
    }
    push_nonzero_difference(
        &mut differences,
        "plan_identity_mapping_missing",
        "plans",
        source_counts.plans,
    );
    push_nonzero_difference(
        &mut differences,
        "task_identity_and_state_mapping_missing",
        "tasks",
        source_counts.tasks,
    );
    push_nonzero_difference(
        &mut differences,
        "submission_contract_mapping_missing",
        "submissions",
        source_counts.submissions,
    );
    push_nonzero_difference(
        &mut differences,
        "review_contract_mapping_missing",
        "reviews",
        source_counts.reviews,
    );
    push_nonzero_difference(
        &mut differences,
        "history_event_contract_mapping_missing",
        "history_events",
        source_counts.history_events,
    );
    push_nonzero_difference(
        &mut differences,
        "problem_projection_mapping_missing",
        "problems",
        source_counts.problems,
    );
    push_nonzero_difference(
        &mut differences,
        "transition_event_mapping_missing",
        "transitions",
        source_counts.transitions,
    );

    LegacyDryRunReport {
        contract_version: DRY_RUN_REPORT_CONTRACT.to_string(),
        migration_id: normalized.receipt.migration_id.clone(),
        project_id: normalized.receipt.project_id.clone(),
        source_snapshot_sha256: normalized.receipt.source_snapshot_sha256.clone(),
        staging_snapshot_sha256: normalized.receipt.staging_snapshot_sha256.clone(),
        normalized_snapshot_sha256: normalized.receipt.normalized_snapshot_sha256.clone(),
        semantic_snapshot_sha256: normalized.receipt.semantic_snapshot_sha256.clone(),
        manifest_snapshot_sha256: manifest_snapshot_sha256.to_string(),
        target_contract_version: TARGET_CONTRACT.to_string(),
        target_storage_schema_version: STORAGE_SCHEMA_VERSION,
        source_counts,
        projected_counts,
        differences,
        checks: LegacyDryRunChecks {
            normalized_equivalence: "passed".to_string(),
            target_schema_migrations: "passed".to_string(),
            target_integrity: "passed".to_string(),
            canonical_import: "not_materialized".to_string(),
            target_projection: "blocked_by_unmapped_state".to_string(),
        },
        target_project_preexisting,
        comparison: "differences_found".to_string(),
        cutover_readiness: "blocked".to_string(),
        approval_eligible: false,
        approval_created: false,
        cutover_performed: false,
        manifest_state_before: "staged".to_string(),
        manifest_state_after: "staged".to_string(),
        state: "dry_run_completed".to_string(),
    }
}

fn domain_counts(counts: &SemanticCounts) -> LegacyDomainCounts {
    LegacyDomainCounts {
        plans: counts.plans,
        tasks: counts.tasks,
        submissions: counts.submissions,
        reviews: counts.reviews,
        history_events: counts.history_events,
        problems: counts.problems,
        transitions: counts.transitions,
    }
}

impl LegacyDomainCounts {
    fn zero() -> Self {
        Self {
            plans: 0,
            tasks: 0,
            submissions: 0,
            reviews: 0,
            history_events: 0,
            problems: 0,
            transitions: 0,
        }
    }
}

fn push_nonzero_difference(
    differences: &mut Vec<LegacyDryRunDifference>,
    code: &str,
    domain: &str,
    count: u64,
) {
    if count != 0 {
        push_difference(differences, code, domain, count);
    }
}

fn push_difference(
    differences: &mut Vec<LegacyDryRunDifference>,
    code: &str,
    domain: &str,
    source_count: u64,
) {
    differences.push(LegacyDryRunDifference {
        code: code.to_string(),
        severity: "blocker".to_string(),
        domain: domain.to_string(),
        source_count,
        projected_count: 0,
        decision_required: true,
    });
}

fn validate_shadow_database(root: &Path) -> Result<(), LegacyDryRunError> {
    require_private_directory(root).map_err(|_| LegacyDryRunError::Destination)?;
    let database =
        ProjectDatabase::open_existing_at(root).map_err(|_| LegacyDryRunError::Target)?;
    validate_shadow(&database)
}

fn validate_shadow(database: &ProjectDatabase) -> Result<(), LegacyDryRunError> {
    let health = database.health().map_err(|_| LegacyDryRunError::Target)?;
    let projects = database
        .registered_projects()
        .map_err(|_| LegacyDryRunError::Target)?;
    if health.schema_version != STORAGE_SCHEMA_VERSION
        || health.quick_check != "ok"
        || health.foreign_key_violations != 0
        || health.trusted_schema
        || !health.foreign_keys
        || !projects.is_empty()
    {
        return Err(LegacyDryRunError::Target);
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), LegacyDryRunError> {
    fs::create_dir(path).map_err(|_| LegacyDryRunError::Destination)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| LegacyDryRunError::Destination)
}

fn read_private_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
    ownership_root: &Path,
) -> Result<T, LegacyDryRunError> {
    let bytes = read_private_file(path, ownership_root)?;
    serde_json::from_slice(&bytes).map_err(|_| LegacyDryRunError::Destination)
}

fn read_private_file(path: &Path, ownership_root: &Path) -> Result<Vec<u8>, LegacyDryRunError> {
    let root = fs::symlink_metadata(ownership_root).map_err(|_| LegacyDryRunError::Destination)?;
    if root.file_type().is_symlink() || !root.is_dir() || root.mode() & 0o777 != 0o700 {
        return Err(LegacyDryRunError::Destination);
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(path)
        .map_err(|_| LegacyDryRunError::Destination)?;
    let metadata = file
        .metadata()
        .map_err(|_| LegacyDryRunError::Destination)?;
    if !metadata.is_file()
        || metadata.uid() != root.uid()
        || metadata.mode() & 0o777 != 0o600
        || metadata.len() == 0
        || metadata.len() > MAX_CONTROL_DOCUMENT_BYTES
    {
        return Err(LegacyDryRunError::Destination);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| LegacyDryRunError::Destination)?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "tests/Legacy_Dry_Run_test.rs"]
mod legacy_dry_run_tests;
