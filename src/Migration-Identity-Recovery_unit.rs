use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

use ac_ant_delirium::libres::Rusqlite_unit::{
    AC_Backup, AC_Connection, AC_OpenFlags, AC_OptionalExtension, AC_Transaction, AC_params,
};
use ac_ant_delirium::libres::Sha2_unit::{sha256, sha256_reader};
use ac_ant_delirium::libres::System_Random_unit::{crockford_id, random_crockford_id};
use ac_ant_delirium::structs::{
    NewAuditEvent, TransactionalStorageError, TransactionalStorageResult,
};

use crate::legacy_staging::{lowercase_hex, sync_directory, write_private_bytes};
use crate::migration_identity::AssociationContext;
use crate::task_package_contracts::{
    validate_identity_rollback, validate_identity_rollback_receipt, MigrationIdentityAssociation,
    MigrationIdentityRollback, MigrationIdentityRollbackReceipt,
    MIGRATION_IDENTITY_ROLLBACK_RECEIPT_V1,
};

pub(crate) fn rollback_receipt(
    request: &MigrationIdentityRollback,
) -> TransactionalStorageResult<MigrationIdentityRollbackReceipt> {
    validate_identity_rollback(request).map_err(|_| invalid())?;
    let digest = sha256(&serde_json::to_vec(request).map_err(|_| invalid())?);
    let receipt = MigrationIdentityRollbackReceipt {
        contract_version: MIGRATION_IDENTITY_ROLLBACK_RECEIPT_V1.to_string(),
        request_id: request.request_id.clone(),
        rollback_id: crockford_id("rbk_", &digest[..16].try_into().map_err(|_| invalid())?),
        association_id: request.association_id.clone(),
        project_id: request.provisional_project_id.clone(),
        workspace_id: request.provisional_workspace_id.clone(),
        preserved_candidate_project_id: request.candidate_project_id.clone(),
        preserved_candidate_workspace_id: request.candidate_workspace_id.clone(),
        previous_registry_revision: request.expected_registry_revision,
        resulting_registry_revision: request
            .expected_registry_revision
            .checked_add(1)
            .ok_or_else(invalid)?,
        rollback_sha256: lowercase_hex(&digest),
        state: "rolled_back".to_string(),
    };
    validate_identity_rollback_receipt(&receipt).map_err(|_| invalid())?;
    Ok(receipt)
}

