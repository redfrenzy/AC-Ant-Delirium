use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::random_crockford_id;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const INVENTORY_CONTRACT: &str = "legacy-source-inventory-v1";
const MIGRATION_MANIFEST_CONTRACT: &str = "migration-manifest-v1";
const MAX_INVENTORY_BYTES: usize = 8 * 1024 * 1024;
const MAX_INVENTORY_ENTRIES: usize = 100_000;
const MAX_SINGLE_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_STAGED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyStagingError {
    Inventory,
    SourceMismatch,
    Stream,
    Destination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyStageReceipt {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) source_snapshot_sha256: String,
    pub(crate) staging_snapshot_sha256: String,
    pub(crate) entries: u64,
    pub(crate) regular_files: u64,
    pub(crate) directories: u64,
    pub(crate) staged_bytes: u64,
    pub(crate) state: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyInventoryDocument {
    contract_version: String,
    captured_at: String,
    inventory_sha256: String,
    consistency: String,
    payload: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyInventoryPayload {
    source_kind: String,
    source_locator: String,
    root: InventoryRoot,
    validation: InventoryValidation,
    counts: InventoryEntryCounts,
    mode_histogram: Vec<ModeCount>,
    ownership_histogram: Vec<OwnershipCount>,
    semantic_counts: SemanticCounts,
    git: GitState,
    pub(crate) entries: Vec<InventoryEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryRoot {
    r#type: String,
    mode: String,
    uid: u32,
    gid: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryValidation {
    json_files: u64,
    valid_json_files: u64,
    invalid_json_files: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryEntryCounts {
    entries: u64,
    regular_files: u64,
    directories: u64,
    symlinks: u64,
    other: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeCount {
    mode: String,
    count: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnershipCount {
    uid: u32,
    gid: u32,
    count: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SemanticCounts {
    plans: u64,
    tasks: u64,
    submissions: u64,
    reviews: u64,
    history_events: u64,
    problems: u64,
    transitions: u64,
    json_schemas: u64,
    generated_artifacts: u64,
    task_evidence: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitState {
    repository: bool,
    head: Option<String>,
    branch: Option<String>,
    worktree_clean: Option<bool>,
    status: Vec<String>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InventoryEntry {
    pub(crate) path: String,
    pub(crate) r#type: String,
    pub(crate) mode: String,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) size: u64,
    pub(crate) sha256: Option<String>,
    link: InventoryLink,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryLink {
    kind: String,
    target: Option<String>,
    target_sha256: Option<String>,
}

#[derive(Serialize)]
struct MigrationManifest<'a> {
    contract_version: &'static str,
    migration_id: &'a str,
    project_id: &'a str,
    source_kind: &'static str,
    source_snapshot_sha256: &'a str,
    staging_snapshot_sha256: &'a str,
    source_schema_version: &'static str,
    target_schema_version: &'static str,
    counts: MigrationCounts,
    state: &'static str,
    created_at: &'a str,
    approved_at: Option<String>,
    cutover_at: Option<String>,
    rollback_deadline: Option<String>,
    approvals: Vec<Value>,
    rollback: MigrationRollback<'a>,
}

#[derive(Serialize)]
struct MigrationCounts {
    plans: u64,
    tasks: u64,
    submissions: u64,
    reviews: u64,
    history_events: u64,
    problems: u64,
    transitions: u64,
}

#[derive(Serialize)]
struct MigrationRollback<'a> {
    legacy_snapshot_sha256: &'a str,
    post_cutover_first_sequence: Option<u64>,
    status: &'static str,
}

pub(crate) fn stage_legacy_stream<R: Read>(
    staging_parent: &Path,
    inventory_document: &[u8],
    expected_inventory_sha256: &str,
    stream: &mut R,
) -> Result<LegacyStageReceipt, LegacyStagingError> {
    let payload = validated_inventory_document(inventory_document, expected_inventory_sha256)?;
    stage_validated_stream(
        staging_parent,
        inventory_document,
        expected_inventory_sha256,
        stream,
        payload,
    )
}

pub(crate) fn validated_inventory_document(
    inventory_document: &[u8],
    expected_inventory_sha256: &str,
) -> Result<LegacyInventoryPayload, LegacyStagingError> {
    if inventory_document.is_empty()
        || inventory_document.len() > MAX_INVENTORY_BYTES
        || !valid_sha256(expected_inventory_sha256)
    {
        return Err(LegacyStagingError::Inventory);
    }
    let document: LegacyInventoryDocument =
        serde_json::from_slice(inventory_document).map_err(|_| LegacyStagingError::Inventory)?;
    if document.contract_version != INVENTORY_CONTRACT
        || document.consistency != "two-identical-read-only-passes"
        || document.inventory_sha256 != expected_inventory_sha256
        || document.captured_at.is_empty()
        || value_contains_physical_home(&document.payload)
    {
        return Err(LegacyStagingError::Inventory);
    }
    let mut canonical_payload =
        serde_json::to_vec(&document.payload).map_err(|_| LegacyStagingError::Inventory)?;
    // The T8.2 contract was generated by `jq -cS`, whose canonical output is
    // terminated by one LF before hashing.
    canonical_payload.push(b'\n');
    if lowercase_hex(&sha256(&canonical_payload)) != expected_inventory_sha256 {
        return Err(LegacyStagingError::Inventory);
    }
    let payload: LegacyInventoryPayload =
        serde_json::from_value(document.payload).map_err(|_| LegacyStagingError::Inventory)?;
    validate_inventory_payload(&payload)?;
    Ok(payload)
}

fn stage_validated_stream<R: Read>(
    staging_parent: &Path,
    inventory_document: &[u8],
    expected_inventory_sha256: &str,
    stream: &mut R,
    payload: LegacyInventoryPayload,
) -> Result<LegacyStageReceipt, LegacyStagingError> {
    let migration_id = random_crockford_id("mig_").map_err(|_| LegacyStagingError::Destination)?;
    let project_id = random_crockford_id("prj_").map_err(|_| LegacyStagingError::Destination)?;
    let partial_root = staging_parent.join(format!("{migration_id}.partial"));
    let final_root = staging_parent.join(&migration_id);
    if fs::symlink_metadata(&partial_root).is_ok() || fs::symlink_metadata(&final_root).is_ok() {
        return Err(LegacyStagingError::Destination);
    }
    create_private_directory(&partial_root)?;
    let source_copy = partial_root.join("source");
    create_private_directory(&source_copy)?;

    let mut staged_bytes = 0_u64;
    for entry in &payload.entries {
        let relative = validated_relative_path(&entry.path)?;
        let destination = source_copy.join(relative);
        match entry.r#type.as_str() {
            "directory" => create_private_directory(&destination)?,
            "regular" => {
                staged_bytes = staged_bytes
                    .checked_add(entry.size)
                    .filter(|size| *size <= MAX_STAGED_BYTES)
                    .ok_or(LegacyStagingError::Inventory)?;
                stage_regular_file(stream, &destination, entry)?;
            }
            _ => return Err(LegacyStagingError::Inventory),
        }
    }

    let staging_snapshot_sha256 = staged_content_digest(&payload.entries)?;
    let created_at = rfc3339_utc_now()?;
    let manifest = MigrationManifest {
        contract_version: MIGRATION_MANIFEST_CONTRACT,
        migration_id: &migration_id,
        project_id: &project_id,
        source_kind: "legacy_plugin",
        source_snapshot_sha256: expected_inventory_sha256,
        staging_snapshot_sha256: &staging_snapshot_sha256,
        source_schema_version: INVENTORY_CONTRACT,
        target_schema_version: "canonical-state-v1",
        counts: MigrationCounts {
            plans: payload.semantic_counts.plans,
            tasks: payload.semantic_counts.tasks,
            submissions: payload.semantic_counts.submissions,
            reviews: payload.semantic_counts.reviews,
            history_events: payload.semantic_counts.history_events,
            problems: payload.semantic_counts.problems,
            transitions: payload.semantic_counts.transitions,
        },
        state: "staged",
        created_at: &created_at,
        approved_at: None,
        cutover_at: None,
        rollback_deadline: None,
        approvals: Vec::new(),
        rollback: MigrationRollback {
            legacy_snapshot_sha256: expected_inventory_sha256,
            post_cutover_first_sequence: None,
            status: "prepared",
        },
    };
    write_private_json(&partial_root.join("migration-manifest-v1.json"), &manifest)?;
    write_private_bytes(
        &partial_root.join("legacy-source-inventory-v1.json"),
        inventory_document,
    )?;

    let receipt = LegacyStageReceipt {
        contract_version: "legacy-stage-receipt-v1".to_string(),
        migration_id,
        project_id,
        source_snapshot_sha256: expected_inventory_sha256.to_string(),
        staging_snapshot_sha256,
        entries: payload.counts.entries,
        regular_files: payload.counts.regular_files,
        directories: payload.counts.directories,
        staged_bytes,
        state: "staged".to_string(),
    };
    write_private_json(&partial_root.join("stage-receipt-v1.json"), &receipt)?;
    sync_directory(&source_copy)?;
    sync_directory(&partial_root)?;
    fs::rename(&partial_root, &final_root).map_err(|_| LegacyStagingError::Destination)?;
    sync_directory(staging_parent)?;
    Ok(receipt)
}

pub(crate) fn write_validated_source_stream<W: Write>(
    source_root: &Path,
    inventory_document: &[u8],
    expected_inventory_sha256: &str,
    writer: &mut W,
) -> Result<(), LegacyStagingError> {
    let payload = validated_inventory_document(inventory_document, expected_inventory_sha256)?;
    let root_metadata =
        fs::symlink_metadata(source_root).map_err(|_| LegacyStagingError::SourceMismatch)?;
    if !source_root.is_absolute()
        || root_metadata.file_type().is_symlink()
        || !root_metadata.is_dir()
        || unix_mode(&root_metadata) != payload.root.mode
        || root_metadata.uid() != payload.root.uid
        || root_metadata.gid() != payload.root.gid
    {
        return Err(LegacyStagingError::SourceMismatch);
    }
    let mut root_options = OpenOptions::new();
    root_options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let root_directory = root_options
        .open(source_root)
        .map_err(|_| LegacyStagingError::SourceMismatch)?;
    let opened_root_metadata = root_directory
        .metadata()
        .map_err(|_| LegacyStagingError::SourceMismatch)?;
    if metadata_signature(&opened_root_metadata) != metadata_signature(&root_metadata) {
        return Err(LegacyStagingError::SourceMismatch);
    }
    let root_signature = metadata_signature(&opened_root_metadata);
    let observed_paths = collect_source_paths(source_root)?;
    let expected_paths = payload
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    if observed_paths
        .iter()
        .map(String::as_str)
        .ne(expected_paths.iter().copied())
    {
        return Err(LegacyStagingError::SourceMismatch);
    }

    for entry in &payload.entries {
        let relative = validated_relative_path(&entry.path)?;
        let path = source_root.join(&relative);
        let mut file =
            open_source_entry_beneath(&root_directory, &relative, entry.r#type == "directory")?;
        let opened = file
            .metadata()
            .map_err(|_| LegacyStagingError::SourceMismatch)?;
        if unix_mode(&opened) != entry.mode
            || opened.uid() != entry.uid
            || opened.gid() != entry.gid
            || opened.len() != entry.size
        {
            return Err(LegacyStagingError::SourceMismatch);
        }
        match entry.r#type.as_str() {
            "directory" if opened.is_dir() => {}
            "regular" if opened.is_file() => {
                let size =
                    usize::try_from(entry.size).map_err(|_| LegacyStagingError::Inventory)?;
                let mut bytes = vec![0_u8; size];
                file.read_exact(&mut bytes)
                    .map_err(|_| LegacyStagingError::SourceMismatch)?;
                let mut trailing = [0_u8; 1];
                if file
                    .read(&mut trailing)
                    .map_err(|_| LegacyStagingError::SourceMismatch)?
                    != 0
                    || lowercase_hex(&sha256(&bytes))
                        != entry
                            .sha256
                            .as_deref()
                            .ok_or(LegacyStagingError::Inventory)?
                {
                    return Err(LegacyStagingError::SourceMismatch);
                }
                writer
                    .write_all(&bytes)
                    .map_err(|_| LegacyStagingError::Stream)?;
            }
            _ => return Err(LegacyStagingError::SourceMismatch),
        }
        let after = fs::symlink_metadata(&path).map_err(|_| LegacyStagingError::SourceMismatch)?;
        if after.file_type().is_symlink()
            || metadata_signature(&after) != metadata_signature(&opened)
        {
            return Err(LegacyStagingError::SourceMismatch);
        }
    }
    let final_paths = collect_source_paths(source_root)?;
    let final_root =
        fs::symlink_metadata(source_root).map_err(|_| LegacyStagingError::SourceMismatch)?;
    if final_paths
        .iter()
        .map(String::as_str)
        .ne(expected_paths.iter().copied())
        || final_root.file_type().is_symlink()
        || metadata_signature(&final_root) != root_signature
    {
        return Err(LegacyStagingError::SourceMismatch);
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn open_source_entry_beneath(
    root: &File,
    relative: &Path,
    final_is_directory: bool,
) -> Result<File, LegacyStagingError> {
    let components = relative.components().collect::<Vec<_>>();
    let mut current = root
        .try_clone()
        .map_err(|_| LegacyStagingError::SourceMismatch)?;
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(LegacyStagingError::Inventory);
        };
        let name =
            std::ffi::CString::new(name.as_bytes()).map_err(|_| LegacyStagingError::Inventory)?;
        let is_last = index + 1 == components.len();
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        if !is_last || final_is_directory {
            flags |= libc::O_DIRECTORY;
        }
        let descriptor = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(LegacyStagingError::SourceMismatch);
        }
        current = unsafe { File::from_raw_fd(descriptor) };
    }
    Ok(current)
}

pub(crate) fn collect_source_paths(root: &Path) -> Result<Vec<String>, LegacyStagingError> {
    let mut pending = vec![root.to_path_buf()];
    let mut paths = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|_| LegacyStagingError::SourceMismatch)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| LegacyStagingError::SourceMismatch)?;
        for entry in entries {
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|_| LegacyStagingError::SourceMismatch)?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| LegacyStagingError::SourceMismatch)?
                .to_str()
                .ok_or(LegacyStagingError::SourceMismatch)?
                .to_string();
            validated_relative_path(&relative)?;
            paths.push(relative);
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                pending.push(path);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn metadata_signature(metadata: &fs::Metadata) -> (u64, u64, u64, u32, u32, u32, i64, i64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mode(),
        metadata.uid(),
        metadata.gid(),
        metadata.mtime(),
        metadata.ctime(),
    )
}

fn unix_mode(metadata: &fs::Metadata) -> String {
    format!("{:o}", metadata.mode() & 0o7777)
}

fn validate_inventory_payload(payload: &LegacyInventoryPayload) -> Result<(), LegacyStagingError> {
    if payload.source_kind != "legacy-plugin"
        || payload.source_locator != "operator-private"
        || payload.root.r#type != "directory"
        || !valid_mode(&payload.root.mode)
        || payload.entries.is_empty()
        || payload.entries.len() > MAX_INVENTORY_ENTRIES
        || payload.counts.entries != payload.entries.len() as u64
        || payload.counts.symlinks != 0
        || payload.counts.other != 0
        || payload.validation.invalid_json_files != 0
        || payload.validation.json_files != payload.validation.valid_json_files
    {
        return Err(LegacyStagingError::Inventory);
    }
    let _root_identity = (payload.root.uid, payload.root.gid);
    let _legacy_summary = (
        payload.semantic_counts.json_schemas,
        payload.semantic_counts.generated_artifacts,
        payload.semantic_counts.task_evidence,
    );
    validate_git_state(&payload.git)?;

    let mut previous = None::<&str>;
    let mut unique_paths = HashSet::with_capacity(payload.entries.len());
    let mut regular_files = 0_u64;
    let mut directories = 0_u64;
    let mut json_files = 0_u64;
    let mut modes = BTreeMap::<String, u64>::new();
    let mut owners = BTreeMap::<(u32, u32), u64>::new();
    for entry in &payload.entries {
        validated_relative_path(&entry.path)?;
        if previous.is_some_and(|path| path >= entry.path.as_str())
            || !unique_paths.insert(entry.path.as_str())
            || !valid_mode(&entry.mode)
        {
            return Err(LegacyStagingError::Inventory);
        }
        previous = Some(&entry.path);
        *modes.entry(entry.mode.clone()).or_default() += 1;
        *owners.entry((entry.uid, entry.gid)).or_default() += 1;
        match entry.r#type.as_str() {
            "regular" => {
                regular_files += 1;
                if entry.size > MAX_SINGLE_FILE_BYTES
                    || !entry.sha256.as_deref().is_some_and(valid_sha256)
                    || entry.link.kind != "none"
                    || entry.link.target.is_some()
                    || entry.link.target_sha256.is_some()
                {
                    return Err(LegacyStagingError::Inventory);
                }
                if entry.path.to_ascii_lowercase().ends_with(".json") {
                    json_files += 1;
                }
            }
            "directory" => {
                directories += 1;
                if entry.sha256.is_some()
                    || entry.link.kind != "none"
                    || entry.link.target.is_some()
                    || entry.link.target_sha256.is_some()
                {
                    return Err(LegacyStagingError::Inventory);
                }
            }
            _ => return Err(LegacyStagingError::Inventory),
        }
    }
    if regular_files != payload.counts.regular_files
        || directories != payload.counts.directories
        || regular_files + directories != payload.counts.entries
        || json_files != payload.validation.json_files
        || !histograms_match(
            &payload.mode_histogram,
            &payload.ownership_histogram,
            &modes,
            &owners,
        )
    {
        return Err(LegacyStagingError::Inventory);
    }
    Ok(())
}

fn validate_git_state(git: &GitState) -> Result<(), LegacyStagingError> {
    if git.repository {
        if !git.head.as_deref().is_some_and(|head| {
            (40..=64).contains(&head.len()) && head.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) || git.branch.as_deref().is_none_or(str::is_empty)
            || git.worktree_clean != Some(git.status.is_empty())
        {
            return Err(LegacyStagingError::Inventory);
        }
    } else if git.head.is_some()
        || git.branch.is_some()
        || git.worktree_clean.is_some()
        || !git.status.is_empty()
    {
        return Err(LegacyStagingError::Inventory);
    }
    Ok(())
}

fn histograms_match(
    mode_histogram: &[ModeCount],
    ownership_histogram: &[OwnershipCount],
    modes: &BTreeMap<String, u64>,
    owners: &BTreeMap<(u32, u32), u64>,
) -> bool {
    let declared_modes = mode_histogram
        .iter()
        .map(|entry| (entry.mode.clone(), entry.count))
        .collect::<BTreeMap<_, _>>();
    let declared_owners = ownership_histogram
        .iter()
        .map(|entry| ((entry.uid, entry.gid), entry.count))
        .collect::<BTreeMap<_, _>>();
    declared_modes.len() == mode_histogram.len()
        && declared_owners.len() == ownership_histogram.len()
        && &declared_modes == modes
        && &declared_owners == owners
}

fn stage_regular_file<R: Read>(
    stream: &mut R,
    destination: &Path,
    entry: &InventoryEntry,
) -> Result<(), LegacyStagingError> {
    let parent = destination
        .parent()
        .ok_or(LegacyStagingError::Destination)?;
    require_private_directory(parent)?;
    let size = usize::try_from(entry.size).map_err(|_| LegacyStagingError::Inventory)?;
    let mut bytes = vec![0_u8; size];
    stream
        .read_exact(&mut bytes)
        .map_err(|_| LegacyStagingError::Stream)?;
    let expected = entry
        .sha256
        .as_deref()
        .ok_or(LegacyStagingError::Inventory)?;
    if lowercase_hex(&sha256(&bytes)) != expected {
        return Err(LegacyStagingError::SourceMismatch);
    }
    write_private_bytes(destination, &bytes)
}

pub(crate) fn staged_content_digest(
    entries: &[InventoryEntry],
) -> Result<String, LegacyStagingError> {
    let summary = entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "path": entry.path,
                "type": entry.r#type,
                "sha256": entry.sha256,
            })
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&summary).map_err(|_| LegacyStagingError::Inventory)?;
    Ok(lowercase_hex(&sha256(&bytes)))
}

