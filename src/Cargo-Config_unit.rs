use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use serde::{Deserialize, Serialize};
use toml_edit::{value, DocumentMut};

const CONFIG_FILE_NAME: &str = "config.toml";
const LEGACY_CONFIG_FILE_NAME: &str = "config";
const BACKUP_DIRECTORY_NAME: &str = ".antdelirium-install";
const BACKUP_PAYLOAD_NAME: &str = "config.toml.original";
const BACKUP_RECEIPT_NAME: &str = "config-backup-v1.json";
const BACKUP_CONTRACT_VERSION: &str = "cargo-config-backup-v1";
const INSTALLED_BINARY_NAME: &str = "antdelirium";
const MAXIMUM_CONFIG_BYTES: u64 = 4 * 1024 * 1024;
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CargoConfigMergeOutcome {
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceWrapperStatus {
    ActiveOwned,
    ActiveUnclaimed,
    Missing,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CargoConfigError {
    InvalidCargoHome,
    UnsafeFilesystemEntry,
    LegacyConfigConflict,
    InvalidConfig,
    ConfigTooLarge,
    WrapperConflict,
    BackupConflict,
    ConcurrentChange,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoConfigBackupReceipt {
    contract_version: String,
    original_present: bool,
    original_sha256: String,
    installed_sha256: String,
    original_mode: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedCargoConfigBackup {
    pub(crate) original_present: bool,
    pub(crate) original_bytes: Vec<u8>,
    pub(crate) original_mode: Option<u32>,
    pub(crate) installed_sha256: String,
}

pub(crate) fn configure_workspace_wrapper(
    cargo_home: &Path,
) -> Result<CargoConfigMergeOutcome, CargoConfigError> {
    let wrapper = expected_workspace_wrapper(cargo_home)?;
    merge_cargo_config(cargo_home, move |document| {
        configure_workspace_wrapper_document(document, &wrapper)
    })
}

pub(crate) fn inspect_workspace_wrapper(
    cargo_home: &Path,
) -> Result<WorkspaceWrapperStatus, CargoConfigError> {
    let owner = effective_uid()?;
    validate_cargo_home(cargo_home, owner)?;
    reject_legacy_config(cargo_home)?;
    let config = match read_existing_config(&cargo_home.join(CONFIG_FILE_NAME), owner)? {
        Some(config) => config,
        None => return Ok(WorkspaceWrapperStatus::Missing),
    };
    let source = std::str::from_utf8(&config.bytes).map_err(|_| CargoConfigError::InvalidConfig)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|_| CargoConfigError::InvalidConfig)?;
    let build = match document.get("build") {
        None => return Ok(WorkspaceWrapperStatus::Missing),
        Some(item) => match item.as_table_like() {
            Some(build) => build,
            None => return Ok(WorkspaceWrapperStatus::Conflict),
        },
    };
    if build.get("rustc-wrapper").is_some() {
        return Ok(WorkspaceWrapperStatus::Conflict);
    }
    let expected = expected_workspace_wrapper(cargo_home)?;
    match build.get("rustc-workspace-wrapper") {
        None => return Ok(WorkspaceWrapperStatus::Missing),
        Some(configured) if configured.as_str() == Some(expected.as_str()) => {}
        Some(_) => return Ok(WorkspaceWrapperStatus::Conflict),
    }
    match fs::symlink_metadata(cargo_home.join(BACKUP_DIRECTORY_NAME)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Ok(WorkspaceWrapperStatus::ActiveUnclaimed)
        }
        Err(_) => Err(CargoConfigError::Io),
        Ok(_) => {
            let backup = read_verified_config_backup(cargo_home)?;
            let expected_mode = backup.original_mode.unwrap_or(0o600);
            if backup.installed_sha256 == lowercase_hex(&sha256(&config.bytes))
                && config.mode == expected_mode
            {
                Ok(WorkspaceWrapperStatus::ActiveOwned)
            } else {
                Ok(WorkspaceWrapperStatus::Conflict)
            }
        }
    }
}

pub(crate) fn preflight_workspace_wrapper(cargo_home: &Path) -> Result<(), CargoConfigError> {
    let owner = effective_uid()?;
    validate_cargo_home(cargo_home, owner)?;
    reject_legacy_config(cargo_home)?;
    let original = read_existing_config(&cargo_home.join(CONFIG_FILE_NAME), owner)?;
    let original_bytes = original
        .as_ref()
        .map(|config| config.bytes.as_slice())
        .unwrap_or_default();
    let source =
        std::str::from_utf8(original_bytes).map_err(|_| CargoConfigError::InvalidConfig)?;
    let mut document = if original.is_some() {
        source
            .parse::<DocumentMut>()
            .map_err(|_| CargoConfigError::InvalidConfig)?
    } else {
        DocumentMut::new()
    };
    configure_workspace_wrapper_document(
        &mut document,
        expected_workspace_wrapper(cargo_home)?.as_str(),
    )?;
    let planned = document.to_string().into_bytes();
    validate_serialized_config(&planned)?;
    if planned != original_bytes {
        match fs::symlink_metadata(cargo_home.join(BACKUP_DIRECTORY_NAME)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => return Err(CargoConfigError::BackupConflict),
            Err(_) => return Err(CargoConfigError::Io),
        }
    }
    Ok(())
}

fn expected_workspace_wrapper(cargo_home: &Path) -> Result<String, CargoConfigError> {
    cargo_home
        .join("bin")
        .join(INSTALLED_BINARY_NAME)
        .to_str()
        .map(str::to_owned)
        .ok_or(CargoConfigError::InvalidCargoHome)
}

fn configure_workspace_wrapper_document(
    document: &mut DocumentMut,
    expected_wrapper: &str,
) -> Result<(), CargoConfigError> {
    let build = document.entry("build").or_insert(toml_edit::table());
    let build = build
        .as_table_like_mut()
        .ok_or(CargoConfigError::WrapperConflict)?;
    if build.get("rustc-wrapper").is_some() {
        return Err(CargoConfigError::WrapperConflict);
    }
    match build.get("rustc-workspace-wrapper") {
        Some(configured) if configured.as_str() == Some(expected_wrapper) => Ok(()),
        Some(_) => Err(CargoConfigError::WrapperConflict),
        None => {
            build.insert("rustc-workspace-wrapper", value(expected_wrapper));
            Ok(())
        }
    }
}

struct ExistingCargoConfig {
    bytes: Vec<u8>,
    metadata: fs::Metadata,
    mode: u32,
}

pub(crate) fn merge_cargo_config(
    cargo_home: &Path,
    mutate: impl FnOnce(&mut DocumentMut) -> Result<(), CargoConfigError>,
) -> Result<CargoConfigMergeOutcome, CargoConfigError> {
    let owner = effective_uid()?;
    validate_cargo_home(cargo_home, owner)?;
    reject_legacy_config(cargo_home)?;
    let config_path = cargo_home.join(CONFIG_FILE_NAME);
    let original = read_existing_config(&config_path, owner)?;
    let original_bytes = original
        .as_ref()
        .map(|config| config.bytes.as_slice())
        .unwrap_or_default();
    let source =
        std::str::from_utf8(original_bytes).map_err(|_| CargoConfigError::InvalidConfig)?;
    let mut document = if original.is_some() {
        source
            .parse::<DocumentMut>()
            .map_err(|_| CargoConfigError::InvalidConfig)?
    } else {
        DocumentMut::new()
    };
    mutate(&mut document)?;
    let installed_bytes = document.to_string().into_bytes();
    validate_serialized_config(&installed_bytes)?;
    if original_bytes == installed_bytes {
        return Ok(CargoConfigMergeOutcome::Unchanged);
    }

    create_recovery_backup(cargo_home, original.as_ref(), &installed_bytes, owner)?;
    write_config_atomically(
        cargo_home,
        &config_path,
        original.as_ref(),
        &installed_bytes,
        owner,
    )?;
    Ok(CargoConfigMergeOutcome::Updated)
}

#[allow(dead_code)]
pub(crate) fn read_verified_config_backup(
    cargo_home: &Path,
) -> Result<VerifiedCargoConfigBackup, CargoConfigError> {
    let owner = effective_uid()?;
    validate_cargo_home(cargo_home, owner)?;
    let backup_directory = cargo_home.join(BACKUP_DIRECTORY_NAME);
    validate_owned_directory(&backup_directory, owner)?;
    if directory_mode(&backup_directory)? != 0o700 {
        return Err(CargoConfigError::InvalidConfig);
    }
    let payload = read_owned_regular_file(
        &backup_directory.join(BACKUP_PAYLOAD_NAME),
        owner,
        MAXIMUM_CONFIG_BYTES,
    )?;
    let receipt_bytes = read_owned_regular_file(
        &backup_directory.join(BACKUP_RECEIPT_NAME),
        owner,
        16 * 1024,
    )?;
    let receipt: CargoConfigBackupReceipt =
        serde_json::from_slice(&receipt_bytes).map_err(|_| CargoConfigError::InvalidConfig)?;
    if receipt.contract_version != BACKUP_CONTRACT_VERSION
        || receipt.original_sha256 != lowercase_hex(&sha256(&payload))
        || receipt.installed_sha256.len() != 64
        || !receipt
            .installed_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || receipt.original_present != receipt.original_mode.is_some()
        || receipt.original_mode.is_some_and(|mode| mode > 0o777)
    {
        return Err(CargoConfigError::InvalidConfig);
    }
    Ok(VerifiedCargoConfigBackup {
        original_present: receipt.original_present,
        original_bytes: payload,
        original_mode: receipt.original_mode,
        installed_sha256: receipt.installed_sha256,
    })
}

fn effective_uid() -> Result<u32, CargoConfigError> {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::geteuid() };
        if uid == 0 {
            return Err(CargoConfigError::UnsafeFilesystemEntry);
        }
        Ok(uid)
    }
    #[cfg(not(unix))]
    {
        Err(CargoConfigError::UnsafeFilesystemEntry)
    }
}

