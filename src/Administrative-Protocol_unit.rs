use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::{fill_system_random, random_crockford_id};
use ac_ant_delirium::structs::{
    CanonicalTaskAdvance, CanonicalTaskPackage, CanonicalTaskPackageTask, CapabilityActor,
    CapabilityAuthorizationOutcome, CapabilityAuthorizationRequest, CapabilityChannel,
    CapabilityOperation,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::legacy_approval::{LegacyApprovalReceipt, LegacyApprovalRequest};
use crate::legacy_attestation::{LegacyAttestationReceipt, LegacyAttestationRequest};
use crate::legacy_conversion::{LegacyConversionReport, LegacyConversionRequest};
use crate::legacy_cutover::{LegacyCutoverReceipt, LegacyCutoverRequest};
use crate::legacy_dry_run::{LegacyDryRunReport, LegacyDryRunRequest};
use crate::legacy_normalization::{LegacyNormalizationReceipt, LegacyNormalizationRequest};
use crate::legacy_staging::LegacyStageReceipt;
use crate::limited_protocol::{
    decode_base64url, encode_base64url, is_encoded_nonce, random_request_id, valid_opaque_id,
};
use crate::public_output::{OutputRouter, PublicError};
use crate::service_authentication::ConnectionAuthentication;
use crate::service_authority::VerifiedServiceAuthority;
use crate::service_hardening::set_descriptor_close_on_exec;
use crate::service_storage::ManagedCapabilityGrantRequest;
use crate::service_storage::ProtectedServiceStorage;

const ADMIN_MAGIC: [u8; 4] = *b"ADA1";
const ADMIN_WIRE_VERSION: &str = "ada/1";
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_INVENTORY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TASK_PACKAGE_BYTES: u64 = MAX_FRAME_BYTES as u64 - (64 * 1024);
const ADMIN_TIMEOUT: Duration = Duration::from_secs(120);
const MIGRATION_OPERATION_TIMEOUT: Duration = Duration::from_secs(600);
const ADMIN_SOCKET_ENV: &str = "ANTDELIRIUM_ADMIN_SOCKET";
const ADMIN_CAPABILITY_FD_ENV: &str = "ANTDELIRIUM_ADMIN_CAPABILITY_FD";
const TASK_PACKAGE_FD_ENV: &str = "ANTDELIRIUM_TASK_PACKAGE_FD";
const TASK_ADVANCE_FD_ENV: &str = "ANTDELIRIUM_TASK_ADVANCE_FD";
const PROJECTION_CAPABILITY_FD_ENV: &str = "ANTDELIRIUM_PROJECTION_CAPABILITY_FD";
const CAPABILITY_OUTPUT_FD_ENV: &str = "ANTDELIRIUM_CAPABILITY_OUTPUT_FD";
const CAPABILITY_TARGET_FD_ENV: &str = "ANTDELIRIUM_CAPABILITY_TARGET_FD";
const CAPABILITY_SUBJECT_UID_ENV: &str = "ANTDELIRIUM_CAPABILITY_SUBJECT_UID";
const CAPABILITY_ACTOR_ENV: &str = "ANTDELIRIUM_CAPABILITY_ACTOR";
const CAPABILITY_CHANNEL_ENV: &str = "ANTDELIRIUM_CAPABILITY_CHANNEL";
const CAPABILITY_OPERATION_ENV: &str = "ANTDELIRIUM_CAPABILITY_OPERATION";
const CAPABILITY_PROJECT_ID_ENV: &str = "ANTDELIRIUM_CAPABILITY_PROJECT_ID";
const CAPABILITY_WORKSPACE_ID_ENV: &str = "ANTDELIRIUM_CAPABILITY_WORKSPACE_ID";
const CAPABILITY_TASK_ID_ENV: &str = "ANTDELIRIUM_CAPABILITY_TASK_ID";
const CAPABILITY_LIFETIME_SECONDS_ENV: &str = "ANTDELIRIUM_CAPABILITY_LIFETIME_SECONDS";
const CAPABILITY_REVOCATION_EPOCH_ENV: &str = "ANTDELIRIUM_CAPABILITY_REVOCATION_EPOCH";
const MIGRATION_SOURCE_FD_ENV: &str = "ANTDELIRIUM_MIGRATION_SOURCE_FD";
const MIGRATION_INVENTORY_FD_ENV: &str = "ANTDELIRIUM_MIGRATION_INVENTORY_FD";
const EXPECTED_SOURCE_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_SOURCE_SHA256";
const EXPECTED_STAGING_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_STAGING_SHA256";
const EXPECTED_NORMALIZED_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_NORMALIZED_SHA256";
const EXPECTED_SEMANTIC_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_SEMANTIC_SHA256";
const MIGRATION_ID_ENV: &str = "ANTDELIRIUM_MIGRATION_ID";
const EXPECTED_DRY_RUN_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_DRY_RUN_SHA256";
const EXPECTED_CONVERSION_REPORT_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_CONVERSION_REPORT_SHA256";
const MIGRATION_PROJECT_ID_ENV: &str = "ANTDELIRIUM_MIGRATION_PROJECT_ID";
const MIGRATION_WORKSPACE_ID_ENV: &str = "ANTDELIRIUM_MIGRATION_WORKSPACE_ID";
const EXPECTED_MANIFEST_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_MANIFEST_SHA256";
const MIGRATION_ATTESTATION_ID_ENV: &str = "ANTDELIRIUM_MIGRATION_ATTESTATION_ID";
const EXPECTED_ATTESTATION_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_ATTESTATION_SHA256";
const EXPECTED_CANDIDATE_LOGICAL_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_CANDIDATE_LOGICAL_SHA256";
const ROLLBACK_LEGACY_SHA256_ENV: &str = "ANTDELIRIUM_ROLLBACK_LEGACY_SHA256";
const MIGRATION_APPROVAL_ID_ENV: &str = "ANTDELIRIUM_MIGRATION_APPROVAL_ID";
const EXPECTED_APPROVAL_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_APPROVAL_SHA256";
const ROLLBACK_WINDOW_SECONDS_ENV: &str = "ANTDELIRIUM_ROLLBACK_WINDOW_SECONDS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdministrativeProtocolError {
    Endpoint,
    Transport(&'static str),
    Framing,
    Envelope,
    Authorization,
    Staging,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientHello {
    message_type: String,
    channel: String,
    wire_versions: Vec<String>,
    client_nonce: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerHello {
    message_type: String,
    wire_version: String,
    connection_nonce: String,
    max_frame_bytes: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCapability {
    capability_id: String,
    token: String,
}

impl Drop for WireCapability {
    fn drop(&mut self) {
        erase_string(&mut self.token);
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdministrativeCapabilityBundle {
    contract_version: String,
    capability: WireCapability,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationStageBody {
    expected_source_snapshot_sha256: String,
    inventory: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityGrantBody {
    contract_version: String,
    capability_id: String,
    token: String,
    subject_uid: u32,
    actor: String,
    channel: String,
    capability_operation: String,
    project_id: Option<String>,
    workspace_id: Option<String>,
    task_id: Option<String>,
    lifetime_seconds: i64,
    revocation_epoch: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityRevokeBody {
    contract_version: String,
    capability_id: String,
    revocation_epoch: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityGrantReceipt {
    contract_version: String,
    capability_id: String,
    audit_sequence: i64,
    event_id: String,
    correlation_id: String,
    state: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityRevokeReceipt {
    contract_version: String,
    capability_id: String,
    revocation_id: String,
    audit_sequence: i64,
    event_id: String,
    correlation_id: String,
    state: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskPackageTaskBody {
    task_id: String,
    task_revision: i64,
    objective: String,
    acceptance_criteria: Vec<String>,
    relevant_paths: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskPackageBody {
    contract_version: String,
    project_id: String,
    workspace_id: String,
    plan_id: String,
    expected_state_revision: i64,
    tasks: Vec<TaskPackageTaskBody>,
}

impl From<TaskPackageBody> for CanonicalTaskPackage {
    fn from(body: TaskPackageBody) -> Self {
        Self {
            contract_version: body.contract_version,
            project_id: body.project_id,
            workspace_id: body.workspace_id,
            plan_id: body.plan_id,
            expected_state_revision: body.expected_state_revision,
            tasks: body
                .tasks
                .into_iter()
                .map(|task| CanonicalTaskPackageTask {
                    task_id: task.task_id,
                    task_revision: task.task_revision,
                    objective: task.objective,
                    acceptance_criteria: task.acceptance_criteria,
                    relevant_paths: task.relevant_paths,
                })
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskPackageReceipt {
    contract_version: String,
    project_id: String,
    workspace_id: String,
    plan_id: String,
    current_task_id: String,
    state_revision: i64,
    task_count: usize,
    audit_sequence: i64,
    state: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskAdvanceBody {
    contract_version: String,
    project_id: String,
    workspace_id: String,
    task_id: String,
    review_id: String,
    expected_state_revision: i64,
}

impl From<TaskAdvanceBody> for CanonicalTaskAdvance {
    fn from(body: TaskAdvanceBody) -> Self {
        Self {
            project_id: body.project_id,
            workspace_id: body.workspace_id,
            task_id: body.task_id,
            review_id: body.review_id,
            expected_state_revision: body.expected_state_revision,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskAdvanceReceipt {
    contract_version: String,
    project_id: String,
    workspace_id: String,
    completed_task_id: String,
    next_task_id: Option<String>,
    state_revision: i64,
    audit_sequence: i64,
    state: String,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum AdministrativeBody {
    Grant(CapabilityGrantBody),
    Revoke(CapabilityRevokeBody),
    TaskPackage(TaskPackageBody),
    TaskAdvance(TaskAdvanceBody),
    Stage(MigrationStageBody),
    Normalize(LegacyNormalizationRequest),
    DryRun(LegacyDryRunRequest),
    Convert(LegacyConversionRequest),
    Attest(LegacyAttestationRequest),
    Approve(LegacyApprovalRequest),
    Cutover(LegacyCutoverRequest),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdministrativeRequest {
    message_type: String,
    wire_version: String,
    request_id: String,
    connection_nonce: String,
    operation: String,
    capability: WireCapability,
    body: AdministrativeBody,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum AdministrativeResult {
    Grant(CapabilityGrantReceipt),
    Revoke(CapabilityRevokeReceipt),
    TaskPackage(TaskPackageReceipt),
    TaskAdvance(TaskAdvanceReceipt),
    Stage(LegacyStageReceipt),
    Normalize(LegacyNormalizationReceipt),
    DryRun(LegacyDryRunReport),
    Convert(LegacyConversionReport),
    Attest(LegacyAttestationReceipt),
    Approve(LegacyApprovalReceipt),
    Cutover(LegacyCutoverReceipt),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdministrativeResponse {
    message_type: String,
    wire_version: String,
    request_id: String,
    status: String,
    result: Option<AdministrativeResult>,
    error: Option<Value>,
}

pub(crate) fn run_migration_operator(args: &[OsString]) {
    let normalize = args.len() == 2
        && args[0] == OsStr::new("migrate-plugin")
        && args[1] == OsStr::new("normalize");
    let dry_run = args.len() == 2
        && args[0] == OsStr::new("migrate-plugin")
        && args[1] == OsStr::new("dry-run");
    let convert = args.len() == 2
        && args[0] == OsStr::new("migrate-plugin")
        && args[1] == OsStr::new("convert");
    let attest = args.len() == 2
        && args[0] == OsStr::new("migrate-plugin")
        && args[1] == OsStr::new("attest");
    let approve = args.len() == 2
        && args[0] == OsStr::new("migrate-plugin")
        && args[1] == OsStr::new("approve");
    let cutover = args.len() == 2
        && args[0] == OsStr::new("migrate-plugin")
        && args[1] == OsStr::new("cutover");
    if !(args.len() == 1 && args[0] == OsStr::new("migrate-plugin"))
        && !normalize
        && !dry_run
        && !convert
        && !attest
        && !approve
        && !cutover
    {
        OutputRouter::exit(PublicError::Authorization);
    }
    if cfg!(debug_assertions) {
        OutputRouter::exit(PublicError::Configuration);
    }
    if crate::service_hardening::harden_operator_process().is_err() {
        OutputRouter::exit(PublicError::Authorization);
    }
    let result = if cutover {
        run_operator_cutover_inner().map(AdministrativeResult::Cutover)
    } else if approve {
        run_operator_approve_inner().map(AdministrativeResult::Approve)
    } else if attest {
        run_verifier_attest_inner().map(AdministrativeResult::Attest)
    } else if convert {
        run_operator_convert_inner().map(AdministrativeResult::Convert)
    } else if dry_run {
        run_operator_dry_run_inner().map(AdministrativeResult::DryRun)
    } else if normalize {
        run_operator_normalize_inner().map(AdministrativeResult::Normalize)
    } else {
        run_operator_stage_inner().map(AdministrativeResult::Stage)
    };
    match result {
        Ok(AdministrativeResult::Grant(receipt)) => OutputRouter::write_success(&format!(
            "antdelirium[AD-CAPABILITY-GRANTED]: capability_id={} audit_sequence={} state=active",
            receipt.capability_id, receipt.audit_sequence,
        )),
        Ok(AdministrativeResult::Revoke(receipt)) => OutputRouter::write_success(&format!(
            "antdelirium[AD-CAPABILITY-REVOKED]: capability_id={} revocation_id={} audit_sequence={} state=revoked",
            receipt.capability_id, receipt.revocation_id, receipt.audit_sequence,
        )),
        Ok(AdministrativeResult::Stage(receipt)) => OutputRouter::write_success(&format!(
                "antdelirium[AD-MIGRATION-STAGED]: migration_id={} project_id={} source_sha256={} staging_sha256={} state=staged",
                receipt.migration_id,
                receipt.project_id,
                receipt.source_snapshot_sha256,
                receipt.staging_snapshot_sha256,
            )),
        Ok(AdministrativeResult::Normalize(receipt)) => OutputRouter::write_success(&format!(
                "antdelirium[AD-MIGRATION-NORMALIZED]: migration_id={} project_id={} source_sha256={} staging_sha256={} normalized_sha256={} semantic_sha256={} comparison=equivalent state=normalized",
                receipt.migration_id,
                receipt.project_id,
                receipt.source_snapshot_sha256,
                receipt.staging_snapshot_sha256,
                receipt.normalized_snapshot_sha256,
                receipt.semantic_snapshot_sha256,
            )),
        Ok(AdministrativeResult::DryRun(report)) => {
            let report_sha256 = canonical_json_sha256(&report)
                .unwrap_or_else(|_| OutputRouter::exit(PublicError::Internal));
            OutputRouter::write_success(&format!(
                "antdelirium[AD-MIGRATION-DRY-RUN]: migration_id={} project_id={} report_sha256={} manifest_sha256={} differences={} comparison={} readiness={} approval_created=false cutover=false state=dry_run_completed",
                report.migration_id,
                report.project_id,
                report_sha256,
                report.manifest_snapshot_sha256,
                report.differences.len(),
                report.comparison,
                report.cutover_readiness,
            ))
        }
        Ok(AdministrativeResult::Convert(report)) => {
            let report_sha256 = canonical_json_sha256(&report)
                .unwrap_or_else(|_| OutputRouter::exit(PublicError::Internal));
            OutputRouter::write_success(&format!(
                "antdelirium[AD-MIGRATION-CONVERTED]: migration_id={} project_id={} workspace_id={} report_sha256={} manifest_sha256={} identity_bindings_sha256={} task_projection_sha256={} errors_projection_sha256={} comparison=equivalent approval_created=false cutover=false state=candidate_ready_for_independent_attestation",
                report.migration_id,
                report.project_id,
                report.workspace_id,
                report_sha256,
                report.manifest_snapshot_sha256,
                report.identity_bindings_sha256,
                report.current_task_projection_sha256,
                report.governance_errors_projection_sha256,
            ))
        }
        Ok(AdministrativeResult::Attest(receipt)) => OutputRouter::write_success(&format!(
                "antdelirium[AD-MIGRATION-ATTESTED]: migration_id={} project_id={} workspace_id={} attestation_id={} evidence_sha256={} candidate_logical_sha256={} approval_created=false cutover=false state=verifier_attested",
                receipt.attestation.migration_id,
                receipt.attestation.project_id,
                receipt.attestation.workspace_id,
                receipt.attestation.attestation_id,
                receipt.attestation_sha256,
                receipt.attestation.candidate_logical_state_sha256,
            )),
        Ok(AdministrativeResult::Approve(receipt)) => OutputRouter::write_success(&format!(
                "antdelirium[AD-MIGRATION-APPROVED]: migration_id={} project_id={} approval_id={} approval_sha256={} manifest_sha256={} cutover=false state=approved_awaiting_cutover",
                receipt.migration_id,
                receipt.project_id,
                receipt.approval_id,
                receipt.approval_sha256,
                receipt.manifest_after_sha256,
            )),
        Ok(AdministrativeResult::Cutover(receipt)) => OutputRouter::write_success(&format!(
                "antdelirium[AD-MIGRATION-CUTOVER]: migration_id={} project_id={} cutover_id={} binding_sha256={} manifest_sha256={} first_sequence={} rollback_status=available state=cutover_awaiting_verification",
                receipt.migration_id,
                receipt.project_id,
                receipt.cutover_id,
                receipt.canonical_binding_sha256,
                receipt.manifest_after_sha256,
                receipt.post_cutover_first_sequence,
            )),
        Ok(AdministrativeResult::TaskPackage(_)) => {
            OutputRouter::exit(PublicError::Authorization)
        }
        Ok(AdministrativeResult::TaskAdvance(_)) => {
            OutputRouter::exit(PublicError::Authorization)
        }
        Err(AdministrativeProtocolError::Authorization) => {
            OutputRouter::exit(PublicError::Authorization)
        }
        Err(AdministrativeProtocolError::Endpoint | AdministrativeProtocolError::Transport(_)) => {
            OutputRouter::exit(PublicError::ServiceUnavailable)
        }
        Err(
            AdministrativeProtocolError::Framing | AdministrativeProtocolError::Envelope,
        ) => OutputRouter::exit(PublicError::Protocol),
        Err(AdministrativeProtocolError::Staging) => {
            OutputRouter::exit(PublicError::Configuration)
        }
    }
}

pub(crate) fn run_task_package_operator(args: &[OsString]) {
    let install = args.len() == 3
        && args[0] == OsStr::new("operator")
        && args[1] == OsStr::new("task-package")
        && args[2] == OsStr::new("install");
    let advance = args.len() == 3
        && args[0] == OsStr::new("operator")
        && args[1] == OsStr::new("task")
        && args[2] == OsStr::new("advance");
    let refresh = args.len() == 3
        && args[0] == OsStr::new("operator")
        && args[1] == OsStr::new("projection")
        && args[2] == OsStr::new("refresh");
    if !install && !advance && !refresh {
        OutputRouter::exit(PublicError::Authorization);
    }
    if cfg!(debug_assertions) {
        OutputRouter::exit(PublicError::Configuration);
    }
    if crate::service_hardening::harden_operator_process().is_err() {
        OutputRouter::exit(PublicError::Authorization);
    }
    if install {
        match run_task_package_operator_inner() {
            Ok(receipt) => OutputRouter::write_success(&format!(
            "antdelirium[AD-TASK-PACKAGE-INSTALLED]: project_id={} workspace_id={} plan_id={} current_task_id={} state_revision={} task_count={} audit_sequence={} state={}",
            receipt.project_id,
            receipt.workspace_id,
            receipt.plan_id,
            receipt.current_task_id,
            receipt.state_revision,
            receipt.task_count,
            receipt.audit_sequence,
            receipt.state,
            )),
            Err(error) => exit_operator_protocol_error(error),
        }
    } else if advance {
        match run_task_advance_operator_inner() {
            Ok(receipt) => OutputRouter::write_success(&format!(
                "antdelirium[AD-TASK-ADVANCED]: project_id={} workspace_id={} completed_task_id={} next_task_id={} state_revision={} audit_sequence={} state={}",
                receipt.project_id,
                receipt.workspace_id,
                receipt.completed_task_id,
                receipt.next_task_id.as_deref().unwrap_or("none"),
                receipt.state_revision,
                receipt.audit_sequence,
                receipt.state,
            )),
            Err(error) => exit_operator_protocol_error(error),
        }
    } else {
        match run_projection_refresh_operator_inner() {
            Ok(()) => {
                OutputRouter::write_success("antdelirium[AD-PROJECTION-REFRESHED]: state=refreshed")
            }
            Err(error) => exit_operator_protocol_error(error),
        }
    }
}

fn exit_operator_protocol_error(error: AdministrativeProtocolError) -> ! {
    match error {
        AdministrativeProtocolError::Authorization => {
            OutputRouter::exit(PublicError::Authorization)
        }
        AdministrativeProtocolError::Endpoint | AdministrativeProtocolError::Transport(_) => {
            OutputRouter::exit(PublicError::ServiceUnavailable)
        }
        AdministrativeProtocolError::Framing | AdministrativeProtocolError::Envelope => {
            OutputRouter::exit(PublicError::Protocol)
        }
        AdministrativeProtocolError::Staging => OutputRouter::exit(PublicError::Configuration),
    }
}

#[cfg(unix)]
fn run_task_package_operator_inner() -> Result<TaskPackageReceipt, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let package_fd = std::env::var_os(TASK_PACKAGE_FD_ENV);
    clear_task_package_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let capability = read_administrative_capability()?;
    let mut package_bytes = read_bounded_json_from_raw_fd(
        package_fd.ok_or(AdministrativeProtocolError::Authorization)?,
    )?;
    let parsed = serde_json::from_slice(&package_bytes);
    package_bytes.fill(0);
    let package: TaskPackageBody = parsed.map_err(|_| AdministrativeProtocolError::Envelope)?;
    match exchange_administrative_request(
        &endpoint,
        capability.capability,
        "governance.task-package.install",
        AdministrativeBody::TaskPackage(package),
    )? {
        AdministrativeResult::TaskPackage(receipt) => Ok(receipt),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(unix)]
fn run_task_advance_operator_inner() -> Result<TaskAdvanceReceipt, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let advance_fd = std::env::var_os(TASK_ADVANCE_FD_ENV);
    clear_task_advance_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let capability = read_administrative_capability()?;
    let mut advance_bytes = read_bounded_json_from_raw_fd(
        advance_fd.ok_or(AdministrativeProtocolError::Authorization)?,
    )?;
    let parsed = serde_json::from_slice(&advance_bytes);
    advance_bytes.fill(0);
    let advance: TaskAdvanceBody = parsed.map_err(|_| AdministrativeProtocolError::Envelope)?;
    match exchange_administrative_request(
        &endpoint,
        capability.capability,
        "governance.task.advance.accepted",
        AdministrativeBody::TaskAdvance(advance),
    )? {
        AdministrativeResult::TaskAdvance(receipt) => Ok(receipt),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(unix)]
fn run_projection_refresh_operator_inner() -> Result<(), AdministrativeProtocolError> {
    let endpoint = std::env::var_os(crate::cargo_config::LIMITED_SOCKET_ENV);
    let capability_fd = std::env::var_os(PROJECTION_CAPABILITY_FD_ENV);
    std::env::remove_var(crate::cargo_config::LIMITED_SOCKET_ENV);
    std::env::remove_var(PROJECTION_CAPABILITY_FD_ENV);
    let endpoint = required_absolute_path(endpoint)?;
    let raw = capability_fd.ok_or(AdministrativeProtocolError::Authorization)?;
    let mut capability = read_capability_bundle_from_raw_fd(raw)?;
    if capability.contract_version != "wrapper-projection-capability-v1" {
        erase_string(&mut capability.capability.token);
        return Err(AdministrativeProtocolError::Authorization);
    }
    let project_root =
        std::env::current_dir().map_err(|_| AdministrativeProtocolError::Envelope)?;
    let projection = project_root.join("guard/to-do_list.json");
    let metadata = std::fs::symlink_metadata(&projection)
        .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > 1024 * 1024
    {
        return Err(AdministrativeProtocolError::Authorization);
    }
    let bytes = std::fs::read(&projection).map_err(|_| AdministrativeProtocolError::Envelope)?;
    let document: Value =
        serde_json::from_slice(&bytes).map_err(|_| AdministrativeProtocolError::Envelope)?;
    if document.get("contract_version").and_then(Value::as_str) != Some("agent-projection-v1")
        || document.get("artifact_kind").and_then(Value::as_str) != Some("current_task")
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    let project_id = document
        .get("project_ref")
        .and_then(Value::as_str)
        .filter(|value| valid_opaque_id(value, "prj_"))
        .ok_or(AdministrativeProtocolError::Envelope)?;
    let workspace_id = document
        .get("workspace_ref")
        .and_then(Value::as_str)
        .filter(|value| valid_opaque_id(value, "wsp_"))
        .ok_or(AdministrativeProtocolError::Envelope)?;
    let documents = crate::limited_protocol::refresh_scoped_projections(
        endpoint.as_os_str(),
        project_id,
        workspace_id,
        &capability.capability.capability_id,
        &capability.capability.token,
    )
    .map_err(|_| AdministrativeProtocolError::Authorization)?;
    ac_ant_delirium::structs::materialize_limited_project_projections(
        &project_root,
        &documents.current_task,
        &documents.governance_errors,
    )
    .map_err(|_| AdministrativeProtocolError::Envelope)
}

#[cfg(not(unix))]
fn run_projection_refresh_operator_inner() -> Result<(), AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(not(unix))]
fn run_task_advance_operator_inner() -> Result<TaskAdvanceReceipt, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(not(unix))]
fn run_task_package_operator_inner() -> Result<TaskPackageReceipt, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

pub(crate) fn run_capability_operator(args: &[OsString]) {
    let grant =
        args.len() == 2 && args[0] == OsStr::new("capability") && args[1] == OsStr::new("grant");
    let revoke =
        args.len() == 2 && args[0] == OsStr::new("capability") && args[1] == OsStr::new("revoke");
    if !grant && !revoke {
        OutputRouter::exit(PublicError::Authorization);
    }
    if cfg!(debug_assertions) {
        OutputRouter::exit(PublicError::Configuration);
    }
    if crate::service_hardening::harden_operator_process().is_err() {
        OutputRouter::exit(PublicError::Authorization);
    }
    let result = if grant {
        run_capability_grant_inner().map(AdministrativeResult::Grant)
    } else {
        run_capability_revoke_inner().map(AdministrativeResult::Revoke)
    };
    match result {
        Ok(AdministrativeResult::Grant(receipt)) => OutputRouter::write_success(&format!(
            "antdelirium[AD-CAPABILITY-GRANTED]: capability_id={} audit_sequence={} state=active",
            receipt.capability_id, receipt.audit_sequence,
        )),
        Ok(AdministrativeResult::Revoke(receipt)) => OutputRouter::write_success(&format!(
            "antdelirium[AD-CAPABILITY-REVOKED]: capability_id={} revocation_id={} audit_sequence={} state=revoked",
            receipt.capability_id, receipt.revocation_id, receipt.audit_sequence,
        )),
        Ok(_) => OutputRouter::exit(PublicError::Protocol),
        Err(AdministrativeProtocolError::Authorization) => {
            OutputRouter::exit(PublicError::Authorization)
        }
        Err(AdministrativeProtocolError::Endpoint | AdministrativeProtocolError::Transport(_)) => {
            OutputRouter::exit(PublicError::ServiceUnavailable)
        }
        Err(AdministrativeProtocolError::Framing | AdministrativeProtocolError::Envelope) => {
            OutputRouter::exit(PublicError::Protocol)
        }
        Err(AdministrativeProtocolError::Staging) => {
            OutputRouter::exit(PublicError::Configuration)
        }
    }
}

#[cfg(unix)]
fn run_capability_grant_inner() -> Result<CapabilityGrantReceipt, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let output_fd = std::env::var_os(CAPABILITY_OUTPUT_FD_ENV);
    let subject_uid = std::env::var(CAPABILITY_SUBJECT_UID_ENV).ok();
    let actor = std::env::var(CAPABILITY_ACTOR_ENV).ok();
    let channel = std::env::var(CAPABILITY_CHANNEL_ENV).ok();
    let operation = std::env::var(CAPABILITY_OPERATION_ENV).ok();
    let project_id = std::env::var(CAPABILITY_PROJECT_ID_ENV).ok();
    let workspace_id = std::env::var(CAPABILITY_WORKSPACE_ID_ENV).ok();
    let task_id = std::env::var(CAPABILITY_TASK_ID_ENV).ok();
    let lifetime_seconds = std::env::var(CAPABILITY_LIFETIME_SECONDS_ENV).ok();
    let revocation_epoch = std::env::var(CAPABILITY_REVOCATION_EPOCH_ENV).ok();
    clear_capability_environment();

    let endpoint = required_absolute_path(endpoint)?;
    let subject_uid = parse_decimal::<u32>(subject_uid)?;
    let actor = actor.ok_or(AdministrativeProtocolError::Envelope)?;
    let channel = channel.ok_or(AdministrativeProtocolError::Envelope)?;
    let operation = operation.ok_or(AdministrativeProtocolError::Envelope)?;
    let issued_contract = issued_capability_contract(&actor, &channel, &operation);
    let lifetime_seconds = parse_decimal::<i64>(lifetime_seconds)?;
    let revocation_epoch = parse_decimal::<u64>(revocation_epoch)?;
    let capability = read_administrative_capability()?;

    let capability_id =
        random_crockford_id("cap_").map_err(|_| AdministrativeProtocolError::Envelope)?;
    let mut token = [0_u8; 32];
    fill_system_random(&mut token)
        .map_err(|_| AdministrativeProtocolError::Transport("capability-entropy"))?;
    if token.iter().all(|byte| *byte == 0) {
        return Err(AdministrativeProtocolError::Transport(
            "capability-entropy-zero",
        ));
    }
    let mut issued = AdministrativeCapabilityBundle {
        contract_version: issued_contract.to_string(),
        capability: WireCapability {
            capability_id: capability_id.clone(),
            token: encode_base64url(&token),
        },
    };
    let write_result = match output_fd {
        Some(output_fd) => write_capability_bundle_to_fd(output_fd, &issued),
        None => Err(AdministrativeProtocolError::Authorization),
    };
    token.fill(0);
    if let Err(error) = write_result {
        erase_string(&mut issued.capability.token);
        return Err(error);
    }

    let result = exchange_administrative_request(
        &endpoint,
        capability.capability,
        "capability.grant",
        AdministrativeBody::Grant(CapabilityGrantBody {
            contract_version: "capability-grant-request-v1".to_string(),
            capability_id: capability_id.clone(),
            token: issued.capability.token.clone(),
            subject_uid,
            actor,
            channel,
            capability_operation: operation,
            project_id,
            workspace_id,
            task_id,
            lifetime_seconds,
            revocation_epoch,
        }),
    );
    erase_string(&mut issued.capability.token);
    match result? {
        AdministrativeResult::Grant(receipt) if receipt.capability_id == capability_id => {
            Ok(receipt)
        }
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(not(unix))]
fn run_capability_grant_inner() -> Result<CapabilityGrantReceipt, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(unix)]
fn run_capability_revoke_inner() -> Result<CapabilityRevokeReceipt, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let target_fd = std::env::var_os(CAPABILITY_TARGET_FD_ENV);
    let revocation_epoch = std::env::var(CAPABILITY_REVOCATION_EPOCH_ENV).ok();
    clear_capability_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let revocation_epoch = parse_decimal::<u64>(revocation_epoch)?;
    let capability = read_administrative_capability()?;
    let mut target = read_capability_bundle_from_raw_fd(
        target_fd.ok_or(AdministrativeProtocolError::Authorization)?,
    )?;
    let capability_id = target.capability.capability_id.clone();
    erase_string(&mut target.capability.token);
    match exchange_administrative_request(
        &endpoint,
        capability.capability,
        "capability.revoke",
        AdministrativeBody::Revoke(CapabilityRevokeBody {
            contract_version: "capability-revoke-request-v1".to_string(),
            capability_id: capability_id.clone(),
            revocation_epoch,
        }),
    )? {
        AdministrativeResult::Revoke(receipt) if receipt.capability_id == capability_id => {
            Ok(receipt)
        }
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(not(unix))]
fn run_capability_revoke_inner() -> Result<CapabilityRevokeReceipt, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(unix)]
fn run_operator_stage_inner() -> Result<LegacyStageReceipt, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let source_fd = std::env::var_os(MIGRATION_SOURCE_FD_ENV);
    let inventory_fd = std::env::var_os(MIGRATION_INVENTORY_FD_ENV);
    let expected = std::env::var(EXPECTED_SOURCE_SHA256_ENV).ok();
    clear_operator_path_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let expected = expected.ok_or(AdministrativeProtocolError::Envelope)?;
    let capability = read_administrative_capability()?;
    let (source, inventory) = read_migration_source_inputs(
        source_fd.ok_or(AdministrativeProtocolError::Authorization)?,
        inventory_fd.ok_or(AdministrativeProtocolError::Authorization)?,
    )?;
    let inventory_value: Value =
        serde_json::from_slice(&inventory).map_err(|_| AdministrativeProtocolError::Envelope)?;

    let mut stream = UnixStream::connect(endpoint)
        .map_err(|_| AdministrativeProtocolError::Transport("connect"))?;
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Transport("close-on-exec"))?;
    let connection_nonce = negotiate_client(&mut stream)?;
    let request_id = random_request_id().map_err(|_| AdministrativeProtocolError::Envelope)?;
    let request = AdministrativeRequest {
        message_type: "request".to_string(),
        wire_version: ADMIN_WIRE_VERSION.to_string(),
        request_id: request_id.clone(),
        connection_nonce,
        operation: "migration.stage".to_string(),
        capability: capability.capability,
        body: AdministrativeBody::Stage(MigrationStageBody {
            expected_source_snapshot_sha256: expected.clone(),
            inventory: inventory_value,
        }),
    };
    write_frame(&mut stream, &request)?;
    {
        let mut writer = DeadlineWriter::new(&mut stream, MIGRATION_OPERATION_TIMEOUT);
        crate::legacy_staging::write_validated_source_stream_from_directory(
            &source,
            &inventory,
            &expected,
            &mut writer,
        )
        .map_err(|_| AdministrativeProtocolError::Staging)?;
        writer
            .flush()
            .map_err(|_| AdministrativeProtocolError::Transport("source-flush"))?;
    }
    let response: AdministrativeResponse = serde_json::from_slice(&read_frame_with_timeout(
        &mut stream,
        MIGRATION_OPERATION_TIMEOUT,
    )?)
    .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match response.result {
        Some(AdministrativeResult::Stage(receipt)) => Ok(receipt),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(not(unix))]
fn run_operator_stage_inner() -> Result<LegacyStageReceipt, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(unix)]
fn run_operator_normalize_inner() -> Result<LegacyNormalizationReceipt, AdministrativeProtocolError>
{
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let source_fd = std::env::var_os(MIGRATION_SOURCE_FD_ENV);
    let inventory_fd = std::env::var_os(MIGRATION_INVENTORY_FD_ENV);
    let expected = std::env::var(EXPECTED_SOURCE_SHA256_ENV).ok();
    let migration_id = std::env::var(MIGRATION_ID_ENV).ok();
    clear_operator_path_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let expected = expected.ok_or(AdministrativeProtocolError::Envelope)?;
    let migration_id = migration_id.ok_or(AdministrativeProtocolError::Envelope)?;
    let capability = read_administrative_capability()?;
    let (source, inventory) = read_migration_source_inputs(
        source_fd.ok_or(AdministrativeProtocolError::Authorization)?,
        inventory_fd.ok_or(AdministrativeProtocolError::Authorization)?,
    )?;
    let normalization = crate::legacy_normalization::prepare_source_normalization_from_directory(
        &source,
        &inventory,
        &expected,
        &migration_id,
    )
    .map_err(|_| AdministrativeProtocolError::Staging)?;

    let mut stream = UnixStream::connect(endpoint)
        .map_err(|_| AdministrativeProtocolError::Transport("connect"))?;
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Transport("close-on-exec"))?;
    let connection_nonce = negotiate_client(&mut stream)?;
    let request_id = random_request_id().map_err(|_| AdministrativeProtocolError::Envelope)?;
    write_frame(
        &mut stream,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request_id.clone(),
            connection_nonce,
            operation: "migration.normalize".to_string(),
            capability: capability.capability,
            body: AdministrativeBody::Normalize(normalization),
        },
    )?;
    let response: AdministrativeResponse = serde_json::from_slice(&read_frame_with_timeout(
        &mut stream,
        MIGRATION_OPERATION_TIMEOUT,
    )?)
    .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match response.result {
        Some(AdministrativeResult::Normalize(receipt)) => Ok(receipt),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(unix)]
fn run_operator_dry_run_inner() -> Result<LegacyDryRunReport, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let migration_id = std::env::var(MIGRATION_ID_ENV).ok();
    let source = std::env::var(EXPECTED_SOURCE_SHA256_ENV).ok();
    let staging = std::env::var(EXPECTED_STAGING_SHA256_ENV).ok();
    let normalized = std::env::var(EXPECTED_NORMALIZED_SHA256_ENV).ok();
    let semantic = std::env::var(EXPECTED_SEMANTIC_SHA256_ENV).ok();
    clear_operator_path_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let request = crate::legacy_dry_run::dry_run_request(
        migration_id.ok_or(AdministrativeProtocolError::Envelope)?,
        source.ok_or(AdministrativeProtocolError::Envelope)?,
        staging.ok_or(AdministrativeProtocolError::Envelope)?,
        normalized.ok_or(AdministrativeProtocolError::Envelope)?,
        semantic.ok_or(AdministrativeProtocolError::Envelope)?,
    );
    let capability = read_administrative_capability()?;
    let mut stream = UnixStream::connect(endpoint)
        .map_err(|_| AdministrativeProtocolError::Transport("connect"))?;
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Transport("close-on-exec"))?;
    let connection_nonce = negotiate_client(&mut stream)?;
    let request_id = random_request_id().map_err(|_| AdministrativeProtocolError::Envelope)?;
    write_frame(
        &mut stream,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request_id.clone(),
            connection_nonce,
            operation: "migration.dry-run".to_string(),
            capability: capability.capability,
            body: AdministrativeBody::DryRun(request),
        },
    )?;
    let response: AdministrativeResponse = serde_json::from_slice(&read_frame_with_timeout(
        &mut stream,
        MIGRATION_OPERATION_TIMEOUT,
    )?)
    .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match response.result {
        Some(AdministrativeResult::DryRun(report)) => Ok(report),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(unix)]
fn run_operator_convert_inner() -> Result<LegacyConversionReport, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let migration_id = std::env::var(MIGRATION_ID_ENV).ok();
    let source = std::env::var(EXPECTED_SOURCE_SHA256_ENV).ok();
    let staging = std::env::var(EXPECTED_STAGING_SHA256_ENV).ok();
    let normalized = std::env::var(EXPECTED_NORMALIZED_SHA256_ENV).ok();
    let semantic = std::env::var(EXPECTED_SEMANTIC_SHA256_ENV).ok();
    let dry_run = std::env::var(EXPECTED_DRY_RUN_SHA256_ENV).ok();
    clear_operator_path_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let request = crate::legacy_conversion::conversion_request(
        migration_id.ok_or(AdministrativeProtocolError::Envelope)?,
        source.ok_or(AdministrativeProtocolError::Envelope)?,
        staging.ok_or(AdministrativeProtocolError::Envelope)?,
        normalized.ok_or(AdministrativeProtocolError::Envelope)?,
        semantic.ok_or(AdministrativeProtocolError::Envelope)?,
        dry_run.ok_or(AdministrativeProtocolError::Envelope)?,
    );
    let capability = read_administrative_capability()?;
    let mut stream = UnixStream::connect(endpoint)
        .map_err(|_| AdministrativeProtocolError::Transport("connect"))?;
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Transport("close-on-exec"))?;
    let connection_nonce = negotiate_client(&mut stream)?;
    let request_id = random_request_id().map_err(|_| AdministrativeProtocolError::Envelope)?;
    write_frame(
        &mut stream,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request_id.clone(),
            connection_nonce,
            operation: "migration.convert".to_string(),
            capability: capability.capability,
            body: AdministrativeBody::Convert(request),
        },
    )?;
    let response: AdministrativeResponse = serde_json::from_slice(&read_frame_with_timeout(
        &mut stream,
        MIGRATION_OPERATION_TIMEOUT,
    )?)
    .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match response.result {
        Some(AdministrativeResult::Convert(report)) => Ok(report),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(unix)]
fn run_verifier_attest_inner() -> Result<LegacyAttestationReceipt, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let migration_id = std::env::var(MIGRATION_ID_ENV).ok();
    let project_id = std::env::var(MIGRATION_PROJECT_ID_ENV).ok();
    let workspace_id = std::env::var(MIGRATION_WORKSPACE_ID_ENV).ok();
    let report = std::env::var(EXPECTED_CONVERSION_REPORT_SHA256_ENV).ok();
    clear_operator_path_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let request = crate::legacy_attestation::attestation_request(
        migration_id.ok_or(AdministrativeProtocolError::Envelope)?,
        project_id.ok_or(AdministrativeProtocolError::Envelope)?,
        workspace_id.ok_or(AdministrativeProtocolError::Envelope)?,
        report.ok_or(AdministrativeProtocolError::Envelope)?,
    );
    let capability = read_administrative_capability()?;
    let mut stream = UnixStream::connect(endpoint)
        .map_err(|_| AdministrativeProtocolError::Transport("connect"))?;
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Transport("close-on-exec"))?;
    let connection_nonce = negotiate_client(&mut stream)?;
    let request_id = random_request_id().map_err(|_| AdministrativeProtocolError::Envelope)?;
    write_frame(
        &mut stream,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request_id.clone(),
            connection_nonce,
            operation: "migration.attest".to_string(),
            capability: capability.capability,
            body: AdministrativeBody::Attest(request),
        },
    )?;
    let response: AdministrativeResponse = serde_json::from_slice(&read_frame_with_timeout(
        &mut stream,
        MIGRATION_OPERATION_TIMEOUT,
    )?)
    .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match response.result {
        Some(AdministrativeResult::Attest(receipt)) => Ok(receipt),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(unix)]
fn run_operator_approve_inner() -> Result<LegacyApprovalReceipt, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let migration_id = std::env::var(MIGRATION_ID_ENV).ok();
    let project_id = std::env::var(MIGRATION_PROJECT_ID_ENV).ok();
    let workspace_id = std::env::var(MIGRATION_WORKSPACE_ID_ENV).ok();
    let manifest = std::env::var(EXPECTED_MANIFEST_SHA256_ENV).ok();
    let source = std::env::var(EXPECTED_SOURCE_SHA256_ENV).ok();
    let staging = std::env::var(EXPECTED_STAGING_SHA256_ENV).ok();
    let conversion = std::env::var(EXPECTED_CONVERSION_REPORT_SHA256_ENV).ok();
    let attestation_id = std::env::var(MIGRATION_ATTESTATION_ID_ENV).ok();
    let attestation = std::env::var(EXPECTED_ATTESTATION_SHA256_ENV).ok();
    let logical = std::env::var(EXPECTED_CANDIDATE_LOGICAL_SHA256_ENV).ok();
    let rollback = std::env::var(ROLLBACK_LEGACY_SHA256_ENV).ok();
    clear_operator_path_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let request = crate::legacy_approval::approval_request(
        migration_id.ok_or(AdministrativeProtocolError::Envelope)?,
        project_id.ok_or(AdministrativeProtocolError::Envelope)?,
        workspace_id.ok_or(AdministrativeProtocolError::Envelope)?,
        manifest.ok_or(AdministrativeProtocolError::Envelope)?,
        source.ok_or(AdministrativeProtocolError::Envelope)?,
        staging.ok_or(AdministrativeProtocolError::Envelope)?,
        conversion.ok_or(AdministrativeProtocolError::Envelope)?,
        attestation_id.ok_or(AdministrativeProtocolError::Envelope)?,
        attestation.ok_or(AdministrativeProtocolError::Envelope)?,
        logical.ok_or(AdministrativeProtocolError::Envelope)?,
        rollback.ok_or(AdministrativeProtocolError::Envelope)?,
    );
    let capability = read_administrative_capability()?;
    let mut stream = UnixStream::connect(endpoint)
        .map_err(|_| AdministrativeProtocolError::Transport("connect"))?;
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Transport("close-on-exec"))?;
    let connection_nonce = negotiate_client(&mut stream)?;
    let request_id = random_request_id().map_err(|_| AdministrativeProtocolError::Envelope)?;
    write_frame(
        &mut stream,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request_id.clone(),
            connection_nonce,
            operation: "migration.approve".to_string(),
            capability: capability.capability,
            body: AdministrativeBody::Approve(request),
        },
    )?;
    let response: AdministrativeResponse = serde_json::from_slice(&read_frame_with_timeout(
        &mut stream,
        MIGRATION_OPERATION_TIMEOUT,
    )?)
    .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match response.result {
        Some(AdministrativeResult::Approve(receipt)) => Ok(receipt),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(unix)]
fn run_operator_cutover_inner() -> Result<LegacyCutoverReceipt, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let migration_id = std::env::var(MIGRATION_ID_ENV).ok();
    let project_id = std::env::var(MIGRATION_PROJECT_ID_ENV).ok();
    let workspace_id = std::env::var(MIGRATION_WORKSPACE_ID_ENV).ok();
    let manifest = std::env::var(EXPECTED_MANIFEST_SHA256_ENV).ok();
    let source = std::env::var(EXPECTED_SOURCE_SHA256_ENV).ok();
    let staging = std::env::var(EXPECTED_STAGING_SHA256_ENV).ok();
    let conversion = std::env::var(EXPECTED_CONVERSION_REPORT_SHA256_ENV).ok();
    let attestation_id = std::env::var(MIGRATION_ATTESTATION_ID_ENV).ok();
    let attestation = std::env::var(EXPECTED_ATTESTATION_SHA256_ENV).ok();
    let approval_id = std::env::var(MIGRATION_APPROVAL_ID_ENV).ok();
    let approval = std::env::var(EXPECTED_APPROVAL_SHA256_ENV).ok();
    let logical = std::env::var(EXPECTED_CANDIDATE_LOGICAL_SHA256_ENV).ok();
    let rollback = std::env::var(ROLLBACK_LEGACY_SHA256_ENV).ok();
    let rollback_window = std::env::var(ROLLBACK_WINDOW_SECONDS_ENV)
        .ok()
        .and_then(|value| value.parse::<i64>().ok());
    clear_operator_path_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let request = crate::legacy_cutover::cutover_request(
        migration_id.ok_or(AdministrativeProtocolError::Envelope)?,
        project_id.ok_or(AdministrativeProtocolError::Envelope)?,
        workspace_id.ok_or(AdministrativeProtocolError::Envelope)?,
        manifest.ok_or(AdministrativeProtocolError::Envelope)?,
        source.ok_or(AdministrativeProtocolError::Envelope)?,
        staging.ok_or(AdministrativeProtocolError::Envelope)?,
        conversion.ok_or(AdministrativeProtocolError::Envelope)?,
        attestation_id.ok_or(AdministrativeProtocolError::Envelope)?,
        attestation.ok_or(AdministrativeProtocolError::Envelope)?,
        approval_id.ok_or(AdministrativeProtocolError::Envelope)?,
        approval.ok_or(AdministrativeProtocolError::Envelope)?,
        logical.ok_or(AdministrativeProtocolError::Envelope)?,
        rollback.ok_or(AdministrativeProtocolError::Envelope)?,
        rollback_window.ok_or(AdministrativeProtocolError::Envelope)?,
    );
    let capability = read_administrative_capability()?;
    let mut stream = UnixStream::connect(endpoint)
        .map_err(|_| AdministrativeProtocolError::Transport("connect"))?;
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Transport("close-on-exec"))?;
    let connection_nonce = negotiate_client(&mut stream)?;
    let request_id = random_request_id().map_err(|_| AdministrativeProtocolError::Envelope)?;
    write_frame(
        &mut stream,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request_id.clone(),
            connection_nonce,
            operation: "migration.cutover".to_string(),
            capability: capability.capability,
            body: AdministrativeBody::Cutover(request),
        },
    )?;
    let response: AdministrativeResponse = serde_json::from_slice(&read_frame_with_timeout(
        &mut stream,
        MIGRATION_OPERATION_TIMEOUT,
    )?)
    .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match response.result {
        Some(AdministrativeResult::Cutover(receipt)) => Ok(receipt),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(not(unix))]
fn run_operator_approve_inner() -> Result<LegacyApprovalReceipt, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(not(unix))]
fn run_operator_cutover_inner() -> Result<LegacyCutoverReceipt, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(not(unix))]
fn run_verifier_attest_inner() -> Result<LegacyAttestationReceipt, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(not(unix))]
fn run_operator_convert_inner() -> Result<LegacyConversionReport, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(not(unix))]
fn run_operator_dry_run_inner() -> Result<LegacyDryRunReport, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(not(unix))]
fn run_operator_normalize_inner() -> Result<LegacyNormalizationReceipt, AdministrativeProtocolError>
{
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(unix)]
pub(crate) fn answer_administrative_hello(
    stream: &mut UnixStream,
    connection_nonce: &[u8; 32],
) -> Result<(), AdministrativeProtocolError> {
    configure_nonblocking(stream)?;
    let hello: ClientHello = serde_json::from_slice(&read_frame(stream)?)
        .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if hello.message_type != "client_hello"
        || hello.channel != "admin"
        || hello.wire_versions != [ADMIN_WIRE_VERSION]
        || !is_encoded_nonce(&hello.client_nonce)
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    write_frame(
        stream,
        &ServerHello {
            message_type: "server_hello".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            connection_nonce: encode_base64url(connection_nonce),
            max_frame_bytes: MAX_FRAME_BYTES,
        },
    )
}

#[cfg(unix)]
pub(crate) fn serve_administrative_request(
    stream: &mut UnixStream,
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
) -> Result<(), AdministrativeProtocolError> {
    let mut payload = read_frame(stream)?;
    let parsed = serde_json::from_slice(&payload);
    payload.fill(0);
    let request: AdministrativeRequest =
        parsed.map_err(|_| AdministrativeProtocolError::Envelope)?;
    validate_request(&request, authentication.connection_nonce())?;
    let authorization = capability_request(authentication, &request)?;
    let outcome = authentication
        .authorize(
            storage.registry(),
            CapabilityChannel::Administrative,
            &authorization,
        )
        .map_err(|_| AdministrativeProtocolError::Authorization)?;
    let capability = match outcome {
        CapabilityAuthorizationOutcome::Fresh(capability)
            if request_authority_matches(&request, &capability) =>
        {
            capability
        }
        _ => return Err(AdministrativeProtocolError::Authorization),
    };
    let result = match request.body {
        AdministrativeBody::Grant(mut body) => {
            let actor = parse_managed_actor(&body.actor)?;
            let channel = parse_capability_channel(&body.channel)?;
            let operation = parse_managed_operation(&body.capability_operation)?;
            let decoded_token = decode_base64url(&body.token);
            erase_string(&mut body.token);
            let mut token = decoded_token.map_err(|_| AdministrativeProtocolError::Envelope)?;
            let grant_result = storage.grant_managed_capability(
                &capability,
                ManagedCapabilityGrantRequest {
                    grant_id: body.capability_id,
                    secret_hash: sha256(&token),
                    subject_uid: body.subject_uid,
                    actor,
                    channel,
                    operation,
                    project_id: body.project_id,
                    workspace_id: body.workspace_id,
                    task_id: body.task_id,
                    lifetime_seconds: body.lifetime_seconds,
                    revocation_epoch: body.revocation_epoch,
                },
            );
            token.fill(0);
            let receipt = grant_result.map_err(|_| AdministrativeProtocolError::Authorization)?;
            AdministrativeResult::Grant(CapabilityGrantReceipt {
                contract_version: "capability-grant-receipt-v1".to_string(),
                capability_id: receipt.grant_id,
                audit_sequence: receipt.audit_sequence,
                event_id: receipt.event_id,
                correlation_id: receipt.correlation_id,
                state: "active".to_string(),
            })
        }
        AdministrativeBody::Revoke(body) => {
            let receipt = storage
                .revoke_managed_capability(&capability, &body.capability_id, body.revocation_epoch)
                .map_err(|_| AdministrativeProtocolError::Authorization)?;
            AdministrativeResult::Revoke(CapabilityRevokeReceipt {
                contract_version: "capability-revoke-receipt-v1".to_string(),
                capability_id: receipt.grant_id,
                revocation_id: receipt.revocation_id,
                audit_sequence: receipt.audit_sequence,
                event_id: receipt.event_id,
                correlation_id: receipt.correlation_id,
                state: "revoked".to_string(),
            })
        }
        AdministrativeBody::TaskPackage(body) => {
            let package = CanonicalTaskPackage::from(body);
            let outcome = storage
                .install_generic_task_package(authority, &capability, &package)
                .map_err(|_| AdministrativeProtocolError::Authorization)?;
            AdministrativeResult::TaskPackage(TaskPackageReceipt {
                contract_version: "governance-task-package-receipt-v1".to_string(),
                project_id: package.project_id,
                workspace_id: package.workspace_id,
                plan_id: package.plan_id,
                current_task_id: outcome
                    .state
                    .current_task_id
                    .ok_or(AdministrativeProtocolError::Envelope)?,
                state_revision: outcome.state.state_revision,
                task_count: outcome.state.tasks.len(),
                audit_sequence: outcome.audit.sequence(),
                state: if outcome.audit.inserted() {
                    "installed".to_string()
                } else {
                    "already-installed".to_string()
                },
            })
        }
        AdministrativeBody::TaskAdvance(body) => {
            let advance = CanonicalTaskAdvance::from(body);
            let outcome = storage
                .advance_reviewed_task(authority, &capability, &advance)
                .map_err(|_| AdministrativeProtocolError::Authorization)?;
            AdministrativeResult::TaskAdvance(TaskAdvanceReceipt {
                contract_version: "governance-task-advance-receipt-v1".to_string(),
                project_id: advance.project_id,
                workspace_id: advance.workspace_id,
                completed_task_id: outcome.completed_task_id,
                next_task_id: outcome.next_task_id,
                state_revision: outcome.state.state_revision,
                audit_sequence: outcome.audit.sequence(),
                state: if outcome.audit.inserted() {
                    "advanced".to_string()
                } else {
                    "already-advanced".to_string()
                },
            })
        }
        AdministrativeBody::Stage(body) => {
            let inventory_document = serde_json::to_vec_pretty(&body.inventory)
                .map_err(|_| AdministrativeProtocolError::Envelope)?;
            let mut reader = DeadlineReader::new(stream, MIGRATION_OPERATION_TIMEOUT);
            let receipt = storage
                .stage_legacy_stream(
                    authority,
                    &capability,
                    &inventory_document,
                    &body.expected_source_snapshot_sha256,
                    &mut reader,
                )
                .map_err(|_| AdministrativeProtocolError::Staging)?;
            AdministrativeResult::Stage(receipt)
        }
        AdministrativeBody::Normalize(body) => {
            let receipt = storage
                .normalize_legacy_migration(authority, &capability, &body)
                .map_err(|_| AdministrativeProtocolError::Staging)?;
            AdministrativeResult::Normalize(receipt)
        }
        AdministrativeBody::DryRun(body) => {
            let report = storage
                .dry_run_legacy_migration(authority, &capability, &body)
                .map_err(|_| AdministrativeProtocolError::Staging)?;
            AdministrativeResult::DryRun(report)
        }
        AdministrativeBody::Convert(body) => {
            let report = storage
                .convert_legacy_migration(authority, &capability, &body)
                .map_err(|_| AdministrativeProtocolError::Staging)?;
            AdministrativeResult::Convert(report)
        }
        AdministrativeBody::Attest(body) => {
            let receipt = storage
                .attest_legacy_migration(authority, &capability, &body)
                .map_err(|_| AdministrativeProtocolError::Staging)?;
            AdministrativeResult::Attest(receipt)
        }
        AdministrativeBody::Approve(body) => {
            let receipt = storage
                .approve_legacy_migration(authority, &capability, &body)
                .map_err(|_| AdministrativeProtocolError::Staging)?;
            AdministrativeResult::Approve(receipt)
        }
        AdministrativeBody::Cutover(body) => {
            let receipt = storage
                .cutover_legacy_migration(authority, &capability, &body)
                .map_err(|_| AdministrativeProtocolError::Staging)?;
            AdministrativeResult::Cutover(receipt)
        }
    };
    write_frame(
        stream,
        &AdministrativeResponse {
            message_type: "response".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request.request_id,
            status: "ok".to_string(),
            result: Some(result),
            error: None,
        },
    )
}

fn capability_request(
    authentication: &ConnectionAuthentication,
    request: &AdministrativeRequest,
) -> Result<CapabilityAuthorizationRequest, AdministrativeProtocolError> {
    if !valid_opaque_id(&request.capability.capability_id, "cap_") {
        return Err(AdministrativeProtocolError::Authorization);
    }
    let mut fingerprint = request.operation.as_bytes().to_vec();
    fingerprint.push(0);
    fingerprint.extend_from_slice(
        &serde_json::to_vec(&request.body).map_err(|_| AdministrativeProtocolError::Envelope)?,
    );
    let (operation, project_id, workspace_id) = match &request.body {
        AdministrativeBody::Grant(_) | AdministrativeBody::Revoke(_) => {
            (CapabilityOperation::InstallationAdmin, None, None)
        }
        AdministrativeBody::Attest(body) => (
            CapabilityOperation::AttestationCreate,
            Some(body.project_id.clone()),
            Some(body.workspace_id.clone()),
        ),
        AdministrativeBody::TaskAdvance(body) => (
            CapabilityOperation::StateTransition,
            Some(body.project_id.clone()),
            Some(body.workspace_id.clone()),
        ),
        _ => (CapabilityOperation::StateAdmin, None, None),
    };
    Ok(CapabilityAuthorizationRequest {
        grant_id: request.capability.capability_id.clone(),
        presented_secret: decode_base64url(&request.capability.token)
            .map_err(|_| AdministrativeProtocolError::Authorization)?,
        operation,
        project_id,
        workspace_id,
        task_id: match &request.body {
            AdministrativeBody::TaskAdvance(body) => Some(body.task_id.clone()),
            _ => None,
        },
        presented_connection_nonce: *authentication.connection_nonce(),
        request_id: request.request_id.clone(),
        request_fingerprint: sha256(&fingerprint),
    })
}

fn request_authority_matches(
    request: &AdministrativeRequest,
    capability: &ac_ant_delirium::structs::AuthorizedCapability,
) -> bool {
    match &request.body {
        AdministrativeBody::Grant(_) | AdministrativeBody::Revoke(_) => {
            capability.actor() == CapabilityActor::Operator
                && capability.channel() == CapabilityChannel::Administrative
                && capability.operation() == CapabilityOperation::InstallationAdmin
                && capability.project_id().is_none()
                && capability.workspace_id().is_none()
                && capability.task_id().is_none()
        }
        AdministrativeBody::Attest(body) => {
            capability.actor() == CapabilityActor::Ci
                && capability.channel() == CapabilityChannel::Administrative
                && capability.operation() == CapabilityOperation::AttestationCreate
                && capability.project_id() == Some(body.project_id.as_str())
                && capability.workspace_id() == Some(body.workspace_id.as_str())
                && capability.task_id().is_none()
        }
        AdministrativeBody::TaskAdvance(body) => {
            capability.actor() == CapabilityActor::Ci
                && capability.channel() == CapabilityChannel::Administrative
                && capability.operation() == CapabilityOperation::StateTransition
                && capability.project_id() == Some(body.project_id.as_str())
                && capability.workspace_id() == Some(body.workspace_id.as_str())
                && capability.task_id() == Some(body.task_id.as_str())
        }
        _ => {
            capability.actor() == CapabilityActor::Operator
                && capability.channel() == CapabilityChannel::Administrative
                && capability.operation() == CapabilityOperation::StateAdmin
                && capability.project_id().is_none()
                && capability.workspace_id().is_none()
                && capability.task_id().is_none()
        }
    }
}

fn validate_request(
    request: &AdministrativeRequest,
    expected_nonce: &[u8; 32],
) -> Result<(), AdministrativeProtocolError> {
    if request.message_type != "request"
        || request.wire_version != ADMIN_WIRE_VERSION
        || !matches!(
            request.operation.as_str(),
            "capability.grant"
                | "capability.revoke"
                | "governance.task-package.install"
                | "governance.task.advance.accepted"
                | "migration.stage"
                | "migration.normalize"
                | "migration.dry-run"
                | "migration.convert"
                | "migration.attest"
                | "migration.approve"
                | "migration.cutover"
        )
        || !valid_opaque_id(&request.request_id, "req_")
        || decode_base64url(&request.connection_nonce)
            .map_err(|_| AdministrativeProtocolError::Envelope)?
            != *expected_nonce
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match (&request.operation[..], &request.body) {
        ("capability.grant", AdministrativeBody::Grant(body))
            if body.contract_version == "capability-grant-request-v1"
                && valid_opaque_id(&body.capability_id, "cap_")
                && is_encoded_nonce(&body.token) =>
        {
            Ok(())
        }
        ("capability.revoke", AdministrativeBody::Revoke(body))
            if body.contract_version == "capability-revoke-request-v1"
                && valid_opaque_id(&body.capability_id, "cap_") =>
        {
            Ok(())
        }
        ("governance.task-package.install", AdministrativeBody::TaskPackage(body))
            if body.contract_version == "governance-task-package-v1"
                && valid_opaque_id(&body.project_id, "prj_")
                && valid_opaque_id(&body.workspace_id, "wsp_")
                && valid_opaque_id(&body.plan_id, "pln_") =>
        {
            Ok(())
        }
        ("governance.task.advance.accepted", AdministrativeBody::TaskAdvance(body))
            if body.contract_version == "governance-task-advance-v1"
                && valid_opaque_id(&body.project_id, "prj_")
                && valid_opaque_id(&body.workspace_id, "wsp_")
                && valid_opaque_id(&body.task_id, "tsk_")
                && valid_opaque_id(&body.review_id, "rev_")
                && body.expected_state_revision >= 0 =>
        {
            Ok(())
        }
        ("migration.stage", AdministrativeBody::Stage(body))
            if body.inventory["inventory_sha256"].as_str()
                == Some(&body.expected_source_snapshot_sha256) =>
        {
            Ok(())
        }
        ("migration.normalize", AdministrativeBody::Normalize(body))
            if body.contract_version == "legacy-normalization-request-v1" =>
        {
            Ok(())
        }
        ("migration.dry-run", AdministrativeBody::DryRun(body))
            if body.contract_version == "legacy-dry-run-request-v1" =>
        {
            Ok(())
        }
        ("migration.convert", AdministrativeBody::Convert(body))
            if body.contract_version == "legacy-conversion-request-v1" =>
        {
            Ok(())
        }
        ("migration.attest", AdministrativeBody::Attest(body))
            if body.contract_version == "migration-attestation-request-v1" =>
        {
            Ok(())
        }
        ("migration.approve", AdministrativeBody::Approve(body))
            if body.contract_version == "migration-approval-request-v1" =>
        {
            Ok(())
        }
        ("migration.cutover", AdministrativeBody::Cutover(body))
            if body.contract_version == "migration-cutover-request-v1" =>
        {
            Ok(())
        }
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

fn parse_managed_actor(value: &str) -> Result<CapabilityActor, AdministrativeProtocolError> {
    match value {
        "wrapper" => Ok(CapabilityActor::Wrapper),
        "agent" => Ok(CapabilityActor::Agent),
        "reviewer" => Ok(CapabilityActor::Reviewer),
        "ci" => Ok(CapabilityActor::Ci),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

fn parse_capability_channel(value: &str) -> Result<CapabilityChannel, AdministrativeProtocolError> {
    match value {
        "limited" => Ok(CapabilityChannel::Limited),
        "administrative" => Ok(CapabilityChannel::Administrative),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

fn parse_managed_operation(
    value: &str,
) -> Result<CapabilityOperation, AdministrativeProtocolError> {
    match value {
        "compile.observe" => Ok(CapabilityOperation::CompileObserve),
        "projection.refresh" => Ok(CapabilityOperation::ProjectionRefresh),
        "task.current.read" => Ok(CapabilityOperation::TaskCurrentRead),
        "submission.create" => Ok(CapabilityOperation::SubmissionCreate),
        "submission.read.assigned" => Ok(CapabilityOperation::SubmissionReadAssigned),
        "review.decide.assigned" => Ok(CapabilityOperation::ReviewDecideAssigned),
        "attestation.create" => Ok(CapabilityOperation::AttestationCreate),
        "state.transition" => Ok(CapabilityOperation::StateTransition),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

fn issued_capability_contract(actor: &str, channel: &str, operation: &str) -> &'static str {
    match (actor, channel, operation) {
        ("agent", "limited", "submission.create") => "agent-submission-capability-v1",
        ("reviewer", "limited", "submission.read.assigned") => "reviewer-assignment-capability-v1",
        ("reviewer", "limited", "review.decide.assigned") => "reviewer-decision-capability-v1",
        ("wrapper", "limited", "projection.refresh") => "wrapper-projection-capability-v1",
        ("ci", "administrative", "state.transition") => "ci-state-transition-capability-v1",
        _ => "admin-capability-v1",
    }
}

fn erase_string(value: &mut String) {
    // SAFETY: zero bytes remain valid UTF-8. The token is never parsed again.
    unsafe { value.as_mut_vec().fill(0) };
}

#[cfg(unix)]
fn negotiate_client(stream: &mut UnixStream) -> Result<String, AdministrativeProtocolError> {
    configure_nonblocking(stream)?;
    let mut nonce = [0_u8; 32];
    fill_system_random(&mut nonce)
        .map_err(|_| AdministrativeProtocolError::Transport("client-entropy"))?;
    if nonce.iter().all(|byte| *byte == 0) {
        return Err(AdministrativeProtocolError::Transport(
            "client-entropy-zero",
        ));
    }
    write_frame(
        stream,
        &ClientHello {
            message_type: "client_hello".to_string(),
            channel: "admin".to_string(),
            wire_versions: vec![ADMIN_WIRE_VERSION.to_string()],
            client_nonce: encode_base64url(&nonce),
        },
    )?;
    let hello: ServerHello = serde_json::from_slice(&read_frame(stream)?)
        .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if hello.message_type != "server_hello"
        || hello.wire_version != ADMIN_WIRE_VERSION
        || hello.max_frame_bytes != MAX_FRAME_BYTES
        || !is_encoded_nonce(&hello.connection_nonce)
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    Ok(hello.connection_nonce)
}

#[cfg(unix)]
fn write_frame<T: Serialize>(
    stream: &mut UnixStream,
    value: &T,
) -> Result<(), AdministrativeProtocolError> {
    let mut payload =
        serde_json::to_vec(value).map_err(|_| AdministrativeProtocolError::Envelope)?;
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(AdministrativeProtocolError::Framing);
    }
    let length = u32::try_from(payload.len()).map_err(|_| AdministrativeProtocolError::Framing)?;
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&ADMIN_MAGIC);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    let result = write_all_before(stream, &frame, Instant::now() + ADMIN_TIMEOUT);
    payload.fill(0);
    frame.fill(0);
    result
}

#[cfg(unix)]
fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>, AdministrativeProtocolError> {
    read_frame_with_timeout(stream, ADMIN_TIMEOUT)
}

#[cfg(unix)]
fn read_frame_with_timeout(
    stream: &mut UnixStream,
    timeout: Duration,
) -> Result<Vec<u8>, AdministrativeProtocolError> {
    let deadline = Instant::now() + timeout;
    let mut header = [0_u8; 8];
    read_exact_before(stream, &mut header, deadline)?;
    if header[..4] != ADMIN_MAGIC {
        return Err(AdministrativeProtocolError::Framing);
    }
    let size = u32::from_be_bytes(
        header[4..]
            .try_into()
            .map_err(|_| AdministrativeProtocolError::Framing)?,
    ) as usize;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err(AdministrativeProtocolError::Framing);
    }
    let mut payload = vec![0_u8; size];
    read_exact_before(stream, &mut payload, deadline)?;
    Ok(payload)
}

#[cfg(unix)]
fn configure_nonblocking(stream: &UnixStream) -> Result<(), AdministrativeProtocolError> {
    stream
        .set_nonblocking(true)
        .map_err(|_| AdministrativeProtocolError::Transport("socket-nonblocking"))
}

#[cfg(unix)]
fn read_exact_before(
    stream: &mut UnixStream,
    mut output: &mut [u8],
    deadline: Instant,
) -> Result<(), AdministrativeProtocolError> {
    while !output.is_empty() {
        wait_for(stream.as_raw_fd(), libc::POLLIN, deadline)?;
        let count =
            unsafe { libc::read(stream.as_raw_fd(), output.as_mut_ptr().cast(), output.len()) };
        if count == 0 {
            return Err(AdministrativeProtocolError::Transport("stream-eof"));
        }
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            return Err(AdministrativeProtocolError::Transport("stream-read"));
        }
        output = &mut output[count as usize..];
    }
    Ok(())
}

#[cfg(unix)]
fn write_all_before(
    stream: &mut UnixStream,
    mut input: &[u8],
    deadline: Instant,
) -> Result<(), AdministrativeProtocolError> {
    while !input.is_empty() {
        wait_for(stream.as_raw_fd(), libc::POLLOUT, deadline)?;
        let chunk = &input[..input.len().min(64 * 1024)];
        let count = unsafe { libc::write(stream.as_raw_fd(), chunk.as_ptr().cast(), chunk.len()) };
        if count == 0 {
            return Err(AdministrativeProtocolError::Transport("stream-write-zero"));
        }
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            return Err(AdministrativeProtocolError::Transport("stream-write"));
        }
        input = &input[count as usize..];
    }
    Ok(())
}

#[cfg(unix)]
fn wait_for(
    raw_fd: i32,
    events: libc::c_short,
    deadline: Instant,
) -> Result<(), AdministrativeProtocolError> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(AdministrativeProtocolError::Transport("deadline"))?;
        let timeout_ms = i32::try_from(remaining.as_millis().max(1)).unwrap_or(i32::MAX);
        let mut descriptor = libc::pollfd {
            fd: raw_fd,
            events,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        if result == 0 {
            return Err(AdministrativeProtocolError::Transport("deadline"));
        }
        if result < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(AdministrativeProtocolError::Transport("poll"));
        }
        if descriptor.revents & (events | libc::POLLHUP) != 0 {
            return Ok(());
        }
        return Err(AdministrativeProtocolError::Transport("poll-events"));
    }
}

#[cfg(unix)]
struct DeadlineReader<'a> {
    stream: &'a mut UnixStream,
    deadline: Instant,
}

#[cfg(unix)]
impl<'a> DeadlineReader<'a> {
    fn new(stream: &'a mut UnixStream, timeout: Duration) -> Self {
        Self {
            stream,
            deadline: Instant::now() + timeout,
        }
    }
}

#[cfg(unix)]
impl Read for DeadlineReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        loop {
            wait_for(self.stream.as_raw_fd(), libc::POLLIN, self.deadline).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "admin read deadline")
            })?;
            let count = unsafe {
                libc::read(
                    self.stream.as_raw_fd(),
                    output.as_mut_ptr().cast(),
                    output.len(),
                )
            };
            if count >= 0 {
                return Ok(count as usize);
            }
            let error = std::io::Error::last_os_error();
            if !matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                return Err(error);
            }
        }
    }
}