pub(crate) fn validated_relative_path(value: &str) -> Result<PathBuf, LegacyStagingError> {
    if value.is_empty() || value.contains('\0') || value.contains('\n') || value.contains('\r') {
        return Err(LegacyStagingError::Inventory);
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            !matches!(component, Component::Normal(_))
                || matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
        })
    {
        return Err(LegacyStagingError::Inventory);
    }
    Ok(path.to_path_buf())
}

fn value_contains_physical_home(value: &Value) -> bool {
    match value {
        Value::String(value) => value.split('/').any(|segment| segment == "home"),
        Value::Array(values) => values.iter().any(value_contains_physical_home),
        Value::Object(values) => values.values().any(value_contains_physical_home),
        _ => false,
    }
}

fn valid_mode(value: &str) -> bool {
    (value.len() == 3 || value.len() == 4) && value.bytes().all(|byte| matches!(byte, b'0'..=b'7'))
}

pub(crate) fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn create_private_directory(path: &Path) -> Result<(), LegacyStagingError> {
    fs::create_dir(path).map_err(|_| LegacyStagingError::Destination)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| LegacyStagingError::Destination)
}

pub(crate) fn require_private_directory(path: &Path) -> Result<(), LegacyStagingError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| LegacyStagingError::Destination)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(LegacyStagingError::Destination);
    }
    Ok(())
}