fn validate_cargo_home(path: &Path, owner: u32) -> Result<(), CargoConfigError> {
    if !path.is_absolute() {
        return Err(CargoConfigError::InvalidCargoHome);
    }
    validate_owned_directory(path, owner)
}

fn validate_owned_directory(path: &Path, owner: u32) -> Result<(), CargoConfigError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => CargoConfigError::InvalidCargoHome,
        _ => CargoConfigError::Io,
    })?;
    if !metadata.file_type().is_dir() || !owned_by(&metadata, owner) {
        return Err(CargoConfigError::UnsafeFilesystemEntry);
    }
    Ok(())
}

fn reject_legacy_config(cargo_home: &Path) -> Result<(), CargoConfigError> {
    match fs::symlink_metadata(cargo_home.join(LEGACY_CONFIG_FILE_NAME)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(CargoConfigError::LegacyConfigConflict),
        Err(_) => Err(CargoConfigError::Io),
    }
}

fn read_existing_config(
    path: &Path,
    owner: u32,
) -> Result<Option<ExistingCargoConfig>, CargoConfigError> {
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(CargoConfigError::Io),
    };
    if !before.file_type().is_file() || !owned_by(&before, owner) {
        return Err(CargoConfigError::UnsafeFilesystemEntry);
    }
    if before.len() > MAXIMUM_CONFIG_BYTES {
        return Err(CargoConfigError::ConfigTooLarge);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(path)
        .map_err(|_| CargoConfigError::UnsafeFilesystemEntry)?;
    let opened_before_read = file.metadata().map_err(|_| CargoConfigError::Io)?;
    if !opened_before_read.file_type().is_file()
        || !owned_by(&opened_before_read, owner)
        || !same_snapshot(&before, &opened_before_read)
    {
        return Err(CargoConfigError::ConcurrentChange);
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    Read::by_ref(&mut file)
        .take(MAXIMUM_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CargoConfigError::Io)?;
    if bytes.len() as u64 > MAXIMUM_CONFIG_BYTES {
        return Err(CargoConfigError::ConfigTooLarge);
    }
    let opened_after_read = file
        .metadata()
        .map_err(|_| CargoConfigError::ConcurrentChange)?;
    let current = fs::symlink_metadata(path).map_err(|_| CargoConfigError::ConcurrentChange)?;
    if !same_snapshot(&opened_before_read, &opened_after_read)
        || !same_snapshot(&opened_after_read, &current)
    {
        return Err(CargoConfigError::ConcurrentChange);
    }
    Ok(Some(ExistingCargoConfig {
        bytes,
        metadata: opened_after_read,
        mode: file_mode(&before),
    }))
}

fn validate_serialized_config(bytes: &[u8]) -> Result<(), CargoConfigError> {
    if bytes.len() as u64 > MAXIMUM_CONFIG_BYTES {
        return Err(CargoConfigError::ConfigTooLarge);
    }
    let source = std::str::from_utf8(bytes).map_err(|_| CargoConfigError::InvalidConfig)?;
    source
        .parse::<DocumentMut>()
        .map(|_| ())
        .map_err(|_| CargoConfigError::InvalidConfig)
}

fn create_recovery_backup(
    cargo_home: &Path,
    original: Option<&ExistingCargoConfig>,
    installed_bytes: &[u8],
    owner: u32,
) -> Result<(), CargoConfigError> {
    let backup_directory = cargo_home.join(BACKUP_DIRECTORY_NAME);
    match fs::create_dir(&backup_directory) {
        Ok(()) => set_mode(&backup_directory, 0o700)?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(CargoConfigError::BackupConflict)
        }
        Err(_) => return Err(CargoConfigError::Io),
    }
    validate_owned_directory(&backup_directory, owner)?;
    let original_bytes = original
        .map(|config| config.bytes.as_slice())
        .unwrap_or_default();
    create_synced_file(
        &backup_directory.join(BACKUP_PAYLOAD_NAME),
        original_bytes,
        0o600,
    )?;
    let receipt = CargoConfigBackupReceipt {
        contract_version: BACKUP_CONTRACT_VERSION.to_string(),
        original_present: original.is_some(),
        original_sha256: lowercase_hex(&sha256(original_bytes)),
        installed_sha256: lowercase_hex(&sha256(installed_bytes)),
        original_mode: original.map(|config| config.mode),
    };
    let receipt_bytes = serde_json::to_vec_pretty(&receipt).map_err(|_| CargoConfigError::Io)?;
    create_synced_file(
        &backup_directory.join(BACKUP_RECEIPT_NAME),
        &receipt_bytes,
        0o600,
    )?;
    File::open(&backup_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| CargoConfigError::Io)?;
    File::open(cargo_home)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| CargoConfigError::Io)
}

