#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[cfg(unix)]
use std::ffi::CStr;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use gix::bstr::ByteSlice;
use serde::{Deserialize, Serialize};

const BACKUP_DIRECTORY_NAME: &str = "backup";
const REPOSITORY_DIRECTORY_NAME: &str = "repository.git";
const SNAPSHOT_REFERENCE: &str = "refs/heads/snapshots";
const MAX_SNAPSHOT_ENTRIES: usize = 100_000;
const MAX_SINGLE_FILE_BYTES: usize = 64 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SNAPSHOT_DEPTH: usize = 64;
const MAX_RELATIVE_PATH_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtectedGitBackupError {
    InvalidRoot,
    InvalidMetadata,
    InvalidEntry,
    DuplicateEntry,
    LimitExceeded,
    Repository,
    Commit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackupTransition {
    Baseline,
    Submission,
    Review,
    Advance,
}

impl BackupTransition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Submission => "submission",
            Self::Review => "review",
            Self::Advance => "advance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackupSnapshotEntry {
    pub(crate) relative_path: String,
    pub(crate) executable: bool,
    pub(crate) contents: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackupCommitMetadata {
    pub(crate) transition: BackupTransition,
    pub(crate) project_id: String,
    pub(crate) workspace_id: String,
    pub(crate) task_id: Option<String>,
    pub(crate) task_revision: Option<i64>,
    pub(crate) state_revision: i64,
    pub(crate) occurred_at_unix: i64,
    pub(crate) observed_compilation_fingerprint: Option<String>,
    pub(crate) transition_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackupCommitReceipt {
    pub(crate) commit_id: String,
    pub(crate) parent_commit_id: Option<String>,
    pub(crate) source_fingerprint: String,
    pub(crate) entry_count: u64,
    pub(crate) byte_count: u64,
}

#[derive(Debug, Serialize)]
struct SnapshotMetadataDocument<'a> {
    contract_version: &'static str,
    transition: BackupTransition,
    project_id: &'a str,
    workspace_id: &'a str,
    task_id: Option<&'a str>,
    task_revision: Option<i64>,
    state_revision: i64,
    occurred_at: String,
    source_fingerprint: &'a str,
    observed_compilation_fingerprint: Option<&'a str>,
    transition_id: Option<&'a str>,
    entry_count: u64,
    byte_count: u64,
}

#[derive(Debug, Deserialize)]
struct StoredSnapshotMetadata {
    contract_version: String,
    transition: BackupTransition,
    project_id: String,
    workspace_id: String,
    task_id: Option<String>,
    task_revision: Option<i64>,
    state_revision: i64,
    source_fingerprint: String,
    observed_compilation_fingerprint: Option<String>,
    transition_id: Option<String>,
    entry_count: u64,
    byte_count: u64,
}

enum SnapshotNode {
    Directory(BTreeMap<String, SnapshotNode>),
    File { executable: bool, contents: Vec<u8> },
}

pub(crate) struct ProtectedGitBackup {
    backup_root: PathBuf,
    repository_path: PathBuf,
    expected_uid: u32,
}

impl ProtectedGitBackup {
    pub(crate) fn initialize(
        project_storage_root: &Path,
        expected_uid: u32,
    ) -> Result<Self, ProtectedGitBackupError> {
        require_owned_directory(project_storage_root, expected_uid)?;
        let backup_root = create_or_require_private_directory(
            project_storage_root,
            BACKUP_DIRECTORY_NAME,
            expected_uid,
        )?;
        let repository_path = backup_root.join(REPOSITORY_DIRECTORY_NAME);
        match fs::symlink_metadata(&repository_path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !metadata.is_dir()
                    || owner(&metadata) != Some(expected_uid)
                {
                    return Err(ProtectedGitBackupError::InvalidRoot);
                }
                gix::open(&repository_path).map_err(|_| ProtectedGitBackupError::Repository)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                gix::init_bare(&repository_path)
                    .map_err(|_| ProtectedGitBackupError::Repository)?;
            }
            Err(_) => return Err(ProtectedGitBackupError::InvalidRoot),
        }
        let repository =
            gix::open(&repository_path).map_err(|_| ProtectedGitBackupError::Repository)?;
        if !repository.is_bare() {
            return Err(ProtectedGitBackupError::Repository);
        }
        harden_repository_tree(&repository_path, expected_uid)?;
        Ok(Self {
            backup_root,
            repository_path,
            expected_uid,
        })
    }

    pub(crate) fn commit_snapshot(
        &self,
        metadata: &BackupCommitMetadata,
        entries: Vec<BackupSnapshotEntry>,
    ) -> Result<BackupCommitReceipt, ProtectedGitBackupError> {
        require_owned_directory(&self.backup_root, self.expected_uid)?;
        require_owned_directory(&self.repository_path, self.expected_uid)?;
        validate_metadata(metadata)?;
        let (source_tree, source_fingerprint, byte_count) = validated_source_tree(entries)?;
        let entry_count = count_files(&source_tree);
        let source_fingerprint_hex = lowercase_hex(&source_fingerprint);
        let metadata_document = SnapshotMetadataDocument {
            contract_version: "protected-git-snapshot-v1",
            transition: metadata.transition,
            project_id: &metadata.project_id,
            workspace_id: &metadata.workspace_id,
            task_id: metadata.task_id.as_deref(),
            task_revision: metadata.task_revision,
            state_revision: metadata.state_revision,
            occurred_at: format!("unix:{}", metadata.occurred_at_unix),
            source_fingerprint: &source_fingerprint_hex,
            observed_compilation_fingerprint: metadata.observed_compilation_fingerprint.as_deref(),
            transition_id: metadata.transition_id.as_deref(),
            entry_count,
            byte_count,
        };
        let metadata_bytes = serde_json::to_vec(&metadata_document)
            .map_err(|_| ProtectedGitBackupError::InvalidMetadata)?;

        let mut root = BTreeMap::new();
        root.insert("source".to_string(), SnapshotNode::Directory(source_tree));
        root.insert(
            "snapshot.json".to_string(),
            SnapshotNode::File {
                executable: false,
                contents: metadata_bytes,
            },
        );

        let repository =
            gix::open(&self.repository_path).map_err(|_| ProtectedGitBackupError::Repository)?;
        if !repository.is_bare() {
            return Err(ProtectedGitBackupError::Repository);
        }
        if let Some(receipt) = existing_transition_receipt(
            &repository,
            metadata,
            &source_fingerprint_hex,
            entry_count,
            byte_count,
        )? {
            return Ok(receipt);
        }
        let tree_id = write_tree(&repository, &root)?;
        let parent = repository
            .try_find_reference(SNAPSHOT_REFERENCE)
            .map_err(|_| ProtectedGitBackupError::Repository)?
            .map(|reference| {
                reference
                    .target()
                    .try_id()
                    .map(ToOwned::to_owned)
                    .ok_or(ProtectedGitBackupError::Repository)
            })
            .transpose()?;
        let parents = parent.iter().cloned().collect::<Vec<_>>();
        let signature_time = format!("{} +0000", metadata.occurred_at_unix);
        let signature = gix::actor::SignatureRef {
            name: b"AntDelirium".as_bstr(),
            email: b"protected@antdelirium.invalid".as_bstr(),
            time: &signature_time,
        };
        let message = format!(
            "antdelirium snapshot: {} project={} workspace={} task={} state_revision={}",
            metadata.transition.as_str(),
            metadata.project_id,
            metadata.workspace_id,
            metadata.task_id.as_deref().unwrap_or("none"),
            metadata.state_revision,
        );
        let commit_id = repository
            .commit_as(
                signature,
                signature,
                SNAPSHOT_REFERENCE,
                message,
                tree_id,
                parents,
            )
            .map_err(|_| ProtectedGitBackupError::Commit)?
            .detach();
        harden_repository_tree(&self.repository_path, self.expected_uid)?;

        let observed = repository
            .find_reference(SNAPSHOT_REFERENCE)
            .map_err(|_| ProtectedGitBackupError::Commit)?
            .target()
            .try_id()
            .map(ToOwned::to_owned)
            .ok_or(ProtectedGitBackupError::Commit)?;
        if observed != commit_id {
            return Err(ProtectedGitBackupError::Commit);
        }

        Ok(BackupCommitReceipt {
            commit_id: commit_id.to_string(),
            parent_commit_id: parent.map(|id| id.to_string()),
            source_fingerprint: source_fingerprint_hex,
            entry_count,
            byte_count,
        })
    }

    pub(crate) fn has_snapshot(&self) -> Result<bool, ProtectedGitBackupError> {
        require_owned_directory(&self.backup_root, self.expected_uid)?;
        require_owned_directory(&self.repository_path, self.expected_uid)?;
        let repository =
            gix::open(&self.repository_path).map_err(|_| ProtectedGitBackupError::Repository)?;
        repository
            .try_find_reference(SNAPSHOT_REFERENCE)
            .map(|reference| reference.is_some())
            .map_err(|_| ProtectedGitBackupError::Repository)
    }

    #[cfg(unix)]
    pub(crate) fn commit_snapshot_from_descriptor(
        &self,
        metadata: &BackupCommitMetadata,
        source_descriptor: &OwnedFd,
    ) -> Result<BackupCommitReceipt, ProtectedGitBackupError> {
        let entries = snapshot_entries_from_descriptor(source_descriptor)?;
        self.commit_snapshot(metadata, entries)
    }

    #[cfg(test)]
    pub(crate) fn repository_path(&self) -> &Path {
        &self.repository_path
    }
}