pub(crate) fn write_private_json<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), LegacyStagingError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| LegacyStagingError::Destination)?;
    write_private_bytes(path, &bytes)
}

pub(crate) fn write_private_bytes(path: &Path, bytes: &[u8]) -> Result<(), LegacyStagingError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(path)
        .map_err(|_| LegacyStagingError::Destination)?;
    file.write_all(bytes)
        .map_err(|_| LegacyStagingError::Destination)?;
    file.sync_all().map_err(|_| LegacyStagingError::Destination)
}

pub(crate) fn sync_directory(path: &Path) -> Result<(), LegacyStagingError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| LegacyStagingError::Destination)
}

fn rfc3339_utc_now() -> Result<String, LegacyStagingError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LegacyStagingError::Destination)?
        .as_secs();
    let timestamp = libc::time_t::try_from(seconds).map_err(|_| LegacyStagingError::Destination)?;
    let mut output = std::mem::MaybeUninit::<libc::tm>::uninit();
    let result = unsafe { libc::gmtime_r(&timestamp, output.as_mut_ptr()) };
    if result.is_null() {
        return Err(LegacyStagingError::Destination);
    }
    let output = unsafe { output.assume_init() };
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        output.tm_year + 1900,
        output.tm_mon + 1,
        output.tm_mday,
        output.tm_hour,
        output.tm_min,
        output.tm_sec,
    ))
}

pub(crate) fn lowercase_hex(input: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(input.len() * 2);
    for byte in input {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
#[path = "tests/Legacy_Staging_test.rs"]
mod legacy_staging_tests;