fn create_synced_file(path: &Path, bytes: &[u8], mode: u32) -> Result<(), CargoConfigError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    let mut file = options.open(path).map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => CargoConfigError::BackupConflict,
        _ => CargoConfigError::Io,
    })?;
    set_mode(path, mode)?;
    file.write_all(bytes).map_err(|_| CargoConfigError::Io)?;
    file.flush().map_err(|_| CargoConfigError::Io)?;
    file.sync_all().map_err(|_| CargoConfigError::Io)
}

fn write_config_atomically(
    cargo_home: &Path,
    destination: &Path,
    original: Option<&ExistingCargoConfig>,
    bytes: &[u8],
    owner: u32,
) -> Result<(), CargoConfigError> {
    let temporary = unique_temporary_path(cargo_home)?;
    let mode = original.map(|config| config.mode).unwrap_or(0o600);
    let result = (|| {
        create_synced_file(&temporary, bytes, mode)?;
        match original {
            Some(original) => {
                let current = read_existing_config(destination, owner)?
                    .ok_or(CargoConfigError::ConcurrentChange)?;
                if !same_snapshot(&original.metadata, &current.metadata)
                    || current.mode != original.mode
                    || current.bytes != original.bytes
                {
                    return Err(CargoConfigError::ConcurrentChange);
                }
                fs::rename(&temporary, destination).map_err(|_| CargoConfigError::Io)?;
            }
            None => {
                fs::hard_link(&temporary, destination).map_err(|error| match error.kind() {
                    io::ErrorKind::AlreadyExists => CargoConfigError::ConcurrentChange,
                    _ => CargoConfigError::Io,
                })?;
            }
        }
        File::open(cargo_home)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| CargoConfigError::Io)
    })();
    let _ = fs::remove_file(&temporary);
    if result.is_ok() {
        File::open(cargo_home)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| CargoConfigError::Io)?;
    }
    result
}

