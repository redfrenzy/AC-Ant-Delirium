use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use ac_ant_delirium::structs::{
    AuthorizedCapability, ProjectDatabase, ServiceSecurityAuditReceipt, StorageProject,
    TransactionalStorageError, STORAGE_SCHEMA_VERSION,
};

use crate::service_authority::VerifiedServiceAuthority;

const STATE_DIRECTORY_ENV: &str = "STATE_DIRECTORY";
const REGISTRY_DIR_NAME: &str = "registry";
const PROJECTS_DIR_NAME: &str = "projects";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtectedStorageError {
    InvalidRoot,
    WrongOwner,
    InvalidProjectIdentity,
    ResourceUnavailable,
    Storage,
}

impl From<TransactionalStorageError> for ProtectedStorageError {
    fn from(_error: TransactionalStorageError) -> Self {
        Self::Storage
    }
}

pub(crate) struct ProtectedServiceStorage {
    projects_dir: PathBuf,
    registry: ProjectDatabase,
}

// This service-only handle deliberately exposes neither project-key nor path.
// It can only be created from the unforgeable result of capability authorization.
#[allow(dead_code)]
pub(crate) struct ResolvedAuthorizedResource {
    database: ProjectDatabase,
    project_id: String,
    workspace_id: Option<String>,
    task_id: Option<String>,
}

#[allow(dead_code)]
impl ResolvedAuthorizedResource {
    pub(crate) fn database(&self) -> &ProjectDatabase {
        &self.database
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub(crate) fn task_id(&self) -> Option<&str> {
        self.task_id.as_deref()
    }
}

impl ProtectedServiceStorage {
    pub(crate) fn registry(&self) -> &ProjectDatabase {
        &self.registry
    }
}

pub(crate) fn initialize_configured_service_storage(
    authority: &VerifiedServiceAuthority,
) -> Result<ProtectedServiceStorage, ()> {
    configured_state_directory()
        .and_then(|root| ProtectedServiceStorage::initialize(authority, &root))
        .map_err(|_| ())
}

fn configured_state_directory() -> Result<PathBuf, ProtectedStorageError> {
    let raw = std::env::var_os(STATE_DIRECTORY_ENV).ok_or(ProtectedStorageError::InvalidRoot)?;
    parse_state_directory(&raw)
}

fn parse_state_directory(raw: &OsStr) -> Result<PathBuf, ProtectedStorageError> {
    let value = raw.to_str().ok_or(ProtectedStorageError::InvalidRoot)?;
    if value.is_empty() || value.contains(':') {
        return Err(ProtectedStorageError::InvalidRoot);
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(ProtectedStorageError::InvalidRoot);
    }
    Ok(path)
}

impl ProtectedServiceStorage {
    fn initialize(
        authority: &VerifiedServiceAuthority,
        root: &Path,
    ) -> Result<Self, ProtectedStorageError> {
        require_owned_directory(root, authority.effective_uid())?;
        let registry_dir = private_child_directory(root, REGISTRY_DIR_NAME, authority)?;
        let projects_dir = private_child_directory(root, PROJECTS_DIR_NAME, authority)?;

        let registry = ProjectDatabase::initialize_at(&registry_dir)?;
        let recovery = registry.recover()?;
        if recovery.health.schema_version != STORAGE_SCHEMA_VERSION {
            return Err(ProtectedStorageError::Storage);
        }
        require_database_ownership(&registry, authority.effective_uid())?;

        let storage = Self {
            projects_dir,
            registry,
        };
        storage.recover_registered_projects(authority)?;
        Ok(storage)
    }

    // T4.5 is the first production caller. Channel acquisition in T4.3 deliberately
    // cannot materialize protected state from client-controlled bytes.
    #[allow(dead_code)]
    fn materialize_project(
        &self,
        authority: &VerifiedServiceAuthority,
        project: &StorageProject,
    ) -> Result<ProjectDatabase, ProtectedStorageError> {
        validate_project_identity(project)?;
        let registered_projects = self.registry.registered_projects()?;
        let mut is_registered = false;
        for registered in &registered_projects {
            if registered == project {
                is_registered = true;
            } else if registered.project_id == project.project_id
                || registered.project_key == project.project_key
            {
                return Err(ProtectedStorageError::Storage);
            }
        }

        let project_root = self.projects_dir.join(&project.project_key);
        let project_root_exists = match fs::symlink_metadata(&project_root) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(ProtectedStorageError::Storage),
        };
        if !project_root_exists {
            fs::create_dir(&project_root).map_err(|_| ProtectedStorageError::Storage)?;
        }
        require_owned_directory(&project_root, authority.effective_uid())?;

