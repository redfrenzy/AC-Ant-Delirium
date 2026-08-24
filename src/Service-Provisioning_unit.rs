use std::ffi::OsStr;
use std::fs::File;
#[cfg(target_os = "linux")]
use std::fs::{Metadata, OpenOptions};
use std::io::Read;
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(target_os = "linux")]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
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
const INITIAL_CONTRACT_VERSION: &str = "initial-capability-provisioning-v1";
const MAX_CREDENTIAL_BYTES: u64 = 64 * 1024;
const MAX_MANIFEST_LIFETIME_SECONDS: i64 = 15 * 60;
const MAX_GRANT_LIFETIME_SECONDS: i64 = 366 * 24 * 60 * 60;
#[cfg(target_os = "linux")]
const POSIX_ACL_XATTR_NAME: &[u8] = b"system.posix_acl_access\0";
#[cfg(target_os = "linux")]
const POSIX_ACL_XATTR_VERSION: u32 = 2;
#[cfg(target_os = "linux")]
const POSIX_ACL_MAX_BYTES: usize = 4 + 32 * 8;
#[cfg(target_os = "linux")]
const ACL_USER_OBJ: u16 = 0x01;
#[cfg(target_os = "linux")]
const ACL_USER: u16 = 0x02;
#[cfg(target_os = "linux")]
const ACL_GROUP_OBJ: u16 = 0x04;
#[cfg(target_os = "linux")]
const ACL_MASK: u16 = 0x10;
#[cfg(target_os = "linux")]
const ACL_OTHER: u16 = 0x20;
#[cfg(target_os = "linux")]
const ACL_READ: u16 = 0x04;
#[cfg(target_os = "linux")]
const ACL_EXECUTE: u16 = 0x01;
#[cfg(target_os = "linux")]
const ACL_UNDEFINED_ID: u32 = u32::MAX;

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

impl InitialProvisioningError {
    pub(crate) fn diagnostic_code(self) -> &'static str {
        match self {
            Self::CredentialContract => "AD-PROVISION-CREDENTIAL-CONTRACT",
            Self::CredentialAccess => "AD-PROVISION-CREDENTIAL-ACCESS",
            Self::Manifest => "AD-PROVISION-MANIFEST",
            Self::AlreadyProvisioned => "AD-PROVISION-REPLAY",
            Self::MissingInitialCredential => "AD-PROVISION-MISSING",
            Self::Storage => "AD-PROVISION-STORAGE",
            Self::Clock => "AD-PROVISION-CLOCK",
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PosixAclEntry {
    tag: u16,
    permissions: u16,
    id: u32,
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
    let directory = open_systemd_credential_directory(&directory, service_uid)?;
    let credential = open_systemd_credential(&directory, service_uid)?;
    let metadata = credential
        .metadata()
        .map_err(|_| InitialProvisioningError::CredentialAccess)?;
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

#[cfg(target_os = "linux")]
fn open_systemd_credential_directory(
    path: &std::path::Path,
    service_uid: u32,
) -> Result<File, InitialProvisioningError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let directory = options
        .open(path)
        .map_err(|_| InitialProvisioningError::CredentialAccess)?;
    let metadata = directory
        .metadata()
        .map_err(|_| InitialProvisioningError::CredentialAccess)?;
    require_systemd_credential_metadata(&directory, &metadata, service_uid, true)?;
    Ok(directory)
}

#[cfg(not(target_os = "linux"))]
fn open_systemd_credential_directory(
    _path: &std::path::Path,
    _service_uid: u32,
) -> Result<File, InitialProvisioningError> {
    Err(InitialProvisioningError::CredentialContract)
}

#[cfg(target_os = "linux")]
fn open_systemd_credential(
    directory: &File,
    service_uid: u32,
) -> Result<File, InitialProvisioningError> {
    let name = b"antdelirium-bootstrap-v1\0";
    // SAFETY: `directory` is an open directory descriptor, `name` is a fixed
    // NUL-terminated byte string, and a successful descriptor is immediately
    // owned by `File`. O_NOFOLLOW prevents replacing the credential with a
    // symlink between validation and open.
    let descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr().cast(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(InitialProvisioningError::CredentialAccess);
    }
    // SAFETY: `openat` returned a new owned descriptor.
    let credential = unsafe { File::from_raw_fd(descriptor) };
    let metadata = credential
        .metadata()
        .map_err(|_| InitialProvisioningError::CredentialAccess)?;
    require_systemd_credential_metadata(&credential, &metadata, service_uid, false)?;
    Ok(credential)
}

#[cfg(not(target_os = "linux"))]
fn open_systemd_credential(
    _directory: &File,
    _service_uid: u32,
) -> Result<File, InitialProvisioningError> {
    Err(InitialProvisioningError::CredentialContract)
}

#[cfg(target_os = "linux")]
fn require_systemd_credential_metadata(
    file: &File,
    metadata: &Metadata,
    service_uid: u32,
    directory: bool,
) -> Result<(), InitialProvisioningError> {
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(InitialProvisioningError::CredentialContract);
    }

    let private_mode = if directory { 0o500 } else { 0o400 };
    let acl_mode = if directory { 0o550 } else { 0o440 };
    match metadata.uid() {
        owner if owner == service_uid => {
            if metadata.mode() & 0o777 != private_mode || read_posix_acl(file)?.is_some() {
                return Err(InitialProvisioningError::CredentialContract);
            }
        }
        0 => {
            if metadata.mode() & 0o777 != acl_mode {
                return Err(InitialProvisioningError::CredentialContract);
            }
            let acl = read_posix_acl(file)?.ok_or(InitialProvisioningError::CredentialContract)?;
            require_exact_systemd_acl(&acl, service_uid, directory)?;
        }
        _ => return Err(InitialProvisioningError::CredentialContract),
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_posix_acl(file: &File) -> Result<Option<Vec<PosixAclEntry>>, InitialProvisioningError> {
    let name = POSIX_ACL_XATTR_NAME.as_ptr().cast();
    // SAFETY: the descriptor is valid, the attribute name is NUL-terminated,
    // and a null value with length zero asks Linux only for the required size.
    let required = unsafe { libc::fgetxattr(file.as_raw_fd(), name, std::ptr::null_mut(), 0) };
    if required < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(code) if code == libc::ENODATA || code == libc::EOPNOTSUPP => Ok(None),
            _ => Err(InitialProvisioningError::CredentialAccess),
        };
    }
    let required =
        usize::try_from(required).map_err(|_| InitialProvisioningError::CredentialContract)?;
    if required < 4 || required > POSIX_ACL_MAX_BYTES {
        return Err(InitialProvisioningError::CredentialContract);
    }
    let mut bytes = vec![0_u8; required];
    // SAFETY: `bytes` owns `required` writable bytes and the descriptor/name
    // remain valid for the duration of the call.
    let read = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            name,
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        )
    };
    if read < 0 || usize::try_from(read).ok() != Some(required) {
        return Err(InitialProvisioningError::CredentialAccess);
    }
    parse_posix_acl(&bytes).map(Some)
}

