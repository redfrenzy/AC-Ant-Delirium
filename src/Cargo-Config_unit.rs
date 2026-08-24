use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use serde::{Deserialize, Serialize};
use toml_edit::{value, DocumentMut, InlineTable, Item, Value};

const CONFIG_FILE_NAME: &str = "config.toml";
const LEGACY_CONFIG_FILE_NAME: &str = "config";
const BACKUP_DIRECTORY_NAME: &str = ".antdelirium-install";
const BACKUP_PAYLOAD_NAME: &str = "config.toml.original";
const BACKUP_RECEIPT_NAME: &str = "config-backup-v1.json";
const BACKUP_CONTRACT_VERSION: &str = "cargo-config-backup-v1";
const ROLLBACK_MARKER_NAME: &str = "rollback-complete-v1.json";
const ROLLBACK_CONTRACT_VERSION: &str = "cargo-config-rollback-v1";
const INSTALLED_BINARY_NAME: &str = "antdelirium";
pub(crate) const LIMITED_SOCKET_ENV: &str = "ANTDELIRIUM_LIMITED_SOCKET";
const MAXIMUM_CONFIG_BYTES: u64 = 4 * 1024 * 1024;
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CargoConfigMergeOutcome {
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CargoConfigRollbackOutcome {
    Restored,
    AlreadyRestored,
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
    RollbackConflict,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoConfigRollbackMarker {
    contract_version: String,
    original_present: bool,
    original_sha256: String,
    installed_sha256: String,
    original_mode: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackupDirectoryEntries {
    payload: bool,
    receipt: bool,
    marker: bool,
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
    limited_endpoint: &Path,
) -> Result<CargoConfigMergeOutcome, CargoConfigError> {
    let wrapper = expected_workspace_wrapper(cargo_home)?;
    let limited_endpoint = validate_limited_endpoint(limited_endpoint)?;
    merge_cargo_config(cargo_home, move |document| {
        configure_cargo_integration_document(document, &wrapper, &limited_endpoint)
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
    match configured_limited_endpoint_document(&document) {
        Ok(Some(_)) => {}
        Ok(None) => return Ok(WorkspaceWrapperStatus::Missing),
        Err(_) => return Ok(WorkspaceWrapperStatus::Conflict),
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

pub(crate) fn preflight_workspace_wrapper(
    cargo_home: &Path,
    limited_endpoint: &Path,
) -> Result<(), CargoConfigError> {
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
    configure_cargo_integration_document(
        &mut document,
        expected_workspace_wrapper(cargo_home)?.as_str(),
        &validate_limited_endpoint(limited_endpoint)?,
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

pub(crate) fn rollback_owned_workspace_wrapper(
    cargo_home: &Path,
) -> Result<CargoConfigRollbackOutcome, CargoConfigError> {
    let owner = effective_uid()?;
    validate_cargo_home(cargo_home, owner)?;
    reject_legacy_config(cargo_home)?;
    let backup_directory = cargo_home.join(BACKUP_DIRECTORY_NAME);
    validate_owned_directory(&backup_directory, owner)?;
    if directory_mode(&backup_directory)? != 0o700 {
        return Err(CargoConfigError::InvalidConfig);
    }

    let rollback_started = match fs::symlink_metadata(backup_directory.join(ROLLBACK_MARKER_NAME)) {
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(_) => return Err(CargoConfigError::Io),
    };
    if rollback_started {
        let entries = validate_backup_entries(&backup_directory, true)?;
        let marker = read_rollback_marker(&backup_directory, owner)?;
        if entries.payload && entries.receipt {
            let backup = read_verified_backup_files(&backup_directory, owner)?;
            if marker != CargoConfigRollbackMarker::from(&backup) {
                return Err(CargoConfigError::InvalidConfig);
            }
        }
        ensure_original_config_state(cargo_home, owner, &marker)?;
        return Ok(CargoConfigRollbackOutcome::AlreadyRestored);
    }

    validate_backup_entries(&backup_directory, false)?;
    let backup = read_verified_config_backup(cargo_home)?;
    let marker = CargoConfigRollbackMarker::from(&backup);
    let config_path = cargo_home.join(CONFIG_FILE_NAME);
    let current = read_existing_config(&config_path, owner)?;
    let expected_installed_mode = backup.original_mode.unwrap_or(0o600);
    let installed_matches = current.as_ref().is_some_and(|config| {
        config.mode == expected_installed_mode
            && lowercase_hex(&sha256(&config.bytes)) == backup.installed_sha256
    });
    let original_matches = config_matches_original(current.as_ref(), &marker);

    let outcome = if installed_matches {
        match (backup.original_present, current.as_ref()) {
            (true, Some(installed)) => write_config_atomically_with_mode(
                cargo_home,
                &config_path,
                Some(installed),
                &backup.original_bytes,
                backup
                    .original_mode
                    .ok_or(CargoConfigError::InvalidConfig)?,
                owner,
            )?,
            (false, Some(installed)) => {
                remove_config_if_unchanged(cargo_home, &config_path, installed, owner)?
            }
            _ => return Err(CargoConfigError::RollbackConflict),
        }
        CargoConfigRollbackOutcome::Restored
    } else if original_matches {
        CargoConfigRollbackOutcome::AlreadyRestored
    } else {
        return Err(CargoConfigError::RollbackConflict);
    };

    ensure_original_config_state(cargo_home, owner, &marker)?;
    create_rollback_marker(&backup_directory, &marker)?;
    Ok(outcome)
}

pub(crate) fn finalize_owned_workspace_wrapper_rollback(
    cargo_home: &Path,
) -> Result<(), CargoConfigError> {
    let owner = effective_uid()?;
    validate_cargo_home(cargo_home, owner)?;
    let backup_directory = cargo_home.join(BACKUP_DIRECTORY_NAME);
    validate_owned_directory(&backup_directory, owner)?;
    if directory_mode(&backup_directory)? != 0o700 {
        return Err(CargoConfigError::InvalidConfig);
    }
    validate_backup_entries(&backup_directory, true)?;
    let marker = read_rollback_marker(&backup_directory, owner)?;
    ensure_original_config_state(cargo_home, owner, &marker)?;

    remove_known_backup_file(&backup_directory, BACKUP_PAYLOAD_NAME, owner)?;
    remove_known_backup_file(&backup_directory, BACKUP_RECEIPT_NAME, owner)?;
    remove_known_backup_file(&backup_directory, ROLLBACK_MARKER_NAME, owner)?;
    fs::remove_dir(&backup_directory).map_err(|_| CargoConfigError::Io)?;
    sync_directory(cargo_home)
}

impl From<&VerifiedCargoConfigBackup> for CargoConfigRollbackMarker {
    fn from(backup: &VerifiedCargoConfigBackup) -> Self {
        Self {
            contract_version: ROLLBACK_CONTRACT_VERSION.to_string(),
            original_present: backup.original_present,
            original_sha256: lowercase_hex(&sha256(&backup.original_bytes)),
            installed_sha256: backup.installed_sha256.clone(),
            original_mode: backup.original_mode,
        }
    }
}

fn expected_workspace_wrapper(cargo_home: &Path) -> Result<String, CargoConfigError> {
    cargo_home
        .join("bin")
        .join(INSTALLED_BINARY_NAME)
        .to_str()
        .map(str::to_owned)
        .ok_or(CargoConfigError::InvalidCargoHome)
}

fn configure_cargo_integration_document(
    document: &mut DocumentMut,
    expected_wrapper: &str,
    limited_endpoint: &str,
) -> Result<(), CargoConfigError> {
    let build = document.entry("build").or_insert(toml_edit::table());
    let build = build
        .as_table_like_mut()
        .ok_or(CargoConfigError::WrapperConflict)?;
    if build.get("rustc-wrapper").is_some() {
        return Err(CargoConfigError::WrapperConflict);
    }
    match build.get("rustc-workspace-wrapper") {
        Some(configured) if configured.as_str() == Some(expected_wrapper) => {}
        Some(_) => return Err(CargoConfigError::WrapperConflict),
        None => {
            build.insert("rustc-workspace-wrapper", value(expected_wrapper));
        }
    }

    let environment = document.entry("env").or_insert(toml_edit::table());
    let environment = environment
        .as_table_like_mut()
        .ok_or(CargoConfigError::WrapperConflict)?;
    match environment.get(LIMITED_SOCKET_ENV) {
        Some(configured) if limited_endpoint_item_matches(configured, limited_endpoint) => Ok(()),
        Some(_) => Err(CargoConfigError::WrapperConflict),
        None => {
            let mut setting = InlineTable::new();
            setting.insert("value", Value::from(limited_endpoint));
            setting.insert("force", Value::from(true));
            environment.insert(LIMITED_SOCKET_ENV, Item::Value(Value::InlineTable(setting)));
            Ok(())
        }
    }
}

pub(crate) fn configured_limited_endpoint(
    cargo_home: &Path,
) -> Result<Option<PathBuf>, CargoConfigError> {
    let owner = effective_uid()?;
    validate_cargo_home(cargo_home, owner)?;
    reject_legacy_config(cargo_home)?;
    let Some(config) = read_existing_config(&cargo_home.join(CONFIG_FILE_NAME), owner)? else {
        return Ok(None);
    };
    let source = std::str::from_utf8(&config.bytes).map_err(|_| CargoConfigError::InvalidConfig)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|_| CargoConfigError::InvalidConfig)?;
    configured_limited_endpoint_document(&document)
}

fn configured_limited_endpoint_document(
    document: &DocumentMut,
) -> Result<Option<PathBuf>, CargoConfigError> {
    let Some(environment) = document.get("env") else {
        return Ok(None);
    };
    let environment = environment
        .as_table_like()
        .ok_or(CargoConfigError::InvalidConfig)?;
    let Some(item) = environment.get(LIMITED_SOCKET_ENV) else {
        return Ok(None);
    };
    let table = item
        .as_inline_table()
        .ok_or(CargoConfigError::InvalidConfig)?;
    if table.len() != 2 || table.get("force").and_then(Value::as_bool) != Some(true) {
        return Err(CargoConfigError::InvalidConfig);
    }
    let value = table
        .get("value")
        .and_then(Value::as_str)
        .ok_or(CargoConfigError::InvalidConfig)?;
    let endpoint = PathBuf::from(value);
    validate_limited_endpoint(&endpoint)?;
    Ok(Some(endpoint))
}

fn limited_endpoint_item_matches(item: &Item, expected: &str) -> bool {
    let Some(table) = item.as_inline_table() else {
        return false;
    };
    table.len() == 2
        && table.get("value").and_then(Value::as_str) == Some(expected)
        && table.get("force").and_then(Value::as_bool) == Some(true)
}

fn validate_limited_endpoint(endpoint: &Path) -> Result<String, CargoConfigError> {
    let value = endpoint
        .to_str()
        .filter(|value| !value.is_empty())
        .ok_or(CargoConfigError::InvalidConfig)?;
    if !endpoint.is_absolute()
        || value
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_control())
        || value
            .split('/')
            .skip(1)
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(CargoConfigError::InvalidConfig);
    }
    Ok(value.to_owned())
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
    write_config_atomically_with_mode(
        cargo_home,
        &config_path,
        original.as_ref(),
        &installed_bytes,
        original.as_ref().map(|config| config.mode).unwrap_or(0o600),
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
    validate_backup_entries(&backup_directory, false)?;
    read_verified_backup_files(&backup_directory, owner)
}

fn read_verified_backup_files(
    backup_directory: &Path,
    owner: u32,
) -> Result<VerifiedCargoConfigBackup, CargoConfigError> {
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

fn create_rollback_marker(
    backup_directory: &Path,
    marker: &CargoConfigRollbackMarker,
) -> Result<(), CargoConfigError> {
    let bytes = serde_json::to_vec_pretty(marker).map_err(|_| CargoConfigError::Io)?;
    create_synced_file(&backup_directory.join(ROLLBACK_MARKER_NAME), &bytes, 0o600)?;
    sync_directory(backup_directory)
}

fn read_rollback_marker(
    backup_directory: &Path,
    owner: u32,
) -> Result<CargoConfigRollbackMarker, CargoConfigError> {
    let bytes = read_owned_regular_file(
        &backup_directory.join(ROLLBACK_MARKER_NAME),
        owner,
        16 * 1024,
    )?;
    let marker: CargoConfigRollbackMarker =
        serde_json::from_slice(&bytes).map_err(|_| CargoConfigError::InvalidConfig)?;
    if marker.contract_version != ROLLBACK_CONTRACT_VERSION
        || !is_lowercase_sha256(&marker.original_sha256)
        || !is_lowercase_sha256(&marker.installed_sha256)
        || marker.original_present != marker.original_mode.is_some()
        || marker.original_mode.is_some_and(|mode| mode > 0o777)
    {
        return Err(CargoConfigError::InvalidConfig);
    }
    Ok(marker)
}

fn validate_backup_entries(
    backup_directory: &Path,
    rollback_started: bool,
) -> Result<BackupDirectoryEntries, CargoConfigError> {
    let mut payload = false;
    let mut receipt = false;
    let mut marker = false;
    for entry in fs::read_dir(backup_directory).map_err(|_| CargoConfigError::Io)? {
        let entry = entry.map_err(|_| CargoConfigError::Io)?;
        match entry.file_name().to_str() {
            Some(BACKUP_PAYLOAD_NAME) if !payload => payload = true,
            Some(BACKUP_RECEIPT_NAME) if !receipt => receipt = true,
            Some(ROLLBACK_MARKER_NAME) if !marker => marker = true,
            _ => return Err(CargoConfigError::InvalidConfig),
        }
    }
    if rollback_started {
        if !marker {
            return Err(CargoConfigError::InvalidConfig);
        }
    } else if !payload || !receipt || marker {
        return Err(CargoConfigError::InvalidConfig);
    }
    Ok(BackupDirectoryEntries {
        payload,
        receipt,
        marker,
    })
}

fn ensure_original_config_state(
    cargo_home: &Path,
    owner: u32,
    marker: &CargoConfigRollbackMarker,
) -> Result<(), CargoConfigError> {
    let current = read_existing_config(&cargo_home.join(CONFIG_FILE_NAME), owner)?;
    if config_matches_original(current.as_ref(), marker) {
        Ok(())
    } else {
        Err(CargoConfigError::RollbackConflict)
    }
}

fn config_matches_original(
    current: Option<&ExistingCargoConfig>,
    marker: &CargoConfigRollbackMarker,
) -> bool {
    match (marker.original_present, current) {
        (false, None) => true,
        (true, Some(config)) => {
            Some(config.mode) == marker.original_mode
                && lowercase_hex(&sha256(&config.bytes)) == marker.original_sha256
        }
        _ => false,
    }
}

fn remove_config_if_unchanged(
    cargo_home: &Path,
    config_path: &Path,
    expected: &ExistingCargoConfig,
    owner: u32,
) -> Result<(), CargoConfigError> {
    let current =
        read_existing_config(config_path, owner)?.ok_or(CargoConfigError::ConcurrentChange)?;
    if !same_snapshot(&expected.metadata, &current.metadata)
        || current.mode != expected.mode
        || current.bytes != expected.bytes
    {
        return Err(CargoConfigError::ConcurrentChange);
    }
    fs::remove_file(config_path).map_err(|_| CargoConfigError::Io)?;
    sync_directory(cargo_home)
}

fn remove_known_backup_file(
    backup_directory: &Path,
    name: &str,
    owner: u32,
) -> Result<(), CargoConfigError> {
    let path = backup_directory.join(name);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(CargoConfigError::Io),
        Ok(metadata)
            if metadata.file_type().is_file()
                && owned_by(&metadata, owner)
                && file_mode(&metadata) == 0o600 => {}
        Ok(_) => return Err(CargoConfigError::InvalidConfig),
    }
    fs::remove_file(path).map_err(|_| CargoConfigError::Io)?;
    sync_directory(backup_directory)
}

fn sync_directory(path: &Path) -> Result<(), CargoConfigError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| CargoConfigError::Io)
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
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

fn write_config_atomically_with_mode(
    cargo_home: &Path,
    destination: &Path,
    original: Option<&ExistingCargoConfig>,
    bytes: &[u8],
    mode: u32,
    owner: u32,
) -> Result<(), CargoConfigError> {
    let temporary = unique_temporary_path(cargo_home)?;
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