fn existing_transition_receipt(
    repository: &gix::Repository,
    metadata: &BackupCommitMetadata,
    source_fingerprint: &str,
    entry_count: u64,
    byte_count: u64,
) -> Result<Option<BackupCommitReceipt>, ProtectedGitBackupError> {
    let Some(transition_id) = metadata.transition_id.as_deref() else {
        return Ok(None);
    };
    let Some(reference) = repository
        .try_find_reference(SNAPSHOT_REFERENCE)
        .map_err(|_| ProtectedGitBackupError::Repository)?
    else {
        return Ok(None);
    };
    let mut next_commit = reference
        .target()
        .try_id()
        .map(ToOwned::to_owned)
        .ok_or(ProtectedGitBackupError::Repository)?;

    loop {
        let commit = repository
            .find_object(next_commit)
            .map_err(|_| ProtectedGitBackupError::Repository)?
            .try_into_commit()
            .map_err(|_| ProtectedGitBackupError::Repository)?;
        let tree = commit
            .tree()
            .map_err(|_| ProtectedGitBackupError::Repository)?;
        let snapshot = tree
            .lookup_entry_by_path("snapshot.json")
            .map_err(|_| ProtectedGitBackupError::Repository)?
            .ok_or(ProtectedGitBackupError::Commit)?
            .object()
            .map_err(|_| ProtectedGitBackupError::Repository)?
            .try_into_blob()
            .map_err(|_| ProtectedGitBackupError::Commit)?
            .data
            .clone();
        let stored: StoredSnapshotMetadata =
            serde_json::from_slice(&snapshot).map_err(|_| ProtectedGitBackupError::Commit)?;
        if stored.transition_id.as_deref() == Some(transition_id) {
            if stored.contract_version != "protected-git-snapshot-v1"
                || stored.transition != metadata.transition
                || stored.project_id != metadata.project_id
                || stored.workspace_id != metadata.workspace_id
                || stored.task_id != metadata.task_id
                || stored.task_revision != metadata.task_revision
                || stored.state_revision != metadata.state_revision
                || stored.source_fingerprint != source_fingerprint
                || stored.observed_compilation_fingerprint
                    != metadata.observed_compilation_fingerprint
                || stored.entry_count != entry_count
                || stored.byte_count != byte_count
            {
                return Err(ProtectedGitBackupError::Commit);
            }
            return Ok(Some(BackupCommitReceipt {
                commit_id: commit.id().to_string(),
                parent_commit_id: commit.parent_ids().next().map(|id| id.to_string()),
                source_fingerprint: stored.source_fingerprint,
                entry_count: stored.entry_count,
                byte_count: stored.byte_count,
            }));
        }
        let Some(parent) = commit.parent_ids().next() else {
            return Ok(None);
        };
        next_commit = parent.detach();
    }
}

