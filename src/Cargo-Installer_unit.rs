use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use crate::cargo_config::{
    configure_workspace_wrapper, finalize_owned_workspace_wrapper_rollback,
    preflight_workspace_wrapper, rollback_owned_workspace_wrapper, CargoConfigError,
    CargoConfigMergeOutcome, CargoConfigRollbackOutcome,
};
use crate::public_output::{OutputRouter, PublicError};

const INSTALLED_BINARY_NAME: &str = "antdelirium";
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryInstallOutcome {
    Installed,
    AlreadyInstalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryUninstallOutcome {
    Removed,
    AlreadyAbsent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryInstallationStatus {
    Ready,
    Missing,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryInstallError {
    InvalidSource,
    InvalidCargoHome,
    UnsafeFilesystemEntry,
    ExistingInstallationConflict,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CargoInstallOutcome {
    pub(crate) binary: BinaryInstallOutcome,
    pub(crate) config: CargoConfigMergeOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CargoUninstallOutcome {
    pub(crate) binary: BinaryUninstallOutcome,
    pub(crate) config: CargoConfigRollbackOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CargoInstallError {
    Binary(BinaryInstallError),
    Config(CargoConfigError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CargoUninstallError {
    Binary(BinaryInstallError),
    Config(CargoConfigError),
}

pub(crate) fn run_operator_mode(args: &[OsString]) {
    if args
        .first()
        .is_some_and(|arg| arg == OsStr::new("migrate-plugin"))
    {
        crate::administrative_protocol::run_migration_stage_operator(args);
        return;
    }
    if args.len() == 1 && args[0] == OsStr::new("doctor") {
        crate::cargo_doctor::run_doctor();
        return;
    }
    if args.len() != 1 || (args[0] != OsStr::new("install") && args[0] != OsStr::new("uninstall")) {
        OutputRouter::exit(PublicError::Authorization);
    }
    if cfg!(debug_assertions) {
        OutputRouter::exit(PublicError::Configuration);
    }

    if args[0] == OsStr::new("uninstall") {
        let outcome = uninstall_current_release().unwrap_or_else(|error| match error {
            CargoUninstallError::Binary(BinaryInstallError::Io)
            | CargoUninstallError::Config(CargoConfigError::Io) => {
                OutputRouter::exit(PublicError::Internal)
            }
            CargoUninstallError::Binary(_) | CargoUninstallError::Config(_) => {
                OutputRouter::exit(PublicError::Configuration)
            }
        });
        let message = match (outcome.binary, outcome.config) {
            (BinaryUninstallOutcome::Removed, CargoConfigRollbackOutcome::Restored) => {
                "antdelirium[AD-UNINSTALL]: integração Cargo revertida; binário removido"
            }
            (BinaryUninstallOutcome::Removed, CargoConfigRollbackOutcome::AlreadyRestored) => {
                "antdelirium[AD-UNINSTALL]: integração Cargo já revertida; binário removido"
            }
            (BinaryUninstallOutcome::AlreadyAbsent, CargoConfigRollbackOutcome::Restored) => {
                "antdelirium[AD-UNINSTALL]: integração Cargo revertida; binário já estava ausente"
            }
            (
                BinaryUninstallOutcome::AlreadyAbsent,
                CargoConfigRollbackOutcome::AlreadyRestored,
            ) => "antdelirium[AD-UNINSTALL]: desinstalação recuperada",
        };
        OutputRouter::write_success(message);
        return;
    }

    let outcome = install_current_release().unwrap_or_else(|error| match error {
        CargoInstallError::Binary(BinaryInstallError::Io)
        | CargoInstallError::Config(CargoConfigError::Io) => {
            OutputRouter::exit(PublicError::Internal)
        }
        CargoInstallError::Binary(_) | CargoInstallError::Config(_) => {
            OutputRouter::exit(PublicError::Configuration)
        }
    });
    let message = match (outcome.binary, outcome.config) {
        (BinaryInstallOutcome::Installed, CargoConfigMergeOutcome::Updated) => {
            "antdelirium[AD-INSTALL]: binário instalado; integração Cargo ativa"
        }
        (BinaryInstallOutcome::Installed, CargoConfigMergeOutcome::Unchanged) => {
            "antdelirium[AD-INSTALL]: binário instalado; integração Cargo já estava ativa"
        }
        (BinaryInstallOutcome::AlreadyInstalled, CargoConfigMergeOutcome::Updated) => {
            "antdelirium[AD-INSTALL]: binário já instalado; integração Cargo ativada"
        }
        (BinaryInstallOutcome::AlreadyInstalled, CargoConfigMergeOutcome::Unchanged) => {
            "antdelirium[AD-INSTALL]: binário já instalado; integração Cargo ativa"
        }
    };
    OutputRouter::write_success(message);
}

fn uninstall_current_release() -> Result<CargoUninstallOutcome, CargoUninstallError> {
    let source = std::env::current_exe()
        .map_err(|_| CargoUninstallError::Binary(BinaryInstallError::InvalidSource))?;
    let cargo_home = cargo_home_from_environment().map_err(CargoUninstallError::Binary)?;
    uninstall_release_and_configure(&source, &cargo_home)
}

pub(crate) fn uninstall_release_and_configure(
    source: &Path,
    cargo_home: &Path,
) -> Result<CargoUninstallOutcome, CargoUninstallError> {
    let binary_status =
        inspect_release_binary(source, cargo_home).map_err(CargoUninstallError::Binary)?;
    if binary_status == BinaryInstallationStatus::Conflict {
        return Err(CargoUninstallError::Binary(
            BinaryInstallError::ExistingInstallationConflict,
        ));
    }

    let config =
        rollback_owned_workspace_wrapper(cargo_home).map_err(CargoUninstallError::Config)?;
    let binary = match binary_status {
        BinaryInstallationStatus::Ready => {
            remove_owned_release_binary(source, cargo_home).map_err(CargoUninstallError::Binary)?
        }
        BinaryInstallationStatus::Missing => BinaryUninstallOutcome::AlreadyAbsent,
        BinaryInstallationStatus::Conflict => unreachable!("conflict returned before rollback"),
    };
    finalize_owned_workspace_wrapper_rollback(cargo_home).map_err(CargoUninstallError::Config)?;
    Ok(CargoUninstallOutcome { binary, config })
}

fn install_current_release() -> Result<CargoInstallOutcome, CargoInstallError> {
    let source = std::env::current_exe()
        .map_err(|_| CargoInstallError::Binary(BinaryInstallError::InvalidSource))?;
    let cargo_home = cargo_home_from_environment().map_err(CargoInstallError::Binary)?;
    install_release_and_configure(&source, &cargo_home)
}

pub(crate) fn install_release_and_configure(
    source: &Path,
    cargo_home: &Path,
) -> Result<CargoInstallOutcome, CargoInstallError> {
    match fs::symlink_metadata(cargo_home) {
        Ok(_) => preflight_workspace_wrapper(cargo_home).map_err(CargoInstallError::Config)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(CargoInstallError::Config(CargoConfigError::Io)),
    }
    let binary = install_release_binary(source, cargo_home).map_err(CargoInstallError::Binary)?;
    let config = configure_workspace_wrapper(cargo_home).map_err(CargoInstallError::Config)?;
    Ok(CargoInstallOutcome { binary, config })
}

pub(crate) fn cargo_home_from_environment() -> Result<PathBuf, BinaryInstallError> {
    let configured = std::env::var_os("CARGO_HOME").filter(|value| !value.is_empty());
    let cargo_home = match configured {
        Some(path) => PathBuf::from(path),
        None => {
            let home = std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .ok_or(BinaryInstallError::InvalidCargoHome)?;
            PathBuf::from(home).join(".cargo")
        }
    };
    if !cargo_home.is_absolute() {
        return Err(BinaryInstallError::InvalidCargoHome);
    }
    Ok(cargo_home)
}

pub(crate) fn inspect_release_binary(
    source: &Path,
    cargo_home: &Path,
) -> Result<BinaryInstallationStatus, BinaryInstallError> {
    if !source.is_absolute() {
        return Err(BinaryInstallError::InvalidSource);
    }
    if !cargo_home.is_absolute() {
        return Err(BinaryInstallError::InvalidCargoHome);
    }
    let owner = effective_uid()?;
    match fs::symlink_metadata(cargo_home) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(BinaryInstallationStatus::Missing)
        }
        Err(_) => return Err(BinaryInstallError::Io),
        Ok(_) => validate_owned_directory(cargo_home, owner)?,
    }
    let bin_directory = cargo_home.join("bin");
    match fs::symlink_metadata(&bin_directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(BinaryInstallationStatus::Missing)
        }
        Err(_) => return Err(BinaryInstallError::Io),
        Ok(_) => validate_owned_directory(&bin_directory, owner)?,
    }
    let source_file = open_owned_executable(source, owner)?;
    let source_metadata = source_file
        .metadata()
        .map_err(|_| BinaryInstallError::InvalidSource)?;
    match inspect_existing_destination(
        &bin_directory.join(INSTALLED_BINARY_NAME),
        owner,
        &source_file,
        &source_metadata,
    ) {
        Ok(Some(BinaryInstallOutcome::AlreadyInstalled)) => Ok(BinaryInstallationStatus::Ready),
        Ok(Some(BinaryInstallOutcome::Installed)) => Ok(BinaryInstallationStatus::Conflict),
        Ok(None) => Ok(BinaryInstallationStatus::Missing),
        Err(BinaryInstallError::ExistingInstallationConflict) => {
            Ok(BinaryInstallationStatus::Conflict)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn install_release_binary(
    source: &Path,
    cargo_home: &Path,
) -> Result<BinaryInstallOutcome, BinaryInstallError> {
    if !source.is_absolute() {
        return Err(BinaryInstallError::InvalidSource);
    }
    if !cargo_home.is_absolute() {
        return Err(BinaryInstallError::InvalidCargoHome);
    }
    let owner = effective_uid()?;
    ensure_owned_cargo_home(cargo_home, owner)?;
    let bin_directory = cargo_home.join("bin");
    ensure_owned_bin_directory(&bin_directory, owner)?;

    let source_file = open_owned_executable(source, owner)?;
    let source_metadata = source_file
        .metadata()
        .map_err(|_| BinaryInstallError::InvalidSource)?;
    let destination = bin_directory.join(INSTALLED_BINARY_NAME);
    if let Some(outcome) =
        inspect_existing_destination(&destination, owner, &source_file, &source_metadata)?
    {
        return Ok(outcome);
    }

    install_without_overwrite(source_file, &destination, &bin_directory)?;
    Ok(BinaryInstallOutcome::Installed)
}

pub(crate) fn remove_owned_release_binary(
    source: &Path,
    cargo_home: &Path,
) -> Result<BinaryUninstallOutcome, BinaryInstallError> {
    match inspect_release_binary(source, cargo_home)? {
        BinaryInstallationStatus::Missing => return Ok(BinaryUninstallOutcome::AlreadyAbsent),
        BinaryInstallationStatus::Conflict => {
            return Err(BinaryInstallError::ExistingInstallationConflict)
        }
        BinaryInstallationStatus::Ready => {}
    }

    let owner = effective_uid()?;
    let bin_directory = cargo_home.join("bin");
    validate_owned_directory(cargo_home, owner)?;
    validate_owned_directory(&bin_directory, owner)?;
    let source_file = open_owned_executable(source, owner)?;
    let destination = bin_directory.join(INSTALLED_BINARY_NAME);
    let before = fs::symlink_metadata(&destination)
        .map_err(|_| BinaryInstallError::ExistingInstallationConflict)?;
    if !before.file_type().is_file() || !owned_by(&before, owner) || !has_installed_mode(&before) {
        return Err(BinaryInstallError::ExistingInstallationConflict);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let installed = options
        .open(&destination)
        .map_err(|_| BinaryInstallError::ExistingInstallationConflict)?;
    let opened = installed
        .metadata()
        .map_err(|_| BinaryInstallError::ExistingInstallationConflict)?;
    if !same_snapshot(&before, &opened)
        || source_file
            .metadata()
            .map_err(|_| BinaryInstallError::InvalidSource)?
            .len()
            != opened.len()
        || !files_have_equal_bytes(&source_file, &installed)?
    {
        return Err(BinaryInstallError::ExistingInstallationConflict);
    }
    let current = fs::symlink_metadata(&destination)
        .map_err(|_| BinaryInstallError::ExistingInstallationConflict)?;
    if !same_snapshot(&opened, &current) {
        return Err(BinaryInstallError::ExistingInstallationConflict);
    }
    fs::remove_file(&destination).map_err(|_| BinaryInstallError::Io)?;
    File::open(&bin_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| BinaryInstallError::Io)?;
    Ok(BinaryUninstallOutcome::Removed)
}

fn effective_uid() -> Result<u32, BinaryInstallError> {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::geteuid() };
        if uid == 0 {
            return Err(BinaryInstallError::UnsafeFilesystemEntry);
        }
        Ok(uid)
    }
    #[cfg(not(unix))]
    {
        Err(BinaryInstallError::UnsafeFilesystemEntry)
    }
}

fn validate_owned_directory(path: &Path, owner: u32) -> Result<(), BinaryInstallError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => BinaryInstallError::InvalidCargoHome,
        _ => BinaryInstallError::Io,
    })?;
    if !metadata.file_type().is_dir() || !owned_by(&metadata, owner) {
        return Err(BinaryInstallError::UnsafeFilesystemEntry);
    }
    Ok(())
}

fn ensure_owned_cargo_home(path: &Path, owner: u32) -> Result<(), BinaryInstallError> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_owned_directory(path, owner),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or(BinaryInstallError::InvalidCargoHome)?;
            validate_owned_directory(parent, owner)?;
            fs::create_dir(path).map_err(|_| BinaryInstallError::Io)?;
            set_directory_permissions(path)?;
            validate_owned_directory(path, owner)
        }
        Err(_) => Err(BinaryInstallError::Io),
    }
}

fn ensure_owned_bin_directory(path: &Path, owner: u32) -> Result<(), BinaryInstallError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() || !owned_by(&metadata, owner) {
                return Err(BinaryInstallError::UnsafeFilesystemEntry);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|_| BinaryInstallError::Io)?;
            set_directory_permissions(path)?;
            validate_owned_directory(path, owner)?;
        }
        Err(_) => return Err(BinaryInstallError::Io),
    }
    Ok(())
}