#[cfg(unix)]
struct DeadlineWriter<'a> {
    stream: &'a mut UnixStream,
    deadline: Instant,
}

#[cfg(unix)]
impl<'a> DeadlineWriter<'a> {
    fn new(stream: &'a mut UnixStream, timeout: Duration) -> Self {
        Self {
            stream,
            deadline: Instant::now() + timeout,
        }
    }
}

#[cfg(unix)]
impl Write for DeadlineWriter<'_> {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        loop {
            wait_for(self.stream.as_raw_fd(), libc::POLLOUT, self.deadline).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "admin write deadline")
            })?;
            let chunk = &input[..input.len().min(64 * 1024)];
            let count =
                unsafe { libc::write(self.stream.as_raw_fd(), chunk.as_ptr().cast(), chunk.len()) };
            if count >= 0 {
                return Ok(count as usize);
            }
            let error = std::io::Error::last_os_error();
            if !matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                return Err(error);
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

fn required_absolute_path(value: Option<OsString>) -> Result<PathBuf, AdministrativeProtocolError> {
    let value = value.ok_or(AdministrativeProtocolError::Endpoint)?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(AdministrativeProtocolError::Endpoint);
    }
    Ok(path)
}

fn clear_operator_path_environment() {
    for name in [
        ADMIN_SOCKET_ENV,
        MIGRATION_SOURCE_FD_ENV,
        MIGRATION_INVENTORY_FD_ENV,
        EXPECTED_SOURCE_SHA256_ENV,
        EXPECTED_STAGING_SHA256_ENV,
        EXPECTED_NORMALIZED_SHA256_ENV,
        EXPECTED_SEMANTIC_SHA256_ENV,
        EXPECTED_DRY_RUN_SHA256_ENV,
        EXPECTED_CONVERSION_REPORT_SHA256_ENV,
        MIGRATION_PROJECT_ID_ENV,
        MIGRATION_WORKSPACE_ID_ENV,
        EXPECTED_MANIFEST_SHA256_ENV,
        MIGRATION_ATTESTATION_ID_ENV,
        EXPECTED_ATTESTATION_SHA256_ENV,
        MIGRATION_APPROVAL_ID_ENV,
        EXPECTED_APPROVAL_SHA256_ENV,
        EXPECTED_CANDIDATE_LOGICAL_SHA256_ENV,
        ROLLBACK_LEGACY_SHA256_ENV,
        ROLLBACK_WINDOW_SECONDS_ENV,
        MIGRATION_ID_ENV,
    ] {
        std::env::remove_var(name);
    }
}