fn validate_metadata(metadata: &BackupCommitMetadata) -> Result<(), ProtectedGitBackupError> {
    if !valid_opaque_id(&metadata.project_id, "prj_")
        || !valid_opaque_id(&metadata.workspace_id, "wsp_")
        || metadata
            .task_id
            .as_deref()
            .is_some_and(|task_id| !valid_opaque_id(task_id, "tsk_"))
        || metadata.state_revision < 0
        || metadata.task_revision.is_some_and(|revision| revision <= 0)
        || metadata.occurred_at_unix < 0
        || metadata.task_id.is_some() != metadata.task_revision.is_some()
        || (metadata.transition != BackupTransition::Baseline && metadata.task_id.is_none())
        || metadata
            .observed_compilation_fingerprint
            .as_deref()
            .is_some_and(|fingerprint| !valid_sha256_hex(fingerprint))
        || !valid_transition_id(metadata)
    {
        return Err(ProtectedGitBackupError::InvalidMetadata);
    }
    Ok(())
}

fn valid_transition_id(metadata: &BackupCommitMetadata) -> bool {
    match metadata.transition {
        BackupTransition::Baseline => metadata.transition_id.is_none(),
        BackupTransition::Submission => metadata
            .transition_id
            .as_deref()
            .is_some_and(|value| valid_opaque_id(value, "sub_")),
        BackupTransition::Review | BackupTransition::Advance => metadata
            .transition_id
            .as_deref()
            .is_some_and(|value| valid_opaque_id(value, "rev_")),
    }
}

fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_opaque_id(value: &str, prefix: &str) -> bool {
    let Some(token) = value.strip_prefix(prefix) else {
        return false;
    };
    token.len() == 26
        && token.bytes().all(|byte| {
            matches!(byte, b'0'..=b'9' | b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z')
        })
}

fn validated_source_tree(
    entries: Vec<BackupSnapshotEntry>,
) -> Result<(BTreeMap<String, SnapshotNode>, [u8; 32], u64), ProtectedGitBackupError> {
    if entries.len() > MAX_SNAPSHOT_ENTRIES {
        return Err(ProtectedGitBackupError::LimitExceeded);
    }
    if entries.is_empty() {
        return Err(ProtectedGitBackupError::InvalidEntry);
    }
    let mut root = BTreeMap::new();
    let mut ordered = entries;
    ordered.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let mut fingerprint = b"antdelirium-protected-source-snapshot-v1\0".to_vec();
    let mut byte_count = 0_u64;
    let mut previous_path: Option<&str> = None;
    for entry in &ordered {
        if previous_path == Some(entry.relative_path.as_str()) {
            return Err(ProtectedGitBackupError::DuplicateEntry);
        }
        previous_path = Some(&entry.relative_path);
        let components = validated_relative_components(&entry.relative_path)?;
        if entry.contents.len() > MAX_SINGLE_FILE_BYTES {
            return Err(ProtectedGitBackupError::LimitExceeded);
        }
        byte_count = byte_count
            .checked_add(entry.contents.len() as u64)
            .ok_or(ProtectedGitBackupError::LimitExceeded)?;
        if byte_count > MAX_SNAPSHOT_BYTES {
            return Err(ProtectedGitBackupError::LimitExceeded);
        }
        fingerprint.extend_from_slice(&(entry.relative_path.len() as u64).to_be_bytes());
        fingerprint.extend_from_slice(entry.relative_path.as_bytes());
        fingerprint.push(u8::from(entry.executable));
        fingerprint.extend_from_slice(&(entry.contents.len() as u64).to_be_bytes());
        fingerprint.extend_from_slice(&sha256(&entry.contents));
        insert_entry(&mut root, &components, entry)?;
    }
    Ok((root, sha256(&fingerprint), byte_count))
}

fn validated_relative_components(path: &str) -> Result<Vec<String>, ProtectedGitBackupError> {
    if path.is_empty() || path.as_bytes().contains(&0) || path.contains('\\') {
        return Err(ProtectedGitBackupError::InvalidEntry);
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(ProtectedGitBackupError::InvalidEntry);
    }
    let mut components = Vec::new();
    for component in candidate.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or(ProtectedGitBackupError::InvalidEntry)?;
                if value.is_empty() || value == ".git" {
                    return Err(ProtectedGitBackupError::InvalidEntry);
                }
                components.push(value.to_string());
            }
            _ => return Err(ProtectedGitBackupError::InvalidEntry),
        }
    }
    if components.is_empty()
        || matches!(
            components[0].as_str(),
            "target" | "guard" | "plugins" | "AC_Ant-Delirium_props"
        )
    {
        return Err(ProtectedGitBackupError::InvalidEntry);
    }
    Ok(components)
}

