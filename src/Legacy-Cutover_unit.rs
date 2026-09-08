use std::fs;
use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use ac_ant_delirium::libres::Rusqlite_unit::{
    AC_Backup, AC_Connection, AC_OpenFlags, AC_OptionalExtension, AC_params,
};
use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::crockford_id;
use ac_ant_delirium::structs::{
    NewAuditEvent, ProjectDatabase, StorageProject, StorageWorkspace, TransactionalStorageError,
    AUDIT_EVENT_CONTRACT_VERSION, STORAGE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::legacy_attestation::{logical_state_sha256, LegacyVerifierAttestation};
use crate::legacy_conversion::{read_control_file, LegacyConversionReport};
use crate::legacy_staging::{
    lowercase_hex, require_private_directory, rfc3339_utc_from_unix, sync_directory, valid_sha256,
    write_private_bytes,
};
use crate::limited_protocol::valid_opaque_id;

const CUTOVER_REQUEST_CONTRACT: &str = "migration-cutover-request-v1";
const CUTOVER_BINDING_CONTRACT: &str = "migration-cutover-binding-v1";
const CUTOVER_RECEIPT_CONTRACT: &str = "migration-cutover-receipt-v1";
pub(crate) const MINIMUM_ROLLBACK_WINDOW_SECONDS: i64 = 30 * 24 * 60 * 60;
const MAXIMUM_ROLLBACK_WINDOW_SECONDS: i64 = 10 * 366 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyCutoverError {
    Request,
    Candidate,
    Storage,
    Manifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyCutoverRequest {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) expected_approved_manifest_sha256: String,
    pub(crate) expected_source_snapshot_sha256: String,
    pub(crate) expected_staging_snapshot_sha256: String,
    pub(crate) expected_conversion_report_sha256: String,
    pub(crate) attestation_id: String,
    pub(crate) expected_attestation_sha256: String,
    pub(crate) approval_id: String,
    pub(crate) expected_approval_sha256: String,
    pub(crate) expected_candidate_logical_state_sha256: String,
    pub(crate) rollback_legacy_snapshot_sha256: String,
    pub(crate) rollback_window_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationCutoverBinding {
    contract_version: String,
    cutover_id: String,
    migration_id: String,
    project_id: String,
    workspace_id: String,
    service_subject_sha256: String,
    cutover_request_sha256: String,
    approved_manifest_sha256: String,
    approval_id: String,
    approval_sha256: String,
    attestation_id: String,
    attestation_sha256: String,
    candidate_logical_state_sha256: String,
    source_snapshot_sha256: String,
    staging_snapshot_sha256: String,
    rollback_legacy_snapshot_sha256: String,
    cutover_at_unix: i64,
    cutover_at: String,
    rollback_deadline_unix: i64,
    rollback_deadline: String,
    post_cutover_first_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyCutoverReceipt {
    pub(crate) contract_version: String,
    pub(crate) cutover_id: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) service_subject_sha256: String,
    pub(crate) cutover_request_sha256: String,
    pub(crate) approved_manifest_sha256: String,
    pub(crate) manifest_after_sha256: String,
    pub(crate) approval_id: String,
    pub(crate) approval_sha256: String,
    pub(crate) attestation_id: String,
    pub(crate) attestation_sha256: String,
    pub(crate) candidate_logical_state_sha256: String,
    pub(crate) canonical_binding_sha256: String,
    pub(crate) source_snapshot_sha256: String,
    pub(crate) staging_snapshot_sha256: String,
    pub(crate) rollback_legacy_snapshot_sha256: String,
    pub(crate) cutover_at: String,
    pub(crate) rollback_deadline: String,
    pub(crate) post_cutover_first_sequence: i64,
    pub(crate) rollback_status: String,
    pub(crate) manifest_state: String,
    pub(crate) origin_preserved: bool,
    pub(crate) cutover_performed: bool,
    pub(crate) state: String,
    pub(crate) cutover_sha256: String,
}

pub(crate) fn cutover_request(
    migration_id: String,
    project_id: String,
    workspace_id: String,
    expected_approved_manifest_sha256: String,
    expected_source_snapshot_sha256: String,
    expected_staging_snapshot_sha256: String,
    expected_conversion_report_sha256: String,
    attestation_id: String,
    expected_attestation_sha256: String,
    approval_id: String,
    expected_approval_sha256: String,
    expected_candidate_logical_state_sha256: String,
    rollback_legacy_snapshot_sha256: String,
    rollback_window_seconds: i64,
) -> LegacyCutoverRequest {
    LegacyCutoverRequest {
        contract_version: CUTOVER_REQUEST_CONTRACT.to_string(),
        migration_id,
        project_id,
        workspace_id,
        expected_approved_manifest_sha256,
        expected_source_snapshot_sha256,
        expected_staging_snapshot_sha256,
        expected_conversion_report_sha256,
        attestation_id,
        expected_attestation_sha256,
        approval_id,
        expected_approval_sha256,
        expected_candidate_logical_state_sha256,
        rollback_legacy_snapshot_sha256,
        rollback_window_seconds,
    }
}

pub(crate) fn cutover_legacy_candidate(
    staging_parent: &Path,
    projects_parent: &Path,
    registry: &ProjectDatabase,
    request: &LegacyCutoverRequest,
    service_subject_sha256: &str,
    trusted_now_unix: i64,
) -> Result<LegacyCutoverReceipt, LegacyCutoverError> {
    validate_request(request, service_subject_sha256, trusted_now_unix)?;
    require_private_directory(staging_parent).map_err(|_| LegacyCutoverError::Candidate)?;
    require_private_directory(projects_parent).map_err(|_| LegacyCutoverError::Storage)?;
    let migration_root = staging_parent.join(&request.migration_id);
    require_private_directory(&migration_root).map_err(|_| LegacyCutoverError::Candidate)?;
    let manifest_path = migration_root.join("migration-manifest-v1.json");
    let manifest_bytes =
        read_control_file(&manifest_path).map_err(|_| LegacyCutoverError::Manifest)?;
    let manifest: Value =
        serde_json::from_slice(&manifest_bytes).map_err(|_| LegacyCutoverError::Manifest)?;
    validate_approved_manifest(&manifest, &manifest_bytes, request)?;

    let converted_root = migration_root.join("converted");
    require_private_directory(&converted_root).map_err(|_| LegacyCutoverError::Candidate)?;
    let report_bytes = read_control_file(&converted_root.join("legacy-conversion-report-v1.json"))
        .map_err(|_| LegacyCutoverError::Candidate)?;
    if lowercase_hex(&sha256(&report_bytes)) != request.expected_conversion_report_sha256 {
        return Err(LegacyCutoverError::Candidate);
    }
    let report: LegacyConversionReport =
        serde_json::from_slice(&report_bytes).map_err(|_| LegacyCutoverError::Candidate)?;
    validate_report(&report, request)?;

    let candidate_root = converted_root.join("candidate");
    let candidate = ProjectDatabase::open_existing_at(&candidate_root)
        .map_err(|_| LegacyCutoverError::Storage)?;
    validate_candidate(&candidate, request, &report)?;
    let (project, workspace) = candidate_identity(&candidate, request)?;
    let request_sha256 = lowercase_hex(&sha256(
        &serde_json::to_vec(request).map_err(|_| LegacyCutoverError::Request)?,
    ));
    let final_root = projects_parent.join(&project.project_key);
    let partial_root = projects_parent.join(format!(".{}-cutover.partial", request.migration_id));

    let (promoted, was_partial) = match fs::symlink_metadata(&final_root) {
        Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_dir() => (
            ProjectDatabase::open_existing_at(&final_root)
                .map_err(|_| LegacyCutoverError::Storage)?,
            false,
        ),
        Ok(_) => return Err(LegacyCutoverError::Storage),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let partial_exists = match fs::symlink_metadata(&partial_root) {
                Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_dir() => true,
                Ok(_) => return Err(LegacyCutoverError::Storage),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(_) => return Err(LegacyCutoverError::Storage),
            };
            if !partial_exists {
                create_consistent_candidate_copy(&candidate, &partial_root)?;
            }
            (
                ProjectDatabase::open_existing_at(&partial_root)
                    .map_err(|_| LegacyCutoverError::Storage)?,
                true,
            )
        }
        Err(_) => return Err(LegacyCutoverError::Storage),
    };
    require_exact_project(&promoted, &project)?;
    validate_candidate_logical_state(&promoted, request, &report)?;

    let binding = match load_cutover_binding(&promoted, request)? {
        Some(binding) => {
            validate_binding(&binding, request, service_subject_sha256, &request_sha256)?;
            binding
        }
        None => {
            if manifest["state"] == "cutover" {
                return Err(LegacyCutoverError::Manifest);
            }
            let first_sequence = promoted
                .last_event_sequence(&request.project_id)
                .map_err(|_| LegacyCutoverError::Storage)?
                .checked_add(1)
                .ok_or(LegacyCutoverError::Storage)?;
            let rollback_deadline_unix = trusted_now_unix
                .checked_add(request.rollback_window_seconds)
                .ok_or(LegacyCutoverError::Request)?;
            let binding = MigrationCutoverBinding {
                contract_version: CUTOVER_BINDING_CONTRACT.to_string(),
                cutover_id: deterministic_id("cut_", &request.approval_id, &request.migration_id),
                migration_id: request.migration_id.clone(),
                project_id: request.project_id.clone(),
                workspace_id: request.workspace_id.clone(),
                service_subject_sha256: service_subject_sha256.to_string(),
                cutover_request_sha256: request_sha256.clone(),
                approved_manifest_sha256: request.expected_approved_manifest_sha256.clone(),
                approval_id: request.approval_id.clone(),
                approval_sha256: request.expected_approval_sha256.clone(),
                attestation_id: request.attestation_id.clone(),
                attestation_sha256: request.expected_attestation_sha256.clone(),
                candidate_logical_state_sha256: request
                    .expected_candidate_logical_state_sha256
                    .clone(),
                source_snapshot_sha256: request.expected_source_snapshot_sha256.clone(),
                staging_snapshot_sha256: request.expected_staging_snapshot_sha256.clone(),
                rollback_legacy_snapshot_sha256: request.rollback_legacy_snapshot_sha256.clone(),
                cutover_at_unix: trusted_now_unix,
                cutover_at: rfc3339_utc_from_unix(trusted_now_unix)
                    .map_err(|_| LegacyCutoverError::Request)?,
                rollback_deadline_unix,
                rollback_deadline: rfc3339_utc_from_unix(rollback_deadline_unix)
                    .map_err(|_| LegacyCutoverError::Request)?,
                post_cutover_first_sequence: first_sequence,
            };
            let outcome = promoted
                .record_event(&cutover_event(&binding)?)
                .map_err(|_| LegacyCutoverError::Storage)?;
            if outcome.sequence() != first_sequence {
                return Err(LegacyCutoverError::Storage);
            }
            binding
        }
    };
    let binding_bytes = serde_json::to_vec(&binding).map_err(|_| LegacyCutoverError::Storage)?;
    let binding_sha256 = lowercase_hex(&sha256(&binding_bytes));

    if was_partial {
        fs::rename(&partial_root, &final_root).map_err(|_| LegacyCutoverError::Storage)?;
        sync_directory(projects_parent).map_err(|_| LegacyCutoverError::Storage)?;
    }
    let canonical =
        ProjectDatabase::open_existing_at(&final_root).map_err(|_| LegacyCutoverError::Storage)?;
    require_exact_project(&canonical, &project)?;
    validate_candidate_logical_state(&canonical, request, &report)?;
    require_exact_cutover_event(
        &canonical,
        &binding,
        &binding_sha256,
        Some(binding.post_cutover_first_sequence),
    )?;

    register_canonical_authority(registry, &project, &workspace, &binding, &binding_sha256)?;

    let cutover_manifest = cutover_manifest_value(&manifest, request, &binding)?;
    let cutover_manifest_bytes =
        serde_json::to_vec_pretty(&cutover_manifest).map_err(|_| LegacyCutoverError::Manifest)?;
    match manifest["state"].as_str() {
        Some("approved") => {
            publish_cutover_manifest(&migration_root, &manifest_path, &cutover_manifest_bytes)?
        }
        Some("cutover") if manifest_bytes == cutover_manifest_bytes => {}
        _ => return Err(LegacyCutoverError::Manifest),
    }
    let manifest_after_sha256 = lowercase_hex(&sha256(&cutover_manifest_bytes));
    let mut receipt = LegacyCutoverReceipt {
        contract_version: CUTOVER_RECEIPT_CONTRACT.to_string(),
        cutover_id: binding.cutover_id.clone(),
        migration_id: binding.migration_id.clone(),
        project_id: binding.project_id.clone(),
        workspace_id: binding.workspace_id.clone(),
        service_subject_sha256: binding.service_subject_sha256.clone(),
        cutover_request_sha256: binding.cutover_request_sha256.clone(),
        approved_manifest_sha256: binding.approved_manifest_sha256.clone(),
        manifest_after_sha256,
        approval_id: binding.approval_id.clone(),
        approval_sha256: binding.approval_sha256.clone(),
        attestation_id: binding.attestation_id.clone(),
        attestation_sha256: binding.attestation_sha256.clone(),
        candidate_logical_state_sha256: binding.candidate_logical_state_sha256.clone(),
        canonical_binding_sha256: binding_sha256,
        source_snapshot_sha256: binding.source_snapshot_sha256.clone(),
        staging_snapshot_sha256: binding.staging_snapshot_sha256.clone(),
        rollback_legacy_snapshot_sha256: binding.rollback_legacy_snapshot_sha256.clone(),
        cutover_at: binding.cutover_at.clone(),
        rollback_deadline: binding.rollback_deadline.clone(),
        post_cutover_first_sequence: binding.post_cutover_first_sequence,
        rollback_status: "available".to_string(),
        manifest_state: "cutover".to_string(),
        origin_preserved: true,
        cutover_performed: true,
        state: "cutover_awaiting_verification".to_string(),
        cutover_sha256: String::new(),
    };
    receipt.cutover_sha256 = receipt_sha256(&receipt)?;
    Ok(receipt)
}

fn validate_request(
    request: &LegacyCutoverRequest,
    service_subject_sha256: &str,
    trusted_now_unix: i64,
) -> Result<(), LegacyCutoverError> {
    let hashes = [
        request.expected_approved_manifest_sha256.as_str(),
        request.expected_source_snapshot_sha256.as_str(),
        request.expected_staging_snapshot_sha256.as_str(),
        request.expected_conversion_report_sha256.as_str(),
        request.expected_attestation_sha256.as_str(),
        request.expected_approval_sha256.as_str(),
        request.expected_candidate_logical_state_sha256.as_str(),
        request.rollback_legacy_snapshot_sha256.as_str(),
        service_subject_sha256,
    ];
    if request.contract_version != CUTOVER_REQUEST_CONTRACT
        || !valid_opaque_id(&request.migration_id, "mig_")
        || !valid_opaque_id(&request.project_id, "prj_")
        || !valid_opaque_id(&request.workspace_id, "wsp_")
        || !valid_opaque_id(&request.attestation_id, "att_")
        || !valid_opaque_id(&request.approval_id, "apr_")
        || hashes.iter().any(|value| !valid_sha256(value))
        || request.rollback_legacy_snapshot_sha256 != request.expected_source_snapshot_sha256
        || !(MINIMUM_ROLLBACK_WINDOW_SECONDS..=MAXIMUM_ROLLBACK_WINDOW_SECONDS)
            .contains(&request.rollback_window_seconds)
        || trusted_now_unix < 0
    {
        return Err(LegacyCutoverError::Request);
    }
    Ok(())
}

fn validate_approved_manifest(
    manifest: &Value,
    bytes: &[u8],
    request: &LegacyCutoverRequest,
) -> Result<(), LegacyCutoverError> {
    let state = manifest["state"].as_str();
    if manifest["contract_version"] != "migration-manifest-v1"
        || manifest["migration_id"] != request.migration_id
        || manifest["project_id"] != request.project_id
        || manifest["source_snapshot_sha256"] != request.expected_source_snapshot_sha256
        || manifest["staging_snapshot_sha256"] != request.expected_staging_snapshot_sha256
        || !matches!(state, Some("approved" | "cutover"))
        || manifest["approved_at"].as_str().is_none()
        || manifest["rollback"]["legacy_snapshot_sha256"] != request.rollback_legacy_snapshot_sha256
    {
        return Err(LegacyCutoverError::Manifest);
    }
    let approvals = manifest["approvals"]
        .as_array()
        .ok_or(LegacyCutoverError::Manifest)?;
    let verifier = approvals.iter().any(|approval| {
        approval["role"] == "verifier"
            && approval["decision"] == "attested"
            && approval["evidence_sha256"] == request.expected_attestation_sha256
    });
    let operator = approvals.iter().any(|approval| {
        approval["role"] == "operator"
            && approval["decision"] == "approved"
            && approval["subject_id"].as_str().is_some_and(valid_sha256)
            && approval["evidence_sha256"]
                .as_str()
                .is_some_and(valid_sha256)
    });
    if !verifier || !operator {
        return Err(LegacyCutoverError::Manifest);
    }
    if state == Some("approved") {
        if lowercase_hex(&sha256(bytes)) != request.expected_approved_manifest_sha256
            || !manifest["cutover_at"].is_null()
            || !manifest["rollback_deadline"].is_null()
            || !manifest["rollback"]["post_cutover_first_sequence"].is_null()
            || manifest["rollback"]["status"] != "prepared"
        {
            return Err(LegacyCutoverError::Manifest);
        }
    }
    Ok(())
}

fn validate_report(
    report: &LegacyConversionReport,
    request: &LegacyCutoverRequest,
) -> Result<(), LegacyCutoverError> {
    if report.contract_version != "legacy-conversion-report-v1"
        || report.migration_id != request.migration_id
        || report.project_id != request.project_id
        || report.workspace_id != request.workspace_id
        || report.source_snapshot_sha256 != request.expected_source_snapshot_sha256
        || report.staging_snapshot_sha256 != request.expected_staging_snapshot_sha256
        || report.source_counts != report.imported_counts
        || report.comparison != "equivalent"
        || report.state != "candidate_ready_for_independent_attestation"
        || report.approval_created
        || report.cutover_performed
    {
        return Err(LegacyCutoverError::Candidate);
    }
    Ok(())
}

fn validate_candidate(
    database: &ProjectDatabase,
    request: &LegacyCutoverRequest,
    report: &LegacyConversionReport,
) -> Result<(), LegacyCutoverError> {
    let health = database.health().map_err(|_| LegacyCutoverError::Storage)?;
    if health.schema_version != STORAGE_SCHEMA_VERSION
        || health.quick_check != "ok"
        || health.foreign_key_violations != 0
        || health.trusted_schema
        || !health.foreign_keys
    {
        return Err(LegacyCutoverError::Storage);
    }
    validate_candidate_logical_state(database, request, report)?;
    database
        .with_immediate_transaction(|transaction| {
            let attestation: Vec<u8> = transaction.query_row(
                "SELECT payload FROM attestations WHERE attestation_id = ?1 AND project_id = ?2",
                AC_params![request.attestation_id, request.project_id],
                |row| row.get(0),
            )?;
            if lowercase_hex(&sha256(&attestation)) != request.expected_attestation_sha256 {
                return Err(integrity("cutover attestation digest differs"));
            }
            let attestation: LegacyVerifierAttestation = serde_json::from_slice(&attestation)
                .map_err(|_| integrity("cutover attestation payload is invalid"))?;
            if attestation.migration_id != request.migration_id
                || attestation.project_id != request.project_id
                || attestation.workspace_id != request.workspace_id
                || attestation.candidate_logical_state_sha256
                    != request.expected_candidate_logical_state_sha256
                || attestation.decision != "attested"
            {
                return Err(integrity("cutover attestation binding differs"));
            }
            let approval_payload = format!(
                "migration candidate approved by operator {}",
                request.approval_id
            );
            let approvals: i64 = transaction.query_row(
                "SELECT count(*) FROM events WHERE project_id = ?1 AND event_kind = 'migration_applied' AND actor_kind = 'operator' AND outcome = 'succeeded' AND fingerprint = ?2 AND resource_id = ?3 AND payload = ?4",
                AC_params![
                    request.project_id,
                    request.expected_approval_sha256,
                    request.migration_id,
                    approval_payload.as_bytes(),
                ],
                |row| row.get(0),
            )?;
            if approvals != 1 {
                return Err(integrity("cutover operator approval event differs"));
            }
            Ok(())
        })
        .map_err(|_| LegacyCutoverError::Candidate)
}

fn validate_candidate_logical_state(
    database: &ProjectDatabase,
    request: &LegacyCutoverRequest,
    report: &LegacyConversionReport,
) -> Result<(), LegacyCutoverError> {
    database
        .with_immediate_transaction(|transaction| {
            let digest = logical_state_sha256(
                transaction,
                &request.project_id,
                &request.workspace_id,
                &request.migration_id,
                &report.imported_counts,
            )?;
            if digest != request.expected_candidate_logical_state_sha256 {
                return Err(integrity("cutover candidate logical state differs"));
            }
            Ok(())
        })
        .map_err(|_| LegacyCutoverError::Storage)
}

fn candidate_identity(
    database: &ProjectDatabase,
    request: &LegacyCutoverRequest,
) -> Result<(StorageProject, StorageWorkspace), LegacyCutoverError> {
    let projects = database
        .registered_projects()
        .map_err(|_| LegacyCutoverError::Storage)?;
    let project = projects
        .into_iter()
        .find(|project| project.project_id == request.project_id)
        .ok_or(LegacyCutoverError::Candidate)?;
    database
        .with_immediate_transaction(|transaction| {
            let workspace = transaction.query_row(
                "SELECT workspace_id, project_id, workspace_key, created_at FROM workspaces WHERE workspace_id = ?1 AND project_id = ?2 AND status = 'active'",
                AC_params![request.workspace_id, request.project_id],
                |row| {
                    Ok(StorageWorkspace {
                        workspace_id: row.get(0)?,
                        project_id: row.get(1)?,
                        workspace_key: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )?;
            Ok((project.clone(), workspace))
        })
        .map_err(|_| LegacyCutoverError::Candidate)
}

fn create_consistent_candidate_copy(
    candidate: &ProjectDatabase,
    partial_root: &Path,
) -> Result<(), LegacyCutoverError> {
    fs::create_dir(partial_root).map_err(|_| LegacyCutoverError::Storage)?;
    #[cfg(unix)]
    fs::set_permissions(partial_root, fs::Permissions::from_mode(0o700))
        .map_err(|_| LegacyCutoverError::Storage)?;
    let state = partial_root.join("state");
    fs::create_dir(&state).map_err(|_| LegacyCutoverError::Storage)?;
    #[cfg(unix)]
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700))
        .map_err(|_| LegacyCutoverError::Storage)?;
    let destination_path = state.join("state.db");
    let source = AC_Connection::open_with_flags(
        candidate.database_path(),
        AC_OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|_| LegacyCutoverError::Storage)?;
    source
        .busy_timeout(Duration::from_secs(5))
        .map_err(|_| LegacyCutoverError::Storage)?;
    let mut destination = AC_Connection::open_with_flags(
        &destination_path,
        AC_OpenFlags::SQLITE_OPEN_READ_WRITE | AC_OpenFlags::SQLITE_OPEN_CREATE,
    )
    .map_err(|_| LegacyCutoverError::Storage)?;
    {
        let backup =
            AC_Backup::new(&source, &mut destination).map_err(|_| LegacyCutoverError::Storage)?;
        backup
            .run_to_completion(128, Duration::from_millis(5), None)
            .map_err(|_| LegacyCutoverError::Storage)?;
    }
    destination
        .execute_batch("PRAGMA synchronous = FULL;")
        .map_err(|_| LegacyCutoverError::Storage)?;
    drop(destination);
    drop(source);
    #[cfg(unix)]
    fs::set_permissions(&destination_path, fs::Permissions::from_mode(0o600))
        .map_err(|_| LegacyCutoverError::Storage)?;
    fs::File::open(&destination_path)
        .and_then(|file| file.sync_all())
        .map_err(|_| LegacyCutoverError::Storage)?;
    sync_directory(&state).map_err(|_| LegacyCutoverError::Storage)?;
    sync_directory(partial_root).map_err(|_| LegacyCutoverError::Storage)?;
    Ok(())
}

fn require_exact_project(
    database: &ProjectDatabase,
    expected: &StorageProject,
) -> Result<(), LegacyCutoverError> {
    let projects = database
        .registered_projects()
        .map_err(|_| LegacyCutoverError::Storage)?;
    if projects.len() != 1 || projects.first() != Some(expected) {
        return Err(LegacyCutoverError::Storage);
    }
    Ok(())
}

fn load_cutover_binding(
    database: &ProjectDatabase,
    request: &LegacyCutoverRequest,
) -> Result<Option<MigrationCutoverBinding>, LegacyCutoverError> {
    database
        .with_immediate_transaction(|transaction| {
            let count: i64 = transaction.query_row(
                "SELECT count(*) FROM events WHERE project_id = ?1 AND workspace_id = ?2 AND actor_kind = 'service' AND event_kind = 'migration_applied' AND outcome = 'succeeded' AND resource_id = ?3",
                AC_params![request.project_id, request.workspace_id, request.migration_id],
                |row| row.get(0),
            )?;
            if count > 1 {
                return Err(integrity("multiple cutover bindings exist for one migration"));
            }
            let payload = transaction
                .query_row(
                    "SELECT payload FROM events WHERE project_id = ?1 AND workspace_id = ?2 AND actor_kind = 'service' AND event_kind = 'migration_applied' AND outcome = 'succeeded' AND resource_id = ?3",
                    AC_params![request.project_id, request.workspace_id, request.migration_id],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?;
            payload
                .map(|payload| {
                    serde_json::from_slice(&payload)
                        .map_err(|_| integrity("stored cutover binding is invalid"))
                })
                .transpose()
        })
        .map_err(|_| LegacyCutoverError::Storage)
}

// Validate the exact durable cutover authority; an approved staging candidate
// without this service event is not a destination for identity association.
pub(crate) fn validate_association_cutover(
    transaction: &ac_ant_delirium::libres::Rusqlite_unit::AC_Transaction<'_>,
    request: &crate::task_package_contracts::MigrationIdentityAssociation,
    service_subject_sha256: &str,
    check_candidate_state: bool,
) -> Result<(), TransactionalStorageError> {
    let mut statement = transaction.prepare(
        "SELECT payload, fingerprint, actor_subject_id, event_id, sequence FROM events WHERE project_id = ?1 AND workspace_id = ?2 AND actor_kind = 'service' AND event_kind = 'migration_applied' AND outcome = 'succeeded' AND resource_id = ?3",
    )?;
    let records = statement
        .query_map(
            AC_params![
                request.candidate_project_id,
                request.candidate_workspace_id,
                request.migration_id
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    if records.len() != 1 {
        return Err(integrity("cutover authority is absent or ambiguous"));
    }
    let record = &records[0];
    let binding: MigrationCutoverBinding =
        serde_json::from_slice(&record.0).map_err(|_| integrity("cutover authority is invalid"))?;
    if binding.contract_version != CUTOVER_BINDING_CONTRACT
        || binding.migration_id != request.migration_id
        || binding.cutover_id != request.cutover_id
        || binding.cutover_id
            != deterministic_id("cut_", &request.approval_id, &request.migration_id)
        || binding.approval_id != request.approval_id
        || binding.project_id != request.candidate_project_id
        || binding.workspace_id != request.candidate_workspace_id
        || binding.candidate_logical_state_sha256 != request.candidate_logical_state_sha256
        || binding.service_subject_sha256 != service_subject_sha256
        || record.2 != service_subject_sha256
        || record.3 != deterministic_id("evt_", &binding.cutover_id, "canonical-authority")
        || record.1 != request.cutover_binding_sha256
        || lowercase_hex(&sha256(&record.0)) != request.cutover_binding_sha256
    {
        return Err(integrity("cutover authority binding conflicts"));
    }
    if check_candidate_state {
        if record.4 != binding.post_cutover_first_sequence {
            return Err(integrity("candidate cutover sequence differs"));
        }
        let count = |sql: &str| -> Result<u64, TransactionalStorageError> {
            Ok(
                transaction.query_row(sql, AC_params![request.candidate_project_id], |row| {
                    row.get(0)
                })?,
            )
        };
        let legacy_count = |kind: &str| -> Result<u64, TransactionalStorageError> {
            Ok(transaction.query_row(
                "SELECT count(*) FROM legacy_migration_records WHERE project_id = ?1 AND migration_id = ?2 AND record_kind = ?3",
                AC_params![request.candidate_project_id, request.migration_id, kind], |row| row.get(0),
            )?)
        };
        let counts = crate::legacy_conversion::LegacyConversionCounts {
            plans: count("SELECT count(*) FROM plans WHERE project_id = ?1")?,
            tasks: count("SELECT count(*) FROM tasks WHERE project_id = ?1")?,
            submissions: 0,
            reviews: 0,
            history_events: legacy_count("history")?,
            problems: legacy_count("problem")?,
            transitions: legacy_count("transition")?,
        };
        if logical_state_sha256(
            transaction,
            &request.candidate_project_id,
            &request.candidate_workspace_id,
            &request.migration_id,
            &counts,
        )? != request.candidate_logical_state_sha256
            || count("SELECT count(*) FROM submissions WHERE project_id = ?1")? != 0
            || count("SELECT count(*) FROM reviews WHERE project_id = ?1")? != 0
        {
            return Err(integrity("candidate state changed after cutover"));
        }
    }
    Ok(())
}

fn validate_binding(
    binding: &MigrationCutoverBinding,
    request: &LegacyCutoverRequest,
    service_subject_sha256: &str,
    request_sha256: &str,
) -> Result<(), LegacyCutoverError> {
    let deadline = binding
        .cutover_at_unix
        .checked_add(request.rollback_window_seconds)
        .ok_or(LegacyCutoverError::Request)?;
    if binding.contract_version != CUTOVER_BINDING_CONTRACT
        || binding.cutover_id
            != deterministic_id("cut_", &request.approval_id, &request.migration_id)
        || binding.migration_id != request.migration_id
        || binding.project_id != request.project_id
        || binding.workspace_id != request.workspace_id
        || binding.service_subject_sha256 != service_subject_sha256
        || binding.cutover_request_sha256 != request_sha256
        || binding.approved_manifest_sha256 != request.expected_approved_manifest_sha256
        || binding.approval_id != request.approval_id
        || binding.approval_sha256 != request.expected_approval_sha256
        || binding.attestation_id != request.attestation_id
        || binding.attestation_sha256 != request.expected_attestation_sha256
        || binding.candidate_logical_state_sha256 != request.expected_candidate_logical_state_sha256
        || binding.source_snapshot_sha256 != request.expected_source_snapshot_sha256
        || binding.staging_snapshot_sha256 != request.expected_staging_snapshot_sha256
        || binding.rollback_legacy_snapshot_sha256 != request.rollback_legacy_snapshot_sha256
        || binding.rollback_deadline_unix != deadline
        || binding.cutover_at
            != rfc3339_utc_from_unix(binding.cutover_at_unix)
                .map_err(|_| LegacyCutoverError::Storage)?
        || binding.rollback_deadline
            != rfc3339_utc_from_unix(binding.rollback_deadline_unix)
                .map_err(|_| LegacyCutoverError::Storage)?
        || binding.post_cutover_first_sequence < 1
    {
        return Err(LegacyCutoverError::Storage);
    }
    Ok(())
}

fn cutover_event(binding: &MigrationCutoverBinding) -> Result<NewAuditEvent, LegacyCutoverError> {
    let payload = serde_json::to_vec(binding).map_err(|_| LegacyCutoverError::Storage)?;
    let fingerprint = lowercase_hex(&sha256(&payload));
    Ok(NewAuditEvent {
        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
        event_id: deterministic_id("evt_", &binding.cutover_id, "canonical-authority"),
        project_id: binding.project_id.clone(),
        workspace_id: Some(binding.workspace_id.clone()),
        actor_kind: "service".to_string(),
        actor_subject_id: binding.service_subject_sha256.clone(),
        event_kind: "migration_applied".to_string(),
        outcome: "succeeded".to_string(),
        occurred_at: binding.cutover_at.clone(),
        fingerprint: fingerprint.clone(),
        correlation_id: deterministic_id("ref_", &fingerprint, "canonical-authority"),
        resource_id: Some(binding.migration_id.clone()),
        payload,
    })
}

fn require_exact_cutover_event(
    database: &ProjectDatabase,
    binding: &MigrationCutoverBinding,
    binding_sha256: &str,
    expected_sequence: Option<i64>,
) -> Result<(), LegacyCutoverError> {
    let event = cutover_event(binding)?;
    if event.fingerprint != binding_sha256 {
        return Err(LegacyCutoverError::Storage);
    }
    database
        .with_immediate_transaction(|transaction| {
            let count: i64 = transaction.query_row(
                "SELECT count(*) FROM events WHERE project_id = ?1 AND (?2 IS NULL OR sequence = ?2) AND event_id = ?3 AND workspace_id = ?4 AND actor_kind = 'service' AND actor_subject_id = ?5 AND event_kind = 'migration_applied' AND outcome = 'succeeded' AND occurred_at = ?6 AND fingerprint = ?7 AND correlation_id = ?8 AND resource_id = ?9 AND payload = ?10",
                AC_params![
                    event.project_id,
                    expected_sequence,
                    event.event_id,
                    binding.workspace_id,
                    event.actor_subject_id,
                    event.occurred_at,
                    event.fingerprint,
                    event.correlation_id,
                    binding.migration_id,
                    event.payload,
                ],
                |row| row.get(0),
            )?;
            if count != 1 {
                return Err(integrity("canonical cutover event differs"));
            }
            let scoped_count: i64 = transaction.query_row(
                "SELECT count(*) FROM events WHERE project_id = ?1 AND workspace_id = ?2 AND actor_kind = 'service' AND event_kind = 'migration_applied' AND outcome = 'succeeded' AND resource_id = ?3",
                AC_params![binding.project_id, binding.workspace_id, binding.migration_id],
                |row| row.get(0),
            )?;
            if scoped_count != 1 {
                return Err(integrity("canonical cutover binding is ambiguous"));
            }
            Ok(())
        })
        .map_err(|_| LegacyCutoverError::Storage)
}

fn register_canonical_authority(
    registry: &ProjectDatabase,
    project: &StorageProject,
    workspace: &StorageWorkspace,
    binding: &MigrationCutoverBinding,
    binding_sha256: &str,
) -> Result<(), LegacyCutoverError> {
    let event = cutover_event(binding)?;
    if event.fingerprint != binding_sha256 {
        return Err(LegacyCutoverError::Storage);
    }
    registry
        .with_immediate_transaction(|transaction| {
            let matching_projects: i64 = transaction.query_row(
                "SELECT count(*) FROM projects WHERE project_id = ?1 OR project_key = ?2",
                AC_params![project.project_id, project.project_key],
                |row| row.get(0),
            )?;
            if matching_projects > 1 {
                return Err(integrity("canonical project authority is ambiguous"));
            }
            let existing_project = transaction
                .query_row(
                    "SELECT project_key, created_at FROM projects WHERE project_id = ?1 OR project_key = ?2",
                    AC_params![project.project_id, project.project_key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let project_was_registered = existing_project.is_some();
            match existing_project {
                Some((key, created_at))
                    if key == project.project_key && created_at == project.created_at => {}
                Some(_) => return Err(integrity("canonical project authority conflicts")),
                None => {
                    transaction.execute(
                        "INSERT INTO projects(project_id, project_key, created_at) VALUES (?1, ?2, ?3)",
                        AC_params![project.project_id, project.project_key, project.created_at],
                    )?;
                }
            }
            let existing_workspace = transaction
                .query_row(
                    "SELECT project_id, workspace_key, created_at, status FROM workspaces WHERE workspace_id = ?1",
                    AC_params![workspace.workspace_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?;
            match existing_workspace {
                Some((project_id, key, created_at, status))
                    if project_id == workspace.project_id
                        && key == workspace.workspace_key
                        && created_at == workspace.created_at
                        && status == "active" => {}
                Some(_) => return Err(integrity("canonical workspace authority conflicts")),
                None => {
                    transaction.execute(
                        "INSERT INTO workspaces(workspace_id, project_id, workspace_key, created_at) VALUES (?1, ?2, ?3, ?4)",
                        AC_params![
                            workspace.workspace_id,
                            workspace.project_id,
                            workspace.workspace_key,
                            workspace.created_at,
                        ],
                    )?;
                }
            }
            let existing_binding: Option<Vec<u8>> = transaction
                .query_row(
                    "SELECT payload FROM events WHERE project_id = ?1 AND workspace_id = ?2 AND actor_kind = 'service' AND event_kind = 'migration_applied' AND outcome = 'succeeded' AND resource_id = ?3",
                    AC_params![binding.project_id, binding.workspace_id, binding.migration_id],
                    |row| row.get(0),
                )
                .optional()?;
            if project_was_registered && existing_binding.as_deref() != Some(event.payload.as_slice()) {
                return Err(integrity(
                    "registered project has no matching canonical cutover binding",
                ));
            }
            registry.record_event_in_transaction(transaction, &event)?;
            Ok(())
        })
        .map_err(|_| LegacyCutoverError::Storage)?;
    require_exact_cutover_event(registry, binding, binding_sha256, None)
}

fn cutover_manifest_value(
    current: &Value,
    request: &LegacyCutoverRequest,
    binding: &MigrationCutoverBinding,
) -> Result<Value, LegacyCutoverError> {
    let mut base = current.clone();
    if current["state"] == "cutover" {
        base["state"] = Value::String("approved".to_string());
        base["cutover_at"] = Value::Null;
        base["rollback_deadline"] = Value::Null;
        base["rollback"]["post_cutover_first_sequence"] = Value::Null;
        base["rollback"]["status"] = Value::String("prepared".to_string());
    }
    if base["state"] != "approved"
        || !base["cutover_at"].is_null()
        || !base["rollback_deadline"].is_null()
        || base["rollback"]["legacy_snapshot_sha256"] != request.rollback_legacy_snapshot_sha256
        || !base["rollback"]["post_cutover_first_sequence"].is_null()
        || base["rollback"]["status"] != "prepared"
    {
        return Err(LegacyCutoverError::Manifest);
    }
    base["state"] = Value::String("cutover".to_string());
    base["cutover_at"] = Value::String(binding.cutover_at.clone());
    base["rollback_deadline"] = Value::String(binding.rollback_deadline.clone());
    base["rollback"]["post_cutover_first_sequence"] =
        Value::Number(binding.post_cutover_first_sequence.into());
    base["rollback"]["status"] = Value::String("available".to_string());
    if current["state"] == "cutover" && base != *current {
        return Err(LegacyCutoverError::Manifest);
    }
    Ok(base)
}

fn publish_cutover_manifest(
    migration_root: &Path,
    manifest_path: &Path,
    bytes: &[u8],
) -> Result<(), LegacyCutoverError> {
    let partial = migration_root.join("migration-manifest-v1.cutover.partial");
    match fs::symlink_metadata(&partial) {
        Ok(_) => {
            if read_control_file(&partial).map_err(|_| LegacyCutoverError::Manifest)? != bytes {
                return Err(LegacyCutoverError::Manifest);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_private_bytes(&partial, bytes).map_err(|_| LegacyCutoverError::Manifest)?;
        }
        Err(_) => return Err(LegacyCutoverError::Manifest),
    }
    fs::rename(&partial, manifest_path).map_err(|_| LegacyCutoverError::Manifest)?;
    sync_directory(migration_root).map_err(|_| LegacyCutoverError::Manifest)?;
    if read_control_file(manifest_path).map_err(|_| LegacyCutoverError::Manifest)? != bytes {
        return Err(LegacyCutoverError::Manifest);
    }
    Ok(())
}

fn receipt_sha256(receipt: &LegacyCutoverReceipt) -> Result<String, LegacyCutoverError> {
    let mut unsigned = receipt.clone();
    unsigned.cutover_sha256.clear();
    let bytes = serde_json::to_vec(&unsigned).map_err(|_| LegacyCutoverError::Storage)?;
    Ok(lowercase_hex(&sha256(&bytes)))
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
#[path = "tests/Legacy_Cutover_test.rs"]
pub(crate) mod legacy_cutover_tests;