fn clear_capability_environment() {
    for name in [
        ADMIN_SOCKET_ENV,
        CAPABILITY_OUTPUT_FD_ENV,
        CAPABILITY_TARGET_FD_ENV,
        CAPABILITY_SUBJECT_UID_ENV,
        CAPABILITY_ACTOR_ENV,
        CAPABILITY_CHANNEL_ENV,
        CAPABILITY_OPERATION_ENV,
        CAPABILITY_PROJECT_ID_ENV,
        CAPABILITY_WORKSPACE_ID_ENV,
        CAPABILITY_TASK_ID_ENV,
        CAPABILITY_LIFETIME_SECONDS_ENV,
        CAPABILITY_REVOCATION_EPOCH_ENV,
    ] {
        std::env::remove_var(name);
    }
}

fn clear_task_package_environment() {
    for name in [ADMIN_SOCKET_ENV, TASK_PACKAGE_FD_ENV] {
        std::env::remove_var(name);
    }
}

fn clear_task_advance_environment() {
    for name in [ADMIN_SOCKET_ENV, TASK_ADVANCE_FD_ENV] {
        std::env::remove_var(name);
    }
}

fn parse_decimal<T: std::str::FromStr>(
    value: Option<String>,
) -> Result<T, AdministrativeProtocolError> {
    value
        .and_then(|value| value.parse::<T>().ok())
        .ok_or(AdministrativeProtocolError::Envelope)
}