#[cfg(unix)]
struct DirectoryStream(*mut libc::DIR);

#[cfg(unix)]
impl DirectoryStream {
    fn duplicate_from(raw_fd: i32) -> Result<Self, ProtectedGitBackupError> {
        let duplicate = unsafe {
            libc::openat(
                raw_fd,
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if duplicate < 0 {
            return Err(ProtectedGitBackupError::InvalidEntry);
        }
        let stream = unsafe { libc::fdopendir(duplicate) };
        if stream.is_null() {
            unsafe {
                libc::close(duplicate);
            }
            return Err(ProtectedGitBackupError::InvalidEntry);
        }
        Ok(Self(stream))
    }

    fn raw_fd(&self) -> i32 {
        unsafe { libc::dirfd(self.0) }
    }
}

#[cfg(unix)]
impl Drop for DirectoryStream {
    fn drop(&mut self) {
        unsafe {
            libc::closedir(self.0);
        }
    }
}

#[cfg(unix)]
fn snapshot_entries_from_descriptor(
    source_descriptor: &OwnedFd,
) -> Result<Vec<BackupSnapshotEntry>, ProtectedGitBackupError> {
    let mut root_stat: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(source_descriptor.as_raw_fd(), &mut root_stat) } != 0
        || root_stat.st_mode & libc::S_IFMT != libc::S_IFDIR
    {
        return Err(ProtectedGitBackupError::InvalidEntry);
    }
    let mut entries = Vec::new();
    let mut total_bytes = 0_u64;
    collect_directory_entries(
        source_descriptor.as_raw_fd(),
        "",
        0,
        &mut entries,
        &mut total_bytes,
    )?;
    if entries.is_empty() {
        return Err(ProtectedGitBackupError::InvalidEntry);
    }
    Ok(entries)
}

#[cfg(unix)]
fn collect_directory_entries(
    raw_fd: i32,
    relative_prefix: &str,
    depth: usize,
    entries: &mut Vec<BackupSnapshotEntry>,
    total_bytes: &mut u64,
) -> Result<(), ProtectedGitBackupError> {
    if depth > MAX_SNAPSHOT_DEPTH {
        return Err(ProtectedGitBackupError::LimitExceeded);
    }
    let directory = DirectoryStream::duplicate_from(raw_fd)?;
    loop {
        let raw_entry = unsafe { libc::readdir(directory.0) };
        if raw_entry.is_null() {
            break;
        }
        let name_cstr = unsafe { CStr::from_ptr((*raw_entry).d_name.as_ptr()) };
        let name_bytes = name_cstr.to_bytes();
        if matches!(name_bytes, b"." | b"..") {
            continue;
        }
        let name =
            std::str::from_utf8(name_bytes).map_err(|_| ProtectedGitBackupError::InvalidEntry)?;
        if relative_prefix.is_empty() && excluded_root_entry(name) {
            continue;
        }
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err(ProtectedGitBackupError::InvalidEntry);
        }
        let relative_path = if relative_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{relative_prefix}/{name}")
        };
        if relative_path.len() > MAX_RELATIVE_PATH_BYTES {
            return Err(ProtectedGitBackupError::LimitExceeded);
        }
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::fstatat(
                directory.raw_fd(),
                name_cstr.as_ptr(),
                &mut stat,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(ProtectedGitBackupError::InvalidEntry);
        }
        match stat.st_mode & libc::S_IFMT {
            libc::S_IFDIR => {
                let child = unsafe {
                    libc::openat(
                        directory.raw_fd(),
                        name_cstr.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                };
                if child < 0 {
                    return Err(ProtectedGitBackupError::InvalidEntry);
                }
                let child = unsafe { OwnedFd::from_raw_fd(child) };
                collect_directory_entries(
                    child.as_raw_fd(),
                    &relative_path,
                    depth + 1,
                    entries,
                    total_bytes,
                )?;
            }
            libc::S_IFREG => {
                if stat.st_size < 0 || stat.st_size as usize > MAX_SINGLE_FILE_BYTES {
                    return Err(ProtectedGitBackupError::LimitExceeded);
                }
                let next_total = total_bytes
                    .checked_add(stat.st_size as u64)
                    .ok_or(ProtectedGitBackupError::LimitExceeded)?;
                if next_total > MAX_SNAPSHOT_BYTES || entries.len() >= MAX_SNAPSHOT_ENTRIES {
                    return Err(ProtectedGitBackupError::LimitExceeded);
                }
                let file_fd = unsafe {
                    libc::openat(
                        directory.raw_fd(),
                        name_cstr.as_ptr(),
                        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                };
                if file_fd < 0 {
                    return Err(ProtectedGitBackupError::InvalidEntry);
                }
                let mut file = unsafe { File::from_raw_fd(file_fd) };
                let opened = file
                    .metadata()
                    .map_err(|_| ProtectedGitBackupError::InvalidEntry)?;
                if !opened.is_file()
                    || opened.dev() != stat.st_dev as u64
                    || opened.ino() != stat.st_ino as u64
                    || opened.len() != stat.st_size as u64
                {
                    return Err(ProtectedGitBackupError::InvalidEntry);
                }
                let mut contents = Vec::with_capacity(stat.st_size as usize);
                file.by_ref()
                    .take(MAX_SINGLE_FILE_BYTES as u64 + 1)
                    .read_to_end(&mut contents)
                    .map_err(|_| ProtectedGitBackupError::InvalidEntry)?;
                if contents.len() != stat.st_size as usize {
                    return Err(ProtectedGitBackupError::InvalidEntry);
                }
                *total_bytes = next_total;
                entries.push(BackupSnapshotEntry {
                    relative_path,
                    executable: stat.st_mode & 0o111 != 0,
                    contents,
                });
            }
            _ => return Err(ProtectedGitBackupError::InvalidEntry),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn excluded_root_entry(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "guard" | "plugins" | "AC_Ant-Delirium_props"
    )
}

fn insert_entry(
    directory: &mut BTreeMap<String, SnapshotNode>,
    components: &[String],
    entry: &BackupSnapshotEntry,
) -> Result<(), ProtectedGitBackupError> {
    let Some((first, rest)) = components.split_first() else {
        return Err(ProtectedGitBackupError::InvalidEntry);
    };
    if rest.is_empty() {
        if directory.contains_key(first) {
            return Err(ProtectedGitBackupError::DuplicateEntry);
        }
        directory.insert(
            first.clone(),
            SnapshotNode::File {
                executable: entry.executable,
                contents: entry.contents.clone(),
            },
        );
        return Ok(());
    }
    let child = directory
        .entry(first.clone())
        .or_insert_with(|| SnapshotNode::Directory(BTreeMap::new()));
    let SnapshotNode::Directory(child) = child else {
        return Err(ProtectedGitBackupError::DuplicateEntry);
    };
    insert_entry(child, rest, entry)
}

fn write_tree(
    repository: &gix::Repository,
    nodes: &BTreeMap<String, SnapshotNode>,
) -> Result<gix::ObjectId, ProtectedGitBackupError> {
    let mut entries = Vec::with_capacity(nodes.len());
    for (name, node) in nodes {
        let (mode, oid) = match node {
            SnapshotNode::Directory(children) => (
                gix::objs::tree::EntryKind::Tree.into(),
                write_tree(repository, children)?,
            ),
            SnapshotNode::File {
                executable,
                contents,
            } => {
                let mode = if *executable {
                    gix::objs::tree::EntryKind::BlobExecutable
                } else {
                    gix::objs::tree::EntryKind::Blob
                };
                let oid = repository
                    .write_blob(contents)
                    .map_err(|_| ProtectedGitBackupError::Repository)?
                    .detach();
                (mode.into(), oid)
            }
        };
        entries.push(gix::objs::tree::Entry {
            mode,
            filename: name.as_bytes().to_vec().into(),
            oid,
        });
    }
    entries.sort();
    repository
        .write_object(&gix::objs::Tree { entries })
        .map_err(|_| ProtectedGitBackupError::Repository)
        .map(gix::Id::detach)
}

fn count_files(nodes: &BTreeMap<String, SnapshotNode>) -> u64 {
    nodes
        .values()
        .map(|node| match node {
            SnapshotNode::Directory(children) => count_files(children),
            SnapshotNode::File { .. } => 1,
        })
        .sum()
}

fn create_or_require_private_directory(
    parent: &Path,
    name: &str,
    expected_uid: u32,
) -> Result<PathBuf, ProtectedGitBackupError> {
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path).map_err(|_| ProtectedGitBackupError::InvalidRoot)?;
        }
        Err(_) => return Err(ProtectedGitBackupError::InvalidRoot),
    }
    require_owned_directory(&path, expected_uid)?;
    Ok(path)
}