pub(crate) fn rollback_identity(
    context: AssociationContext<'_>,
    request: &MigrationIdentityRollback,
    event: &NewAuditEvent,
) -> TransactionalStorageResult<MigrationIdentityRollbackReceipt> {
    let receipt = rollback_receipt(request)?;
    if event.project_id != request.candidate_project_id
        || event.workspace_id.as_deref() != Some(request.candidate_workspace_id.as_str())
        || event.actor_kind != "operator"
        || event.event_kind != "migration_applied"
        || event.outcome != "succeeded"
        || event.resource_id.as_deref() != Some(receipt.rollback_id.as_str())
        || event.fingerprint != receipt.rollback_sha256
        || event.actor_subject_id
            != crate::service_storage::task_package_subject_sha256(context.peer_uid)
    {
        return Err(invalid());
    }
    let payload = serde_json::to_vec(request).map_err(|_| invalid())?;
    let receipt_bytes = serde_json::to_vec(&receipt).map_err(|_| invalid())?;
    context.provisional.with_immediate_transaction(|source| {
        context.candidate.with_immediate_transaction(|candidate| {
            context.registry.with_immediate_transaction(|registry| {
                let previous: Option<(u32, Vec<u8>, Vec<u8>)> = registry.query_row(
                    "SELECT peer_uid, payload, receipt FROM migration_identity_rollbacks WHERE request_id = ?1 OR association_id = ?2",
                    AC_params![request.request_id, request.association_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                ).optional()?;
                if let Some(previous) = previous {
                    if previous != (context.peer_uid, payload.clone(), receipt_bytes.clone()) { return Err(invalid()); }
                    return Ok(receipt.clone());
                }
                let stored: (u32, Vec<u8>) = registry.query_row(
                    "SELECT peer_uid, payload FROM active_migration_identity_associations WHERE association_id = ?1",
                    AC_params![request.association_id], |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                let association: MigrationIdentityAssociation = serde_json::from_slice(&stored.1).map_err(|_| invalid())?;
                let associated = crate::migration_identity::association_receipt(&association)?;
                if stored.0 != context.peer_uid || associated.association_id != request.association_id
                    || associated.association_sha256 != request.association_sha256
                    || association.provisional_project_id != request.provisional_project_id || association.provisional_workspace_id != request.provisional_workspace_id
                    || association.candidate_project_id != request.candidate_project_id || association.candidate_workspace_id != request.candidate_workspace_id {
                    return Err(invalid());
                }
                let revision: i64 = registry.query_row("SELECT revision FROM migration_identity_registry_revision WHERE singleton = 1", [], |row| row.get(0))?;
                if revision != request.expected_registry_revision { return Err(invalid()); }
                let fingerprints: (String, String) = registry.query_row(
                    "SELECT lower(hex(project_identity_fingerprint)), lower(hex(workspace_identity_fingerprint)) FROM provisional_project_registrations WHERE peer_uid = ?1 AND project_id = ?2 AND workspace_id = ?3",
                    AC_params![context.peer_uid, request.provisional_project_id, request.provisional_workspace_id], |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                if fingerprints != (association.project_identity_sha256.clone(), association.workspace_identity_sha256.clone()) { return Err(invalid()); }
                // Only the binding is restored: no old task, review, snapshot,
                // event sequence or Git history is copied over a live database.
                for (db, project, workspace) in [
                    (source, &request.provisional_project_id, &request.provisional_workspace_id),
                    (candidate, &request.candidate_project_id, &request.candidate_workspace_id),
                ] {
                    let actual: (String, String) = db.query_row(
                        "SELECT p.project_key, w.workspace_key FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
                        AC_params![project, workspace], |row| Ok((row.get(0)?, row.get(1)?)),
                    )?;
                    let expected: (String, String) = registry.query_row(
                        "SELECT p.project_key, w.workspace_key FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
                        AC_params![project, workspace], |row| Ok((row.get(0)?, row.get(1)?)),
                    )?;
                    if actual != expected { return Err(invalid()); }
                }
                preserve_databases(&context, registry, &receipt.rollback_id)?;
                revoke_scope_capabilities(registry, &request.provisional_project_id, &receipt.rollback_id, &event.occurred_at)?;
                revoke_scope_capabilities(registry, &request.candidate_project_id, &receipt.rollback_id, &event.occurred_at)?;
                if !context.registry.record_event_in_transaction(registry, event)?.inserted() { return Err(invalid()); }
                registry.execute(
                    "INSERT INTO migration_identity_rollbacks(rollback_id, request_id, association_id, peer_uid, event_id, payload, receipt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    AC_params![receipt.rollback_id, request.request_id, request.association_id, context.peer_uid, event.event_id, payload, receipt_bytes],
                )?;
                if registry.execute("UPDATE migration_identity_registry_revision SET revision = ?1 WHERE singleton = 1 AND revision = ?2",
                    AC_params![receipt.resulting_registry_revision, request.expected_registry_revision])? != 1 { return Err(invalid()); }
                Ok(receipt.clone())
            })
        })
    })
}

pub(crate) fn revoke_scope_capabilities(
    transaction: &AC_Transaction<'_>,
    project: &str,
    operation_id: &str,
    occurred_at: &str,
) -> TransactionalStorageResult<()> {
    let mut statement = transaction.prepare(
        "SELECT grant_id, revocation_epoch FROM capability_grants g WHERE project_id = ?1 AND NOT EXISTS(SELECT 1 FROM revocations r WHERE r.grant_id = g.grant_id)")?;
    let grants = statement
        .query_map(AC_params![project], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (grant, epoch) in grants {
        let digest =
            sha256(format!("identity-scope-revocation-v1\0{operation_id}\0{grant}").as_bytes());
        let id = crockford_id("rev_", &digest[..16].try_into().map_err(|_| invalid())?);
        transaction.execute("INSERT INTO revocations(revocation_id, grant_id, revocation_epoch, revoked_at, reason) VALUES (?1, ?2, ?3, ?4, 'identity scope changed; fresh authority required')",
            AC_params![id, grant, epoch, occurred_at])?;
    }
    Ok(())
}

// Caller holds BEGIN IMMEDIATE on all three databases before this function.
// Independent readers therefore copy their exact pre-operation committed state,
// including WAL contents. Snapshots are sealed before the registry can change.
pub(crate) fn preserve_databases(
    context: &AssociationContext<'_>,
    registry: &AC_Transaction<'_>,
    operation_id: &str,
) -> TransactionalStorageResult<()> {
    let registry_root = context
        .registry
        .database_path()
        .parent()
        .and_then(Path::parent)
        .ok_or_else(invalid)?;
    require_private_directory(registry_root)?;
    let parent = registry_root.join("identity-backups");
    match fs::DirBuilder::new().mode(0o700).create(&parent) {
        Ok(()) => sync_directory(registry_root).map_err(|_| invalid())?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(invalid()),
    }
    require_private_directory(&parent)?;
    let snapshot_id = random_crockford_id("bkp_").map_err(|_| invalid())?;
    let root = parent.join(&snapshot_id);
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .map_err(|_| invalid())?;
    let mut copies = Vec::new();
    for (name, database) in [
        ("registry", context.registry),
        ("provisional", context.provisional),
        ("candidate", context.candidate),
    ] {
        let path = root.join(format!("{name}.sqlite3"));
        write_private_bytes(&path, &[]).map_err(|_| invalid())?;
        let source = AC_Connection::open_with_flags(
            database.database_path(),
            AC_OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        source.busy_timeout(Duration::from_secs(5))?;
        let mut destination =
            AC_Connection::open_with_flags(&path, AC_OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        destination.execute_batch("PRAGMA synchronous = FULL;")?;
        AC_Backup::new(&source, &mut destination)?.run_to_completion(
            128,
            Duration::from_millis(5),
            None,
        )?;
        let valid: String = destination.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if valid != "ok" {
            return Err(invalid());
        }
        destination
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode = DELETE;")?;
        drop(destination);
        let file = fs::File::open(&path).map_err(|_| invalid())?;
        file.sync_all().map_err(|_| invalid())?;
        let metadata = file.metadata().map_err(|_| invalid())?;
        if metadata.mode() & 0o777 != 0o600 || metadata.nlink() != 1 {
            return Err(invalid());
        }
        copies.push(serde_json::json!({"scope": name, "sha256": lowercase_hex(&sha256_reader(file).map_err(|_| invalid())?), "bytes": metadata.len()}));
    }
    let manifest = serde_json::to_vec(&serde_json::json!({"contract_version": "identity-recovery-snapshot-v1", "operation_id": operation_id, "snapshot_id": snapshot_id, "databases": copies})).map_err(|_| invalid())?;
    write_private_bytes(&root.join("manifest.json"), &manifest).map_err(|_| invalid())?;
    sync_directory(&root).map_err(|_| invalid())?;
    sync_directory(&parent).map_err(|_| invalid())?;
    registry.execute("INSERT INTO migration_identity_recovery_points(operation_id, snapshot_id, manifest_sha256) VALUES (?1, ?2, ?3)",
        AC_params![operation_id, snapshot_id, lowercase_hex(&sha256(&manifest))])?;
    Ok(())
}

fn require_private_directory(path: &Path) -> TransactionalStorageResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o700
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> TransactionalStorageError {
    TransactionalStorageError::Integrity(
        "identity recovery binding or preserved state is invalid".to_string(),
    )
}