#[cfg(unix)]
fn exchange_administrative_request(
    endpoint: &Path,
    mut capability: WireCapability,
    operation: &str,
    mut body: AdministrativeBody,
) -> Result<AdministrativeResult, AdministrativeProtocolError> {
    let mut stream = match UnixStream::connect(endpoint) {
        Ok(stream) => stream,
        Err(_) => {
            erase_request_secrets(&mut capability, &mut body);
            return Err(AdministrativeProtocolError::Transport("connect"));
        }
    };
    if set_descriptor_close_on_exec(stream.as_raw_fd()).is_err() {
        erase_request_secrets(&mut capability, &mut body);
        return Err(AdministrativeProtocolError::Transport("close-on-exec"));
    }
    let connection_nonce = match negotiate_client(&mut stream) {
        Ok(nonce) => nonce,
        Err(error) => {
            erase_request_secrets(&mut capability, &mut body);
            return Err(error);
        }
    };
    let request_id = match random_request_id() {
        Ok(request_id) => request_id,
        Err(_) => {
            erase_request_secrets(&mut capability, &mut body);
            return Err(AdministrativeProtocolError::Envelope);
        }
    };
    let mut request = AdministrativeRequest {
        message_type: "request".to_string(),
        wire_version: ADMIN_WIRE_VERSION.to_string(),
        request_id: request_id.clone(),
        connection_nonce,
        operation: operation.to_string(),
        capability,
        body,
    };
    let write_result = write_frame(&mut stream, &request);
    erase_string(&mut request.capability.token);
    if let AdministrativeBody::Grant(body) = &mut request.body {
        erase_string(&mut body.token);
    }
    write_result?;
    let response: AdministrativeResponse =
        serde_json::from_slice(&read_frame_with_timeout(&mut stream, ADMIN_TIMEOUT)?)
            .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    response.result.ok_or(AdministrativeProtocolError::Envelope)
}