fn unique_temporary_path(cargo_home: &Path) -> Result<PathBuf, CargoConfigError> {
    for _ in 0..64 {
        let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = cargo_home.join(format!(
            ".config.toml.antdelirium-tmp-{}-{sequence}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(CargoConfigError::Io)
}

fn read_owned_regular_file(
    path: &Path,
    owner: u32,
    limit: u64,
) -> Result<Vec<u8>, CargoConfigError> {
    let before = fs::symlink_metadata(path).map_err(|_| CargoConfigError::InvalidConfig)?;
    if !before.file_type().is_file() || !owned_by(&before, owner) || before.len() > limit {
        return Err(CargoConfigError::InvalidConfig);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(path)
        .map_err(|_| CargoConfigError::InvalidConfig)?;
    let opened_before_read = file
        .metadata()
        .map_err(|_| CargoConfigError::InvalidConfig)?;
    if !same_snapshot(&before, &opened_before_read) || file_mode(&opened_before_read) != 0o600 {
        return Err(CargoConfigError::InvalidConfig);
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CargoConfigError::Io)?;
    if bytes.len() as u64 > limit {
        return Err(CargoConfigError::InvalidConfig);
    }
    let opened_after_read = file
        .metadata()
        .map_err(|_| CargoConfigError::InvalidConfig)?;
    let current = fs::symlink_metadata(path).map_err(|_| CargoConfigError::InvalidConfig)?;
    if !same_snapshot(&opened_before_read, &opened_after_read)
        || !same_snapshot(&opened_after_read, &current)
    {
        return Err(CargoConfigError::InvalidConfig);
    }
    Ok(bytes)
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(unix)]
fn owned_by(metadata: &fs::Metadata, owner: u32) -> bool {
    metadata.uid() == owner
}

#[cfg(not(unix))]
fn owned_by(_metadata: &fs::Metadata, _owner: u32) -> bool {
    false
}

#[cfg(unix)]
fn same_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(unix)]
fn same_snapshot(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    same_identity(left, right)
        && left.len() == right.len()
        && left.mode() == right.mode()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

#[cfg(not(unix))]
fn same_snapshot(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(not(unix))]
fn same_identity(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn file_mode(_metadata: &fs::Metadata) -> u32 {
    0
}

fn directory_mode(path: &Path) -> Result<u32, CargoConfigError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| CargoConfigError::InvalidConfig)?;
    if !metadata.file_type().is_dir() {
        return Err(CargoConfigError::InvalidConfig);
    }
    Ok(file_mode(&metadata))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), CargoConfigError> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|_| CargoConfigError::Io)
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<(), CargoConfigError> {
    Err(CargoConfigError::UnsafeFilesystemEntry)
}

#[cfg(test)]
#[path = "tests/Cargo_Config_test.rs"]
mod cargo_config_tests;