fn require_owned_directory(path: &Path, expected_uid: u32) -> Result<(), ProtectedGitBackupError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ProtectedGitBackupError::InvalidRoot)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || owner(&metadata) != Some(expected_uid)
    {
        return Err(ProtectedGitBackupError::InvalidRoot);
    }
    set_mode(path, 0o700)?;
    Ok(())
}

#[cfg(unix)]
fn owner(metadata: &fs::Metadata) -> Option<u32> {
    Some(metadata.uid())
}

#[cfg(not(unix))]
fn owner(_metadata: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), ProtectedGitBackupError> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|_| ProtectedGitBackupError::InvalidRoot)
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<(), ProtectedGitBackupError> {
    Err(ProtectedGitBackupError::InvalidRoot)
}

fn harden_repository_tree(root: &Path, expected_uid: u32) -> Result<(), ProtectedGitBackupError> {
    let metadata = fs::symlink_metadata(root).map_err(|_| ProtectedGitBackupError::InvalidRoot)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || owner(&metadata) != Some(expected_uid)
    {
        return Err(ProtectedGitBackupError::InvalidRoot);
    }
    set_mode(root, 0o700)?;
    for entry in fs::read_dir(root).map_err(|_| ProtectedGitBackupError::InvalidRoot)? {
        let entry = entry.map_err(|_| ProtectedGitBackupError::InvalidRoot)?;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| ProtectedGitBackupError::InvalidRoot)?;
        if metadata.file_type().is_symlink() || owner(&metadata) != Some(expected_uid) {
            return Err(ProtectedGitBackupError::InvalidRoot);
        }
        if metadata.is_dir() {
            harden_repository_tree(&entry.path(), expected_uid)?;
        } else if metadata.is_file() {
            set_mode(&entry.path(), 0o600)?;
        } else {
            return Err(ProtectedGitBackupError::InvalidRoot);
        }
    }
    Ok(())
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
#[path = "tests/Protected_Git_Backup_test.rs"]
mod protected_git_backup_tests;