fn erase_request_secrets(capability: &mut WireCapability, body: &mut AdministrativeBody) {
    erase_string(&mut capability.token);
    if let AdministrativeBody::Grant(body) = body {
        erase_string(&mut body.token);
    }
}

fn canonical_json_sha256<T: Serialize>(value: &T) -> Result<String, AdministrativeProtocolError> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| AdministrativeProtocolError::Envelope)?;
    Ok(crate::legacy_staging::lowercase_hex(&sha256(&bytes)))
}

#[cfg(unix)]
fn write_capability_bundle_to_fd(
    raw: OsString,
    capability: &AdministrativeCapabilityBundle,
) -> Result<(), AdministrativeProtocolError> {
    let raw_fd = parse_open_descriptor(&raw)?;
    let mut file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
    set_descriptor_close_on_exec(file.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Authorization)?;
    let mut status: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(file.as_raw_fd(), &mut status) } != 0
        || (status.st_mode & libc::S_IFMT) != libc::S_IFREG
        || (status.st_mode & 0o777) != 0o600
        || status.st_uid != unsafe { libc::geteuid() }
        || status.st_nlink != 1
        || status.st_size != 0
    {
        return Err(AdministrativeProtocolError::Authorization);
    }
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 || (flags & libc::O_ACCMODE) == libc::O_RDONLY {
        return Err(AdministrativeProtocolError::Authorization);
    }
    let mut bytes =
        serde_json::to_vec(capability).map_err(|_| AdministrativeProtocolError::Envelope)?;
    bytes.push(b'\n');
    if bytes.len() > 4096 {
        bytes.fill(0);
        return Err(AdministrativeProtocolError::Envelope);
    }
    let result = file
        .write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| AdministrativeProtocolError::Transport("capability-descriptor-write"));
    bytes.fill(0);
    result
}