#[cfg(target_os = "linux")]
fn parse_posix_acl(bytes: &[u8]) -> Result<Vec<PosixAclEntry>, InitialProvisioningError> {
    if bytes.len() < 4 || (bytes.len() - 4) % 8 != 0 {
        return Err(InitialProvisioningError::CredentialContract);
    }
    let version = u32::from_le_bytes(
        bytes[0..4]
            .try_into()
            .map_err(|_| InitialProvisioningError::CredentialContract)?,
    );
    if version != POSIX_ACL_XATTR_VERSION {
        return Err(InitialProvisioningError::CredentialContract);
    }
    bytes[4..]
        .chunks_exact(8)
        .map(|entry| {
            let tag = u16::from_le_bytes([entry[0], entry[1]]);
            let permissions = u16::from_le_bytes([entry[2], entry[3]]);
            let id = u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]);
            if permissions & !0o7 != 0 {
                return Err(InitialProvisioningError::CredentialContract);
            }
            Ok(PosixAclEntry {
                tag,
                permissions,
                id,
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn require_exact_systemd_acl(
    entries: &[PosixAclEntry],
    service_uid: u32,
    directory: bool,
) -> Result<(), InitialProvisioningError> {
    let allowed = ACL_READ | if directory { ACL_EXECUTE } else { 0 };
    let required = [
        PosixAclEntry {
            tag: ACL_USER_OBJ,
            permissions: allowed,
            id: ACL_UNDEFINED_ID,
        },
        PosixAclEntry {
            tag: ACL_USER,
            permissions: allowed,
            id: service_uid,
        },
        PosixAclEntry {
            tag: ACL_GROUP_OBJ,
            permissions: 0,
            id: ACL_UNDEFINED_ID,
        },
        PosixAclEntry {
            tag: ACL_MASK,
            permissions: allowed,
            id: ACL_UNDEFINED_ID,
        },
        PosixAclEntry {
            tag: ACL_OTHER,
            permissions: 0,
            id: ACL_UNDEFINED_ID,
        },
    ];
    if entries.len() != required.len()
        || required.iter().any(|expected| !entries.contains(expected))
    {
        return Err(InitialProvisioningError::CredentialContract);
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/Service_Provisioning_test.rs"]
mod service_provisioning_tests;
