use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::structs::{
    CapabilityActor, CapabilityChannel, CapabilityOperation, InitialCapabilityProvisioning,
    NewCapabilityGrant, ProjectDatabase,
};
use serde::Deserialize;

use crate::limited_protocol::decode_base64url;
use crate::service_authority::VerifiedServiceAuthority;
use crate::service_storage::ProtectedServiceStorage;

const CREDENTIALS_DIRECTORY_ENV: &str = "CREDENTIALS_DIRECTORY";
const INITIAL_CREDENTIAL_NAME: &str = "antdelirium-bootstrap-v1";
const INITIAL_CONTRACT_VERSION: &str = "initial-capability-provisioning-v1";
const MAX_CREDENTIAL_BYTES: u64 = 64 * 1024;
const MAX_MANIFEST_LIFETIME_SECONDS: i64 = 15 * 60;
const MAX_GRANT_LIFETIME_SECONDS: i64 = 366 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InitialProvisioningError {
    CredentialContract,
    CredentialAccess,
    Manifest,
    AlreadyProvisioned,
    MissingInitialCredential,
    Storage,
    Clock,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireInitialProvisioning {
    contract_version: String,
    provisioning_id: String,
    issued_at_unix: i64,
    expires_at_unix: i64,
    grants: Vec<WireInitialGrant>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireInitialGrant {
    capability_id: String,
    token: String,
    subject_uid: u32,
    actor: String,
    channel: String,
    operation: String,
    project_id: Option<String>,
    workspace_id: Option<String>,
    task_id: Option<String>,
    expires_at_unix: i64,
    revocation_epoch: u64,
}

pub(crate) fn apply_configured_initial_provisioning(
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
) -> Result<(), InitialProvisioningError> {
    let configured_directory = std::env::var_os(CREDENTIALS_DIRECTORY_ENV);
    std::env::remove_var(CREDENTIALS_DIRECTORY_ENV);
    let existing = storage
        .registry()
        .initial_capability_provisioning_receipt()
        .map_err(|_| InitialProvisioningError::Storage)?;
    let Some(directory) = configured_directory else {
        return existing
            .map(|_| ())
            .ok_or(InitialProvisioningError::MissingInitialCredential);
    };
    if existing.is_some() {
        return Err(InitialProvisioningError::AlreadyProvisioned);
    }
    let credential = read_systemd_credential(&directory, authority.effective_uid())?;
    let now_unix = trusted_unix_time()?;
    apply_manifest(storage.registry(), credential, now_unix)
}

fn read_systemd_credential(
    directory: &OsStr,
    service_uid: u32,
) -> Result<Vec<u8>, InitialProvisioningError> {
    let directory = PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err(InitialProvisioningError::CredentialContract);
    }
    require_private_metadata(&directory, service_uid, true)?;
    let credential_path = directory.join(INITIAL_CREDENTIAL_NAME);
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let credential = options
        .open(&credential_path)
        .map_err(|_| InitialProvisioningError::CredentialAccess)?;
    let metadata = credential
        .metadata()
        .map_err(|_| InitialProvisioningError::CredentialAccess)?;
    require_private_file_metadata(&metadata, service_uid)?;
    if metadata.len() > MAX_CREDENTIAL_BYTES {
        return Err(InitialProvisioningError::CredentialContract);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    credential
        .take(MAX_CREDENTIAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| InitialProvisioningError::CredentialAccess)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_CREDENTIAL_BYTES {
        bytes.fill(0);
        return Err(InitialProvisioningError::CredentialContract);
    }
    Ok(bytes)
}

fn apply_manifest(
    registry: &ProjectDatabase,
    mut credential: Vec<u8>,
    now_unix: i64,
) -> Result<(), InitialProvisioningError> {
    let manifest_sha256 = sha256(&credential);
    let parsed = serde_json::from_slice::<WireInitialProvisioning>(&credential);
    credential.fill(0);
    let mut manifest = parsed.map_err(|_| InitialProvisioningError::Manifest)?;
    let provisioning = validate_and_convert_manifest(&mut manifest, manifest_sha256, now_unix)?;
    registry
        .provision_initial_capabilities(&provisioning)
        .map(|_| ())
        .map_err(|_| InitialProvisioningError::Storage)
}

fn validate_and_convert_manifest(
    manifest: &mut WireInitialProvisioning,
    manifest_sha256: [u8; 32],
    now_unix: i64,
) -> Result<InitialCapabilityProvisioning, InitialProvisioningError> {
    let manifest_lifetime = manifest
        .expires_at_unix
        .checked_sub(manifest.issued_at_unix)
        .ok_or(InitialProvisioningError::Manifest)?;
    if manifest.contract_version != INITIAL_CONTRACT_VERSION
        || manifest.issued_at_unix < 0
        || manifest_lifetime <= 0
        || manifest_lifetime > MAX_MANIFEST_LIFETIME_SECONDS
        || now_unix < manifest.issued_at_unix
        || now_unix >= manifest.expires_at_unix
        || !(2..=16).contains(&manifest.grants.len())
    {
        erase_wire_tokens(&mut manifest.grants);
        return Err(InitialProvisioningError::Manifest);
    }
    let not_before_unix = manifest.issued_at_unix;
    let granted_at = format!("unix:{now_unix}");
    let converted = manifest
        .grants
        .iter()
        .map(|grant| {
            let token =
                decode_base64url(&grant.token).map_err(|_| InitialProvisioningError::Manifest)?;
            let grant_lifetime = grant
                .expires_at_unix
                .checked_sub(now_unix)
                .ok_or(InitialProvisioningError::Manifest)?;
            if grant_lifetime <= 0 || grant_lifetime > MAX_GRANT_LIFETIME_SECONDS {
                return Err(InitialProvisioningError::Manifest);
            }
            Ok(NewCapabilityGrant {
                grant_id: grant.capability_id.clone(),
                secret_hash: sha256(&token),
                subject_uid: grant.subject_uid,
                actor: parse_actor(&grant.actor)?,
                channel: parse_channel(&grant.channel)?,
                operation: parse_operation(&grant.operation)?,
                project_id: grant.project_id.clone(),
                workspace_id: grant.workspace_id.clone(),
                task_id: grant.task_id.clone(),
                not_before_unix,
                expires_at_unix: grant.expires_at_unix,
                revocation_epoch: grant.revocation_epoch,
                granted_at: granted_at.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>();
    erase_wire_tokens(&mut manifest.grants);
    let grants = converted?;
    Ok(InitialCapabilityProvisioning {
        provisioning_id: manifest.provisioning_id.clone(),
        manifest_sha256,
        applied_at_unix: now_unix,
        grants,
    })
}

fn parse_actor(value: &str) -> Result<CapabilityActor, InitialProvisioningError> {
    match value {
        "operator" => Ok(CapabilityActor::Operator),
        "ci" => Ok(CapabilityActor::Ci),
        "reviewer" => Ok(CapabilityActor::Reviewer),
        _ => Err(InitialProvisioningError::Manifest),
    }
}

fn parse_channel(value: &str) -> Result<CapabilityChannel, InitialProvisioningError> {
    match value {
        "limited" => Ok(CapabilityChannel::Limited),
        "administrative" => Ok(CapabilityChannel::Administrative),
        _ => Err(InitialProvisioningError::Manifest),
    }
}

fn parse_operation(value: &str) -> Result<CapabilityOperation, InitialProvisioningError> {
    match value {
        "compile.observe" => Ok(CapabilityOperation::CompileObserve),
        "projection.refresh" => Ok(CapabilityOperation::ProjectionRefresh),
        "task.current.read" => Ok(CapabilityOperation::TaskCurrentRead),
        "submission.create" => Ok(CapabilityOperation::SubmissionCreate),
        "submission.read.assigned" => Ok(CapabilityOperation::SubmissionReadAssigned),
        "review.decide.assigned" => Ok(CapabilityOperation::ReviewDecideAssigned),
        "attestation.create" => Ok(CapabilityOperation::AttestationCreate),
        "state.transition" => Ok(CapabilityOperation::StateTransition),
        "state.admin" => Ok(CapabilityOperation::StateAdmin),
        "installation.admin" => Ok(CapabilityOperation::InstallationAdmin),
        "diagnostic.private" => Ok(CapabilityOperation::DiagnosticPrivate),
        _ => Err(InitialProvisioningError::Manifest),
    }
}

fn erase_wire_tokens(grants: &mut [WireInitialGrant]) {
    for grant in grants {
        erase_string(&mut grant.token);
    }
}

fn erase_string(value: &mut String) {
    // SAFETY: overwriting existing bytes with zero preserves UTF-8 validity and
    // length. The string is not read again as protocol input after erasure.
    unsafe { value.as_mut_vec().fill(0) };
}

fn trusted_unix_time() -> Result<i64, InitialProvisioningError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| InitialProvisioningError::Clock)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| InitialProvisioningError::Clock)
}

#[cfg(unix)]
fn require_private_metadata(
    path: &Path,
    service_uid: u32,
    directory: bool,
) -> Result<(), InitialProvisioningError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| InitialProvisioningError::CredentialAccess)?;
    let owner_uid = metadata.uid();
    if metadata.file_type().is_symlink()
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
        || (owner_uid != 0 && owner_uid != service_uid)
        || metadata.mode() & 0o077 != 0
    {
        return Err(InitialProvisioningError::CredentialContract);
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_private_metadata(
    _path: &Path,
    _service_uid: u32,
    _directory: bool,
) -> Result<(), InitialProvisioningError> {
    Err(InitialProvisioningError::CredentialContract)
}

#[cfg(unix)]
fn require_private_file_metadata(
    metadata: &fs::Metadata,
    service_uid: u32,
) -> Result<(), InitialProvisioningError> {
    let owner_uid = metadata.uid();
    if !metadata.is_file()
        || (owner_uid != 0 && owner_uid != service_uid)
        || metadata.mode() & 0o077 != 0
    {
        return Err(InitialProvisioningError::CredentialContract);
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_private_file_metadata(
    _metadata: &fs::Metadata,
    _service_uid: u32,
) -> Result<(), InitialProvisioningError> {
    Err(InitialProvisioningError::CredentialContract)
}

#[cfg(test)]
#[path = "tests/Service_Provisioning_test.rs"]
mod service_provisioning_tests;