#[cfg(unix)]
fn parse_open_descriptor(raw: &OsStr) -> Result<i32, AdministrativeProtocolError> {
    raw.to_str()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|fd| *fd >= 3 && unsafe { libc::fcntl(*fd, libc::F_GETFD) } >= 0)
        .ok_or(AdministrativeProtocolError::Authorization)
}

#[cfg(unix)]
fn read_migration_source_inputs(
    source_raw: OsString,
    inventory_raw: OsString,
) -> Result<(std::fs::File, Vec<u8>), AdministrativeProtocolError> {
    let source_fd = parse_open_descriptor(&source_raw)?;
    let inventory_fd = parse_open_descriptor(&inventory_raw)?;
    if source_fd == inventory_fd {
        return Err(AdministrativeProtocolError::Authorization);
    }

    let source = unsafe { std::fs::File::from_raw_fd(source_fd) };
    set_descriptor_close_on_exec(source.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Authorization)?;
    let mut source_status: libc::stat = unsafe { std::mem::zeroed() };
    let source_flags = unsafe { libc::fcntl(source.as_raw_fd(), libc::F_GETFL) };
    if unsafe { libc::fstat(source.as_raw_fd(), &mut source_status) } != 0
        || (source_status.st_mode & libc::S_IFMT) != libc::S_IFDIR
        || source_flags < 0
        || (source_flags & libc::O_ACCMODE) != libc::O_RDONLY
    {
        return Err(AdministrativeProtocolError::Authorization);
    }

    let mut inventory = unsafe { std::fs::File::from_raw_fd(inventory_fd) };
    set_descriptor_close_on_exec(inventory.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Authorization)?;
    let mut inventory_status: libc::stat = unsafe { std::mem::zeroed() };
    let inventory_flags = unsafe { libc::fcntl(inventory.as_raw_fd(), libc::F_GETFL) };
    let inventory_offset = unsafe { libc::lseek(inventory.as_raw_fd(), 0, libc::SEEK_CUR) };
    if unsafe { libc::fstat(inventory.as_raw_fd(), &mut inventory_status) } != 0
        || (inventory_status.st_mode & libc::S_IFMT) != libc::S_IFREG
        || inventory_status.st_size < 0
        || inventory_status.st_size as u64 > MAX_INVENTORY_BYTES
        || inventory_flags < 0
        || (inventory_flags & libc::O_ACCMODE) != libc::O_RDONLY
        || inventory_offset != 0
    {
        return Err(AdministrativeProtocolError::Authorization);
    }
    let expected_size = inventory_status.st_size as usize;
    let mut bytes = Vec::with_capacity(expected_size);
    inventory
        .read_to_end(&mut bytes)
        .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if bytes.len() != expected_size {
        return Err(AdministrativeProtocolError::Envelope);
    }
    Ok((source, bytes))
}

