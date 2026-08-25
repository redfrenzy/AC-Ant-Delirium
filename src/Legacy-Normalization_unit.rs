use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::random_crockford_id;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use crate::legacy_staging::write_validated_source_stream;
use crate::legacy_staging::{
    collect_source_paths, lowercase_hex, open_source_entry_beneath, require_private_directory,
    staged_content_digest, sync_directory, valid_sha256, validated_inventory_document,
    validated_relative_path, write_private_bytes, write_private_json,
    write_validated_source_stream_from_directory, InventoryEntry, LegacyStageReceipt,
    LegacyStagingError, SemanticCounts,
};
use crate::limited_protocol::valid_opaque_id;

const NORMALIZATION_REQUEST_CONTRACT: &str = "legacy-normalization-request-v1";
const NORMALIZATION_RECEIPT_CONTRACT: &str = "legacy-normalization-receipt-v1";
const REFERENCE_BINDINGS_CONTRACT: &str = "physical-reference-bindings-v1";
const MAX_CONTROL_DOCUMENT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyNormalizationError {
    Inventory,
    SourceMismatch,
    Staging,
    Mapping,
    Destination,
}

impl From<LegacyStagingError> for LegacyNormalizationError {
    fn from(error: LegacyStagingError) -> Self {
        match error {
            LegacyStagingError::Inventory => Self::Inventory,
            LegacyStagingError::SourceMismatch => Self::SourceMismatch,
            LegacyStagingError::Stream => Self::SourceMismatch,
            LegacyStagingError::Destination => Self::Staging,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PhysicalReferenceBinding {
    pub(crate) source_value_sha256: String,
    pub(crate) reference_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyNormalizationRequest {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) expected_source_snapshot_sha256: String,
    pub(crate) source_normalized_snapshot_sha256: String,
    pub(crate) source_semantic_snapshot_sha256: String,
    pub(crate) physical_references: Vec<PhysicalReferenceBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyNormalizationReceipt {
    pub(crate) contract_version: String,
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) source_snapshot_sha256: String,
    pub(crate) staging_snapshot_sha256: String,
    pub(crate) normalized_snapshot_sha256: String,
    pub(crate) semantic_snapshot_sha256: String,
    pub(crate) entries_compared: u64,
    pub(crate) regular_files_compared: u64,
    pub(crate) json_documents_compared: u64,
    pub(crate) physical_references: u64,
    pub(crate) physical_occurrences: u64,
    pub(crate) comparison: String,
    pub(crate) state: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicalReferenceBindingsDocument {
    contract_version: String,
    migration_id: String,
    project_id: String,
    bindings: Vec<PhysicalReferenceBinding>,
}

pub(crate) struct ValidatedLegacyNormalization {
    pub(crate) receipt: LegacyNormalizationReceipt,
    pub(crate) semantic_counts: SemanticCounts,
    pub(crate) files: Vec<ValidatedNormalizedFile>,
}

pub(crate) struct ValidatedNormalizedFile {
    pub(crate) relative_path: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone)]
struct EntryBytes {
    entry: InventoryEntry,
    bytes: Option<Vec<u8>>,
}

struct NormalizedSnapshot {
    entries: Vec<EntryBytes>,
    normalized_snapshot_sha256: String,
    semantic_snapshot_sha256: String,
    regular_files: u64,
    json_documents: u64,
    physical_occurrences: u64,
}

#[derive(Serialize)]
struct SnapshotEntry<'a> {
    path: &'a str,
    r#type: &'a str,
    content_sha256: Option<String>,
}

#[cfg(test)]
pub(crate) fn prepare_source_normalization(
    source_root: &Path,
    inventory_document: &[u8],
    expected_inventory_sha256: &str,
    migration_id: &str,
) -> Result<LegacyNormalizationRequest, LegacyNormalizationError> {
    let mut stream = Vec::new();
    write_validated_source_stream(
        source_root,
        inventory_document,
        expected_inventory_sha256,
        &mut stream,
    )?;
    prepare_source_normalization_from_stream(
        inventory_document,
        expected_inventory_sha256,
        migration_id,
        stream,
    )
}

#[cfg(unix)]
pub(crate) fn prepare_source_normalization_from_directory(
    source_root: &File,
    inventory_document: &[u8],
    expected_inventory_sha256: &str,
    migration_id: &str,
) -> Result<LegacyNormalizationRequest, LegacyNormalizationError> {
    let mut stream = Vec::new();
    write_validated_source_stream_from_directory(
        source_root,
        inventory_document,
        expected_inventory_sha256,
        &mut stream,
    )?;
    prepare_source_normalization_from_stream(
        inventory_document,
        expected_inventory_sha256,
        migration_id,
        stream,
    )
}

fn prepare_source_normalization_from_stream(
    inventory_document: &[u8],
    expected_inventory_sha256: &str,
    migration_id: &str,
    stream: Vec<u8>,
) -> Result<LegacyNormalizationRequest, LegacyNormalizationError> {
    if !valid_opaque_id(migration_id, "mig_") {
        return Err(LegacyNormalizationError::Inventory);
    }
    let payload = validated_inventory_document(inventory_document, expected_inventory_sha256)?;
    let entries = split_inventory_stream(&payload.entries, &stream)?;
    let physical_values = collect_physical_values(&entries)?;
    let mut physical_references = Vec::with_capacity(physical_values.len());
    for value in physical_values {
        physical_references.push(PhysicalReferenceBinding {
            source_value_sha256: lowercase_hex(&sha256(&value)),
            reference_id: random_crockford_id("ref_")
                .map_err(|_| LegacyNormalizationError::Mapping)?,
        });
    }
    physical_references
        .sort_by(|left, right| left.source_value_sha256.cmp(&right.source_value_sha256));
    let snapshot = normalize_entries(&entries, &physical_references)?;
    Ok(LegacyNormalizationRequest {
        contract_version: NORMALIZATION_REQUEST_CONTRACT.to_string(),
        migration_id: migration_id.to_string(),
        expected_source_snapshot_sha256: expected_inventory_sha256.to_string(),
        source_normalized_snapshot_sha256: snapshot.normalized_snapshot_sha256,
        source_semantic_snapshot_sha256: snapshot.semantic_snapshot_sha256,
        physical_references,
    })
}

pub(crate) fn normalize_staged_migration(
    staging_parent: &Path,
    request: &LegacyNormalizationRequest,
) -> Result<LegacyNormalizationReceipt, LegacyNormalizationError> {
    validate_request(request)?;
    require_private_directory(staging_parent)?;
    let migration_root = staging_parent.join(&request.migration_id);
    require_private_directory(&migration_root)?;

    let receipt: LegacyStageReceipt =
        read_control_json(&migration_root.join("stage-receipt-v1.json"))?;
    let manifest: Value = read_control_json(&migration_root.join("migration-manifest-v1.json"))?;
    if receipt.contract_version != "legacy-stage-receipt-v1"
        || receipt.migration_id != request.migration_id
        || receipt.source_snapshot_sha256 != request.expected_source_snapshot_sha256
        || receipt.state != "staged"
        || manifest["contract_version"] != "migration-manifest-v1"
        || manifest["migration_id"] != request.migration_id
        || manifest["project_id"] != receipt.project_id
        || manifest["source_snapshot_sha256"] != request.expected_source_snapshot_sha256
        || manifest["staging_snapshot_sha256"] != receipt.staging_snapshot_sha256
        || manifest["state"] != "staged"
    {
        return Err(LegacyNormalizationError::Staging);
    }

    let inventory_document = read_control_file(
        &migration_root.join("legacy-source-inventory-v1.json"),
        MAX_CONTROL_DOCUMENT_BYTES,
    )?;
    let payload = validated_inventory_document(
        &inventory_document,
        &request.expected_source_snapshot_sha256,
    )?;
    let regular_files = payload
        .entries
        .iter()
        .filter(|entry| entry.r#type == "regular")
        .count() as u64;
    let directories = payload.entries.len() as u64 - regular_files;
    let staged_bytes = payload
        .entries
        .iter()
        .filter(|entry| entry.r#type == "regular")
        .try_fold(0_u64, |total, entry| total.checked_add(entry.size))
        .ok_or(LegacyNormalizationError::Inventory)?;
    if !valid_opaque_id(&receipt.project_id, "prj_")
        || !valid_sha256(&receipt.staging_snapshot_sha256)
        || receipt.staging_snapshot_sha256 != staged_content_digest(&payload.entries)?
        || receipt.entries != payload.entries.len() as u64
        || receipt.regular_files != regular_files
        || receipt.directories != directories
        || receipt.staged_bytes != staged_bytes
    {
        return Err(LegacyNormalizationError::Staging);
    }
    let source_root = migration_root.join("source");
    let entries = read_staged_entries(&source_root, &payload.entries)?;
    let snapshot = normalize_entries(&entries, &request.physical_references)?;
    if snapshot.normalized_snapshot_sha256 != request.source_normalized_snapshot_sha256
        || snapshot.semantic_snapshot_sha256 != request.source_semantic_snapshot_sha256
    {
        return Err(LegacyNormalizationError::SourceMismatch);
    }

    let final_root = migration_root.join("normalized");
    if final_root.exists() {
        require_private_directory(&final_root)?;
        let existing: LegacyNormalizationReceipt =
            read_control_json(&final_root.join("normalization-receipt-v1.json"))?;
        let existing_bindings: PhysicalReferenceBindingsDocument =
            read_control_json(&final_root.join("physical-reference-bindings-v1.json"))?;
        validate_bindings(&existing_bindings.bindings)?;
        let existing_snapshot = normalize_entries(&entries, &existing_bindings.bindings)?;
        if existing.contract_version != NORMALIZATION_RECEIPT_CONTRACT
            || existing.migration_id != request.migration_id
            || existing.project_id != receipt.project_id
            || existing.source_snapshot_sha256 != request.expected_source_snapshot_sha256
            || existing.staging_snapshot_sha256 != receipt.staging_snapshot_sha256
            || existing.normalized_snapshot_sha256 != existing_snapshot.normalized_snapshot_sha256
            || existing.semantic_snapshot_sha256 != existing_snapshot.semantic_snapshot_sha256
            || existing.entries_compared != existing_snapshot.entries.len() as u64
            || existing.regular_files_compared != existing_snapshot.regular_files
            || existing.json_documents_compared != existing_snapshot.json_documents
            || existing.physical_references != existing_bindings.bindings.len() as u64
            || existing.physical_occurrences != existing_snapshot.physical_occurrences
            || existing.comparison != "equivalent"
            || existing.state != "normalized"
            || existing_bindings.contract_version != REFERENCE_BINDINGS_CONTRACT
            || existing_bindings.migration_id != request.migration_id
            || existing_bindings.project_id != receipt.project_id
        {
            return Err(LegacyNormalizationError::Destination);
        }
        verify_materialized_entries(&final_root.join("source"), &existing_snapshot.entries)?;
        return Ok(existing);
    }
    let partial_root = migration_root.join("normalized.partial");
    if partial_root.exists() {
        return Err(LegacyNormalizationError::Destination);
    }
    create_private_directory(&partial_root)?;
    let normalized_source = partial_root.join("source");
    create_private_directory(&normalized_source)?;
    materialize_normalized_entries(&normalized_source, &snapshot.entries)?;

    let bindings = PhysicalReferenceBindingsDocument {
        contract_version: REFERENCE_BINDINGS_CONTRACT.to_string(),
        migration_id: request.migration_id.clone(),
        project_id: receipt.project_id.clone(),
        bindings: request.physical_references.clone(),
    };
    write_private_json(
        &partial_root.join("physical-reference-bindings-v1.json"),
        &bindings,
    )?;
    let normalization_receipt = LegacyNormalizationReceipt {
        contract_version: NORMALIZATION_RECEIPT_CONTRACT.to_string(),
        migration_id: request.migration_id.clone(),
        project_id: receipt.project_id,
        source_snapshot_sha256: request.expected_source_snapshot_sha256.clone(),
        staging_snapshot_sha256: receipt.staging_snapshot_sha256,
        normalized_snapshot_sha256: snapshot.normalized_snapshot_sha256,
        semantic_snapshot_sha256: snapshot.semantic_snapshot_sha256,
        entries_compared: snapshot.entries.len() as u64,
        regular_files_compared: snapshot.regular_files,
        json_documents_compared: snapshot.json_documents,
        physical_references: request.physical_references.len() as u64,
        physical_occurrences: snapshot.physical_occurrences,
        comparison: "equivalent".to_string(),
        state: "normalized".to_string(),
    };
    write_private_json(
        &partial_root.join("normalization-receipt-v1.json"),
        &normalization_receipt,
    )?;
    sync_directory(&normalized_source)?;
    sync_directory(&partial_root)?;
    fs::rename(&partial_root, &final_root).map_err(|_| LegacyNormalizationError::Destination)?;
    sync_directory(&migration_root)?;
    Ok(normalization_receipt)
}

pub(crate) fn validate_existing_normalization(
    staging_parent: &Path,
    migration_id: &str,
    expected_source_snapshot_sha256: &str,
    expected_staging_snapshot_sha256: &str,
    expected_normalized_snapshot_sha256: &str,
    expected_semantic_snapshot_sha256: &str,
) -> Result<ValidatedLegacyNormalization, LegacyNormalizationError> {
    if !valid_opaque_id(migration_id, "mig_")
        || !valid_sha256(expected_source_snapshot_sha256)
        || !valid_sha256(expected_staging_snapshot_sha256)
        || !valid_sha256(expected_normalized_snapshot_sha256)
        || !valid_sha256(expected_semantic_snapshot_sha256)
    {
        return Err(LegacyNormalizationError::Inventory);
    }
    require_private_directory(staging_parent)?;
    let migration_root = staging_parent.join(migration_id);
    require_private_directory(&migration_root)?;
    let normalized_root = migration_root.join("normalized");
    require_private_directory(&normalized_root)?;
    let receipt: LegacyNormalizationReceipt =
        read_control_json(&normalized_root.join("normalization-receipt-v1.json"))?;
    let bindings: PhysicalReferenceBindingsDocument =
        read_control_json(&normalized_root.join("physical-reference-bindings-v1.json"))?;
    if receipt.contract_version != NORMALIZATION_RECEIPT_CONTRACT
        || receipt.migration_id != migration_id
        || receipt.source_snapshot_sha256 != expected_source_snapshot_sha256
        || receipt.staging_snapshot_sha256 != expected_staging_snapshot_sha256
        || receipt.normalized_snapshot_sha256 != expected_normalized_snapshot_sha256
        || receipt.semantic_snapshot_sha256 != expected_semantic_snapshot_sha256
        || bindings.contract_version != REFERENCE_BINDINGS_CONTRACT
        || bindings.migration_id != migration_id
        || bindings.project_id != receipt.project_id
    {
        return Err(LegacyNormalizationError::Destination);
    }
    let request = LegacyNormalizationRequest {
        contract_version: NORMALIZATION_REQUEST_CONTRACT.to_string(),
        migration_id: migration_id.to_string(),
        expected_source_snapshot_sha256: expected_source_snapshot_sha256.to_string(),
        source_normalized_snapshot_sha256: expected_normalized_snapshot_sha256.to_string(),
        source_semantic_snapshot_sha256: expected_semantic_snapshot_sha256.to_string(),
        physical_references: bindings.bindings.clone(),
    };
    let verified = normalize_staged_migration(staging_parent, &request)?;
    if verified != receipt {
        return Err(LegacyNormalizationError::Destination);
    }
    let inventory_document = read_control_file(
        &migration_root.join("legacy-source-inventory-v1.json"),
        MAX_CONTROL_DOCUMENT_BYTES,
    )?;
    let payload =
        validated_inventory_document(&inventory_document, expected_source_snapshot_sha256)?;
    let staged_entries = read_staged_entries(&migration_root.join("source"), &payload.entries)?;
    let snapshot = normalize_entries(&staged_entries, &bindings.bindings)?;
    if snapshot.normalized_snapshot_sha256 != receipt.normalized_snapshot_sha256
        || snapshot.semantic_snapshot_sha256 != receipt.semantic_snapshot_sha256
    {
        return Err(LegacyNormalizationError::Destination);
    }
    let files = snapshot
        .entries
        .into_iter()
        .filter_map(|entry| {
            entry.bytes.map(|bytes| ValidatedNormalizedFile {
                relative_path: entry.entry.path,
                bytes,
            })
        })
        .collect();
    Ok(ValidatedLegacyNormalization {
        receipt,
        semantic_counts: payload.semantic_counts,
        files,
    })
}

fn validate_request(request: &LegacyNormalizationRequest) -> Result<(), LegacyNormalizationError> {
    if request.contract_version != NORMALIZATION_REQUEST_CONTRACT
        || !valid_opaque_id(&request.migration_id, "mig_")
        || !valid_sha256(&request.expected_source_snapshot_sha256)
        || !valid_sha256(&request.source_normalized_snapshot_sha256)
        || !valid_sha256(&request.source_semantic_snapshot_sha256)
        || request.physical_references.len() > 100_000
    {
        return Err(LegacyNormalizationError::Mapping);
    }
    validate_bindings(&request.physical_references)
}

fn validate_bindings(
    bindings: &[PhysicalReferenceBinding],
) -> Result<(), LegacyNormalizationError> {
    let mut hashes = BTreeSet::new();
    let mut references = BTreeSet::new();
    for binding in bindings {
        if !valid_sha256(&binding.source_value_sha256)
            || !valid_opaque_id(&binding.reference_id, "ref_")
            || !hashes.insert(binding.source_value_sha256.as_str())
            || !references.insert(binding.reference_id.as_str())
        {
            return Err(LegacyNormalizationError::Mapping);
        }
    }
    Ok(())
}

fn split_inventory_stream(
    inventory_entries: &[InventoryEntry],
    stream: &[u8],
) -> Result<Vec<EntryBytes>, LegacyNormalizationError> {
    let mut offset = 0_usize;
    let mut entries = Vec::with_capacity(inventory_entries.len());
    for entry in inventory_entries {
        let bytes = if entry.r#type == "regular" {
            let size =
                usize::try_from(entry.size).map_err(|_| LegacyNormalizationError::Inventory)?;
            let end = offset
                .checked_add(size)
                .filter(|end| *end <= stream.len())
                .ok_or(LegacyNormalizationError::SourceMismatch)?;
            let bytes = stream[offset..end].to_vec();
            offset = end;
            Some(bytes)
        } else {
            None
        };
        entries.push(EntryBytes {
            entry: entry.clone(),
            bytes,
        });
    }
    if offset != stream.len() {
        return Err(LegacyNormalizationError::SourceMismatch);
    }
    Ok(entries)
}

fn read_staged_entries(
    source_root: &Path,
    inventory_entries: &[InventoryEntry],
) -> Result<Vec<EntryBytes>, LegacyNormalizationError> {
    require_private_directory(source_root)?;
    let expected_paths = inventory_entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    let observed_paths = collect_source_paths(source_root)?;
    if observed_paths
        .iter()
        .map(String::as_str)
        .ne(expected_paths.iter().copied())
    {
        return Err(LegacyNormalizationError::Staging);
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let root = options
        .open(source_root)
        .map_err(|_| LegacyNormalizationError::Staging)?;
    let root_metadata = root
        .metadata()
        .map_err(|_| LegacyNormalizationError::Staging)?;
    let root_owner = root_metadata.uid();
    let root_signature = metadata_signature(&root_metadata);
    let mut entries = Vec::with_capacity(inventory_entries.len());
    for entry in inventory_entries {
        let relative = validated_relative_path(&entry.path)?;
        let mut file = open_source_entry_beneath(&root, &relative, entry.r#type == "directory")?;
        let metadata = file
            .metadata()
            .map_err(|_| LegacyNormalizationError::Staging)?;
        if metadata.uid() != root_owner
            || (entry.r#type == "directory"
                && (!metadata.is_dir() || metadata.mode() & 0o777 != 0o700))
            || (entry.r#type == "regular"
                && (!metadata.is_file() || metadata.mode() & 0o777 != 0o600))
        {
            return Err(LegacyNormalizationError::Staging);
        }
        let bytes = if entry.r#type == "regular" {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|_| LegacyNormalizationError::Staging)?;
            if bytes.len() as u64 != entry.size
                || lowercase_hex(&sha256(&bytes)) != entry.sha256.as_deref().unwrap_or("")
            {
                return Err(LegacyNormalizationError::Staging);
            }
            Some(bytes)
        } else {
            None
        };
        let after = fs::symlink_metadata(source_root.join(&relative))
            .map_err(|_| LegacyNormalizationError::Staging)?;
        if after.file_type().is_symlink()
            || metadata_signature(&after) != metadata_signature(&metadata)
        {
            return Err(LegacyNormalizationError::Staging);
        }
        entries.push(EntryBytes {
            entry: entry.clone(),
            bytes,
        });
    }
    let final_paths = collect_source_paths(source_root)?;
    let final_root =
        fs::symlink_metadata(source_root).map_err(|_| LegacyNormalizationError::Staging)?;
    if final_paths
        .iter()
        .map(String::as_str)
        .ne(expected_paths.iter().copied())
        || final_root.file_type().is_symlink()
        || metadata_signature(&final_root) != root_signature
    {
        return Err(LegacyNormalizationError::Staging);
    }
    Ok(entries)
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

fn collect_physical_values(
    entries: &[EntryBytes],
) -> Result<BTreeSet<Vec<u8>>, LegacyNormalizationError> {
    let mut values = BTreeSet::new();
    for entry in entries {
        let Some(bytes) = entry.bytes.as_deref() else {
            continue;
        };
        if !is_text(bytes) {
            continue;
        }
        for (start, end) in physical_path_spans(bytes) {
            values.insert(bytes[start..end].to_vec());
        }
    }
    Ok(values)
}

fn normalize_entries(
    entries: &[EntryBytes],
    bindings: &[PhysicalReferenceBinding],
) -> Result<NormalizedSnapshot, LegacyNormalizationError> {
    let mapping = bindings
        .iter()
        .map(|binding| {
            (
                binding.source_value_sha256.as_str(),
                binding.reference_id.as_bytes(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if mapping.len() != bindings.len() {
        return Err(LegacyNormalizationError::Mapping);
    }
    let mut used = BTreeMap::<String, u64>::new();
    let mut normalized_entries = Vec::with_capacity(entries.len());
    let mut raw_summary = Vec::with_capacity(entries.len());
    let mut semantic_summary = Vec::with_capacity(entries.len());
    let mut regular_files = 0_u64;
    let mut json_documents = 0_u64;
    let mut physical_occurrences = 0_u64;
    for entry in entries {
        let normalized = match entry.bytes.as_deref() {
            Some(bytes) => {
                regular_files += 1;
                let (normalized, occurrences) = normalize_bytes(bytes, &mapping, &mut used)?;
                physical_occurrences += occurrences;
                if is_text(&normalized) && !physical_path_spans(&normalized).is_empty() {
                    return Err(LegacyNormalizationError::Mapping);
                }
                let raw_digest = lowercase_hex(&sha256(&normalized));
                raw_summary.push(SnapshotEntry {
                    path: &entry.entry.path,
                    r#type: &entry.entry.r#type,
                    content_sha256: Some(raw_digest),
                });
                let semantic_digest = if entry.entry.path.to_ascii_lowercase().ends_with(".json") {
                    json_documents += 1;
                    let value: Value = serde_json::from_slice(&normalized)
                        .map_err(|_| LegacyNormalizationError::SourceMismatch)?;
                    let canonical = serde_json::to_vec(&value)
                        .map_err(|_| LegacyNormalizationError::SourceMismatch)?;
                    lowercase_hex(&sha256(&canonical))
                } else {
                    lowercase_hex(&sha256(&normalized))
                };
                semantic_summary.push(SnapshotEntry {
                    path: &entry.entry.path,
                    r#type: &entry.entry.r#type,
                    content_sha256: Some(semantic_digest),
                });
                Some(normalized)
            }
            None => {
                raw_summary.push(SnapshotEntry {
                    path: &entry.entry.path,
                    r#type: &entry.entry.r#type,
                    content_sha256: None,
                });
                semantic_summary.push(SnapshotEntry {
                    path: &entry.entry.path,
                    r#type: &entry.entry.r#type,
                    content_sha256: None,
                });
                None
            }
        };
        normalized_entries.push(EntryBytes {
            entry: entry.entry.clone(),
            bytes: normalized,
        });
    }
    if used.len() != mapping.len() || used.values().any(|count| *count == 0) {
        return Err(LegacyNormalizationError::Mapping);
    }
    let raw_bytes =
        serde_json::to_vec(&raw_summary).map_err(|_| LegacyNormalizationError::SourceMismatch)?;
    let semantic_bytes = serde_json::to_vec(&semantic_summary)
        .map_err(|_| LegacyNormalizationError::SourceMismatch)?;
    Ok(NormalizedSnapshot {
        entries: normalized_entries,
        normalized_snapshot_sha256: lowercase_hex(&sha256(&raw_bytes)),
        semantic_snapshot_sha256: lowercase_hex(&sha256(&semantic_bytes)),
        regular_files,
        json_documents,
        physical_occurrences,
    })
}

fn normalize_bytes(
    bytes: &[u8],
    mapping: &BTreeMap<&str, &[u8]>,
    used: &mut BTreeMap<String, u64>,
) -> Result<(Vec<u8>, u64), LegacyNormalizationError> {
    if !is_text(bytes) {
        return Ok((bytes.to_vec(), 0));
    }
    let spans = physical_path_spans(bytes);
    if spans.is_empty() {
        return Ok((bytes.to_vec(), 0));
    }
    let mut output = Vec::with_capacity(bytes.len());
    let mut cursor = 0_usize;
    for (start, end) in &spans {
        output.extend_from_slice(&bytes[cursor..*start]);
        let digest = lowercase_hex(&sha256(&bytes[*start..*end]));
        let reference = mapping
            .get(digest.as_str())
            .ok_or(LegacyNormalizationError::Mapping)?;
        output.extend_from_slice(reference);
        *used.entry(digest).or_default() += 1;
        cursor = *end;
    }
    output.extend_from_slice(&bytes[cursor..]);
    Ok((output, spans.len() as u64))
}

fn physical_path_spans(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        if !physical_path_starts_at(bytes, index) {
            index += 1;
            continue;
        }
        let mut end = index;
        while end < bytes.len() && physical_path_byte(bytes[end]) {
            end += 1;
        }
        while end > index && matches!(bytes[end - 1], b'.' | b',' | b';' | b':') {
            end -= 1;
        }
        if end > index {
            spans.push((index, end));
            index = end;
        } else {
            index += 1;
        }
    }
    spans
}

fn is_text(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
        && bytes
            .iter()
            .all(|byte| *byte >= b' ' || matches!(*byte, b'\n' | b'\r' | b'\t'))
}

fn physical_path_starts_at(bytes: &[u8], index: usize) -> bool {
    let boundary = index == 0 || physical_path_boundary(bytes[index - 1]);
    if !boundary {
        return false;
    }
    const POSIX_ROOTS: [&[u8]; 22] = [
        b"/home/", b"/root/", b"/tmp/", b"/var/", b"/etc/", b"/opt/", b"/usr/", b"/run/", b"/srv/",
        b"/mnt/", b"/media/", b"/Users/", b"/bin/", b"/sbin/", b"/lib/", b"/lib64/", b"/dev/",
        b"/proc/", b"/sys/", b"/boot/", b"/nix/", b"/snap/",
    ];
    POSIX_ROOTS
        .iter()
        .any(|root| bytes[index..].starts_with(root))
        || bytes[index..].starts_with(b"~/")
        || (index + 3 <= bytes.len()
            && bytes[index].is_ascii_alphabetic()
            && bytes[index + 1] == b':'
            && matches!(bytes[index + 2], b'/' | b'\\'))
}

fn physical_path_boundary(byte: u8) -> bool {
    !byte.is_ascii_alphanumeric() && !matches!(byte, b'/' | b'\\' | b'_' | b'-' | b'.' | b']')
}

fn physical_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'/' | b'\\'
                | b'_'
                | b'-'
                | b'.'
                | b'['
                | b']'
                | b'@'
                | b'%'
                | b'+'
                | b'='
                | b'~'
                | b':'
        )
}

fn materialize_normalized_entries(
    root: &Path,
    entries: &[EntryBytes],
) -> Result<(), LegacyNormalizationError> {
    for entry in entries {
        let relative = validated_relative_path(&entry.entry.path)?;
        let destination = root.join(relative);
        match entry.bytes.as_deref() {
            Some(bytes) => write_private_bytes(&destination, bytes)?,
            None => create_private_directory(&destination)?,
        }
    }
    Ok(())
}

fn verify_materialized_entries(
    root: &Path,
    entries: &[EntryBytes],
) -> Result<(), LegacyNormalizationError> {
    require_private_directory(root)?;
    let expected_paths = entries
        .iter()
        .map(|entry| entry.entry.path.as_str())
        .collect::<Vec<_>>();
    let observed_paths = collect_source_paths(root)?;
    if observed_paths
        .iter()
        .map(String::as_str)
        .ne(expected_paths.iter().copied())
    {
        return Err(LegacyNormalizationError::Destination);
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let directory = options
        .open(root)
        .map_err(|_| LegacyNormalizationError::Destination)?;
    let root_metadata = directory
        .metadata()
        .map_err(|_| LegacyNormalizationError::Destination)?;
    let root_owner = root_metadata.uid();
    let root_signature = metadata_signature(&root_metadata);
    for entry in entries {
        let relative = validated_relative_path(&entry.entry.path)?;
        let mut file =
            open_source_entry_beneath(&directory, &relative, entry.entry.r#type == "directory")?;
        let metadata = file
            .metadata()
            .map_err(|_| LegacyNormalizationError::Destination)?;
        if metadata.uid() != root_owner {
            return Err(LegacyNormalizationError::Destination);
        }
        match entry.bytes.as_deref() {
            Some(expected)
                if metadata.is_file()
                    && metadata.mode() & 0o777 == 0o600
                    && metadata.len() == expected.len() as u64 =>
            {
                let mut actual = Vec::with_capacity(expected.len());
                file.read_to_end(&mut actual)
                    .map_err(|_| LegacyNormalizationError::Destination)?;
                if actual != expected {
                    return Err(LegacyNormalizationError::Destination);
                }
            }
            None if metadata.is_dir() && metadata.mode() & 0o777 == 0o700 => {}
            _ => return Err(LegacyNormalizationError::Destination),
        }
        let after = fs::symlink_metadata(root.join(&relative))
            .map_err(|_| LegacyNormalizationError::Destination)?;
        if after.file_type().is_symlink()
            || metadata_signature(&after) != metadata_signature(&metadata)
        {
            return Err(LegacyNormalizationError::Destination);
        }
    }
    let final_paths = collect_source_paths(root)?;
    let final_root =
        fs::symlink_metadata(root).map_err(|_| LegacyNormalizationError::Destination)?;
    if final_paths
        .iter()
        .map(String::as_str)
        .ne(expected_paths.iter().copied())
        || final_root.file_type().is_symlink()
        || metadata_signature(&final_root) != root_signature
    {
        return Err(LegacyNormalizationError::Destination);
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), LegacyNormalizationError> {
    fs::create_dir(path).map_err(|_| LegacyNormalizationError::Destination)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| LegacyNormalizationError::Destination)
}

fn read_control_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<T, LegacyNormalizationError> {
    let bytes = read_control_file(path, MAX_CONTROL_DOCUMENT_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|_| LegacyNormalizationError::Staging)
}

fn read_control_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, LegacyNormalizationError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(path)
        .map_err(|_| LegacyNormalizationError::Staging)?;
    let metadata = file
        .metadata()
        .map_err(|_| LegacyNormalizationError::Staging)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > max_bytes {
        return Err(LegacyNormalizationError::Staging);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| LegacyNormalizationError::Staging)?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "tests/Legacy_Normalization_test.rs"]
mod legacy_normalization_tests;