fn open_owned_executable(path: &Path, owner: u32) -> Result<File, BinaryInstallError> {
    let before = fs::symlink_metadata(path).map_err(|_| BinaryInstallError::InvalidSource)?;
    if !before.file_type().is_file() || !owned_by(&before, owner) || !is_executable(&before) {
        return Err(BinaryInstallError::InvalidSource);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let file = options
        .open(path)
        .map_err(|_| BinaryInstallError::InvalidSource)?;
    let opened = file
        .metadata()
        .map_err(|_| BinaryInstallError::InvalidSource)?;
    if !opened.file_type().is_file()
        || !owned_by(&opened, owner)
        || !is_executable(&opened)
        || !same_identity(&before, &opened)
    {
        return Err(BinaryInstallError::InvalidSource);
    }
    Ok(file)
}

fn inspect_existing_destination(
    destination: &Path,
    owner: u32,
    source: &File,
    source_metadata: &fs::Metadata,
) -> Result<Option<BinaryInstallOutcome>, BinaryInstallError> {
    let metadata = match fs::symlink_metadata(destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(BinaryInstallError::Io),
    };
    if !metadata.file_type().is_file()
        || !owned_by(&metadata, owner)
        || !has_installed_mode(&metadata)
    {
        return Err(BinaryInstallError::ExistingInstallationConflict);
    }
    if same_identity(source_metadata, &metadata) {
        return Ok(Some(BinaryInstallOutcome::AlreadyInstalled));
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let existing = options
        .open(destination)
        .map_err(|_| BinaryInstallError::ExistingInstallationConflict)?;
    let opened = existing
        .metadata()
        .map_err(|_| BinaryInstallError::ExistingInstallationConflict)?;
    if !same_identity(&metadata, &opened) {
        return Err(BinaryInstallError::ExistingInstallationConflict);
    }
    if source_metadata.len() == opened.len() && files_have_equal_bytes(source, &existing)? {
        return Ok(Some(BinaryInstallOutcome::AlreadyInstalled));
    }
    Err(BinaryInstallError::ExistingInstallationConflict)
}

fn files_have_equal_bytes(left: &File, right: &File) -> Result<bool, BinaryInstallError> {
    let left = left.try_clone().map_err(|_| BinaryInstallError::Io)?;
    let right = right.try_clone().map_err(|_| BinaryInstallError::Io)?;
    let mut left = BufReader::new(left);
    let mut right = BufReader::new(right);
    let mut left_buffer = [0_u8; 16 * 1024];
    let mut right_buffer = [0_u8; 16 * 1024];
    loop {
        let left_size = left
            .read(&mut left_buffer)
            .map_err(|_| BinaryInstallError::Io)?;
        let right_size = right
            .read(&mut right_buffer)
            .map_err(|_| BinaryInstallError::Io)?;
        if left_size != right_size || left_buffer[..left_size] != right_buffer[..right_size] {
            return Ok(false);
        }
        if left_size == 0 {
            return Ok(true);
        }
    }
}

fn install_without_overwrite(
    mut source: File,
    destination: &Path,
    bin_directory: &Path,
) -> Result<(), BinaryInstallError> {
    let temporary = create_temporary_path(bin_directory)?;
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_CLOEXEC);
        let mut output = options
            .open(&temporary)
            .map_err(|_| BinaryInstallError::Io)?;
        set_executable_permissions(&temporary)?;
        io::copy(&mut source, &mut output).map_err(|_| BinaryInstallError::Io)?;
        output.flush().map_err(|_| BinaryInstallError::Io)?;
        output.sync_all().map_err(|_| BinaryInstallError::Io)?;
        fs::hard_link(&temporary, destination).map_err(|error| match error.kind() {
            io::ErrorKind::AlreadyExists => BinaryInstallError::ExistingInstallationConflict,
            _ => BinaryInstallError::Io,
        })?;
        File::open(bin_directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| BinaryInstallError::Io)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    if result.is_ok() {
        File::open(bin_directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| BinaryInstallError::Io)?;
    }
    result
}

fn create_temporary_path(bin_directory: &Path) -> Result<PathBuf, BinaryInstallError> {
    for _ in 0..64 {
        let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = bin_directory.join(format!(
            ".antdelirium-install-{}-{sequence}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(BinaryInstallError::Io)
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
fn is_executable(metadata: &fs::Metadata) -> bool {
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
}

#[cfg(unix)]
fn has_installed_mode(metadata: &fs::Metadata) -> bool {
    metadata.permissions().mode() & 0o777 == 0o755
}

#[cfg(not(unix))]
fn has_installed_mode(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
}

#[cfg(unix)]
fn set_directory_permissions(path: &Path) -> Result<(), BinaryInstallError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|_| BinaryInstallError::Io)
}

#[cfg(not(unix))]
fn set_directory_permissions(_path: &Path) -> Result<(), BinaryInstallError> {
    Ok(())
}

#[cfg(unix)]
fn set_executable_permissions(path: &Path) -> Result<(), BinaryInstallError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|_| BinaryInstallError::Io)
}

#[cfg(not(unix))]
fn set_executable_permissions(_path: &Path) -> Result<(), BinaryInstallError> {
    Ok(())
}

#[cfg(test)]
#[path = "tests/Cargo_Installer_test.rs"]
mod cargo_installer_tests;