#[cfg(unix)]
fn read_bounded_json_from_raw_fd(raw: OsString) -> Result<Vec<u8>, AdministrativeProtocolError> {
    let raw_fd = parse_open_descriptor(&raw)?;
    let mut file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
    set_descriptor_close_on_exec(file.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Authorization)?;
    let mut status: libc::stat = unsafe { std::mem::zeroed() };
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    let offset = unsafe { libc::lseek(file.as_raw_fd(), 0, libc::SEEK_CUR) };
    if unsafe { libc::fstat(file.as_raw_fd(), &mut status) } != 0
        || (status.st_mode & libc::S_IFMT) != libc::S_IFREG
        || status.st_size <= 0
        || status.st_size as u64 > MAX_TASK_PACKAGE_BYTES
        || flags < 0
        || (flags & libc::O_ACCMODE) != libc::O_RDONLY
        || offset != 0
    {
        return Err(AdministrativeProtocolError::Authorization);
    }
    let expected_size = status.st_size as usize;
    let mut bytes = Vec::with_capacity(expected_size);
    file.read_to_end(&mut bytes)
        .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if bytes.len() != expected_size {
        bytes.fill(0);
        return Err(AdministrativeProtocolError::Envelope);
    }
    Ok(bytes)
}

#[cfg(unix)]
fn read_administrative_capability(
) -> Result<AdministrativeCapabilityBundle, AdministrativeProtocolError> {
    let raw = std::env::var_os(ADMIN_CAPABILITY_FD_ENV)
        .ok_or(AdministrativeProtocolError::Authorization)?;
    std::env::remove_var(ADMIN_CAPABILITY_FD_ENV);
    let mut capability = read_capability_bundle_from_raw_fd(raw)?;
    if !matches!(
        capability.contract_version.as_str(),
        "admin-capability-v1" | "ci-state-transition-capability-v1"
    ) {
        erase_string(&mut capability.capability.token);
        return Err(AdministrativeProtocolError::Authorization);
    }
    Ok(capability)
}