        let database_path = project_root.join("state/state.db");
        let database_exists = match fs::symlink_metadata(&database_path) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(ProtectedStorageError::Storage),
        };
        let database = if database_exists {
            ProjectDatabase::open_existing_at(&project_root)?
        } else if is_registered {
            return Err(ProtectedStorageError::Storage);
        } else {
            require_recoverable_empty_layout(&project_root)?;
            let database = ProjectDatabase::initialize_at(&project_root)?;
            database.ensure_project(project)?;
            database
        };
        require_single_project(&database, project)?;
        database.recover()?;
        require_database_ownership(&database, authority.effective_uid())?;
        self.registry.ensure_project(project)?;
        Ok(database)
    }

    #[allow(dead_code)]
    pub(crate) fn resolve_authorized_resource(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
    ) -> Result<ResolvedAuthorizedResource, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        match self.resolve_authorized_resource_inner(authority, capability, trusted_now_unix) {
            Ok(resource) => Ok(resource),
            Err(_) => {
                self.audit_rejected_authorized_request_at(capability, trusted_now_unix)?;
                Err(())
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn audit_rejected_authorized_request(
        &self,
        capability: &AuthorizedCapability,
    ) -> Result<ServiceSecurityAuditReceipt, ()> {
        let trusted_now_unix = trusted_service_unix_time().map_err(|_| ())?;
        self.audit_rejected_authorized_request_at(capability, trusted_now_unix)
    }

    fn audit_rejected_authorized_request_at(
        &self,
        capability: &AuthorizedCapability,
        trusted_now_unix: i64,
    ) -> Result<ServiceSecurityAuditReceipt, ()> {
        self.registry
            .record_authorized_request_rejection(capability, trusted_now_unix)
            .map_err(|_| ())
    }

    fn resolve_authorized_resource_inner(
        &self,
        authority: &VerifiedServiceAuthority,
        capability: &AuthorizedCapability,
        trusted_now_unix: i64,
    ) -> Result<ResolvedAuthorizedResource, ProtectedStorageError> {
        let project_id = capability
            .project_id()
            .ok_or(ProtectedStorageError::ResourceUnavailable)?;
        if capability.task_id().is_some() && capability.workspace_id().is_none() {
            return Err(ProtectedStorageError::ResourceUnavailable);
        }
        if !self
            .registry
            .authorized_capability_is_current(capability, trusted_now_unix)?
        {
            return Err(ProtectedStorageError::ResourceUnavailable);
        }

        let mut matches = self
            .registry
            .registered_projects()?
            .into_iter()
            .filter(|project| project.project_id == project_id);
        let project = matches
            .next()
            .ok_or(ProtectedStorageError::ResourceUnavailable)?;
        if matches.next().is_some() {
            return Err(ProtectedStorageError::Storage);
        }
        validate_project_identity(&project)?;

        let project_root = self.projects_dir.join(&project.project_key);
        require_owned_directory(&project_root, authority.effective_uid())?;
        let database = ProjectDatabase::open_existing_at(&project_root)?;
        require_single_project(&database, &project)?;
        database.recover()?;
        require_database_ownership(&database, authority.effective_uid())?;
        if !database.contains_resource_scope(
            project_id,
            capability.workspace_id(),
            capability.task_id(),
        )? {
            return Err(ProtectedStorageError::ResourceUnavailable);
        }

        Ok(ResolvedAuthorizedResource {
            database,
            project_id: project_id.to_string(),
            workspace_id: capability.workspace_id().map(str::to_string),
            task_id: capability.task_id().map(str::to_string),
        })
    }

    fn recover_registered_projects(
        &self,
        authority: &VerifiedServiceAuthority,
    ) -> Result<(), ProtectedStorageError> {
        for project in self.registry.registered_projects()? {
            validate_project_identity(&project)?;
            let project_root = self.projects_dir.join(&project.project_key);
            require_owned_directory(&project_root, authority.effective_uid())?;
            let database = ProjectDatabase::open_existing_at(&project_root)?;
            require_single_project(&database, &project)?;
            database.recover()?;
            require_database_ownership(&database, authority.effective_uid())?;
        }
        Ok(())
    }
}

fn trusted_service_unix_time() -> Result<i64, ProtectedStorageError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ProtectedStorageError::Storage)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| ProtectedStorageError::Storage)
}