#[cfg(unix)]
fn read_capability_bundle_from_raw_fd(
    raw: OsString,
) -> Result<AdministrativeCapabilityBundle, AdministrativeProtocolError> {
    let raw_fd = parse_open_descriptor(&raw)?;
    let mut file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
    set_descriptor_close_on_exec(file.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Authorization)?;
    let mut bytes = Vec::with_capacity(512);
    let read_result = std::io::Read::by_ref(&mut file)
        .take(4097)
        .read_to_end(&mut bytes);
    if read_result.is_err() {
        bytes.fill(0);
        return Err(AdministrativeProtocolError::Authorization);
    }
    if bytes.is_empty() || bytes.len() > 4096 {
        bytes.fill(0);
        return Err(AdministrativeProtocolError::Authorization);
    }
    let parsed = serde_json::from_slice(&bytes);
    bytes.fill(0);
    let mut capability: AdministrativeCapabilityBundle =
        parsed.map_err(|_| AdministrativeProtocolError::Authorization)?;
    if !matches!(
        capability.contract_version.as_str(),
        "admin-capability-v1"
            | "agent-submission-capability-v1"
            | "reviewer-assignment-capability-v1"
            | "reviewer-decision-capability-v1"
            | "ci-state-transition-capability-v1"
            | "wrapper-projection-capability-v1"
    ) || !valid_opaque_id(&capability.capability.capability_id, "cap_")
        || !is_encoded_nonce(&capability.capability.token)
    {
        erase_string(&mut capability.capability.token);
        return Err(AdministrativeProtocolError::Authorization);
    }
    Ok(capability)
}

#[cfg(not(unix))]
fn read_administrative_capability(
) -> Result<AdministrativeCapabilityBundle, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Authorization)
}

#[cfg(test)]
#[path = "tests/Administrative_Protocol_test.rs"]
mod administrative_protocol_tests;