fn require_database_ownership(
    database: &ProjectDatabase,
    expected_uid: u32,
) -> Result<(), ProtectedStorageError> {
    let state_dir = database
        .database_path()
        .parent()
        .ok_or(ProtectedStorageError::Storage)?;
    require_owned_directory(state_dir, expected_uid)?;
    for path in std::iter::once(database.database_path().to_path_buf()).chain(
        ["-journal", "-wal", "-shm"].into_iter().map(|suffix| {
            let mut path = database.database_path().as_os_str().to_os_string();
            path.push(suffix);
            PathBuf::from(path)
        }),
    ) {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(ProtectedStorageError::Storage);
            }
            Ok(metadata) => require_owner(&metadata, expected_uid)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ProtectedStorageError::Storage),
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn require_recoverable_empty_layout(project_root: &Path) -> Result<(), ProtectedStorageError> {
    let entries = fs::read_dir(project_root)
        .map_err(|_| ProtectedStorageError::Storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ProtectedStorageError::Storage)?;
    if entries.is_empty() {
        return Ok(());
    }
    if entries.len() != 1 || entries[0].file_name() != "state" {
        return Err(ProtectedStorageError::Storage);
    }
    let state_path = entries[0].path();
    let metadata = fs::symlink_metadata(&state_path).map_err(|_| ProtectedStorageError::Storage)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ProtectedStorageError::Storage);
    }
    if fs::read_dir(state_path)
        .map_err(|_| ProtectedStorageError::Storage)?
        .next()
        .is_some()
    {
        return Err(ProtectedStorageError::Storage);
    }
    Ok(())
}

fn require_single_project(
    database: &ProjectDatabase,
    expected: &StorageProject,
) -> Result<(), ProtectedStorageError> {
    let projects = database.registered_projects()?;
    if projects.len() != 1 || projects.first() != Some(expected) {
        return Err(ProtectedStorageError::Storage);
    }
    Ok(())
}

fn validate_project_identity(project: &StorageProject) -> Result<(), ProtectedStorageError> {
    let Some(token) = project.project_id.strip_prefix("prj_") else {
        return Err(ProtectedStorageError::InvalidProjectIdentity);
    };
    if !valid_crockford_token(token) {
        return Err(ProtectedStorageError::InvalidProjectIdentity);
    }
    let Some((slug, key_token)) = project.project_key.rsplit_once("--") else {
        return Err(ProtectedStorageError::InvalidProjectIdentity);
    };
    if key_token != token || !valid_project_slug(slug) {
        return Err(ProtectedStorageError::InvalidProjectIdentity);
    }
    Ok(())
}

fn valid_crockford_token(token: &str) -> bool {
    token.len() == 26
        && token
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z'))
}

fn valid_project_slug(slug: &str) -> bool {
    (1..=48).contains(&slug.len())
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn private_child_directory(
    parent: &Path,
    name: &str,
    authority: &VerifiedServiceAuthority,
) -> Result<PathBuf, ProtectedStorageError> {
    let path = parent.join(name);
    if !path.exists() {
        fs::create_dir(&path).map_err(|_| ProtectedStorageError::Storage)?;
    }
    require_owned_directory(&path, authority.effective_uid())?;
    Ok(path)
}

fn require_owned_directory(path: &Path, expected_uid: u32) -> Result<(), ProtectedStorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ProtectedStorageError::InvalidRoot)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ProtectedStorageError::InvalidRoot);
    }
    require_owner(&metadata, expected_uid)?;
    set_private_directory_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn require_owner(metadata: &fs::Metadata, expected_uid: u32) -> Result<(), ProtectedStorageError> {
    if metadata.uid() != expected_uid {
        return Err(ProtectedStorageError::WrongOwner);
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_owner(
    _metadata: &fs::Metadata,
    _expected_uid: u32,
) -> Result<(), ProtectedStorageError> {
    Err(ProtectedStorageError::WrongOwner)
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), ProtectedStorageError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| ProtectedStorageError::Storage)
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), ProtectedStorageError> {
    Err(ProtectedStorageError::Storage)
}

#[cfg(test)]
#[path = "tests/Service_Storage_test.rs"]
mod service_storage_tests;
