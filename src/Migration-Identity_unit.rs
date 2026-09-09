use ac_ant_delirium::libres::Rusqlite_unit::{AC_OptionalExtension, AC_params};
use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::crockford_id;
use ac_ant_delirium::structs::{
    NewAuditEvent, ProjectDatabase, TransactionalStorageError, TransactionalStorageResult,
};

use crate::task_package_contracts::{
    validate_identity_association, validate_identity_association_receipt,
    MigrationIdentityAssociation, MigrationIdentityAssociationReceipt,
    MIGRATION_IDENTITY_ASSOCIATION_RECEIPT_V1,
};

pub(crate) fn association_receipt(
    request: &MigrationIdentityAssociation,
) -> TransactionalStorageResult<MigrationIdentityAssociationReceipt> {
    validate_identity_association(request).map_err(|_| invalid())?;
    let digest = sha256(&serde_json::to_vec(request).map_err(|_| invalid())?);
    let receipt = MigrationIdentityAssociationReceipt {
        contract_version: MIGRATION_IDENTITY_ASSOCIATION_RECEIPT_V1.to_string(),
        request_id: request.request_id.clone(),
        association_id: crockford_id("asn_", &digest[..16].try_into().map_err(|_| invalid())?),
        migration_id: request.migration_id.clone(),
        cutover_id: request.cutover_id.clone(),
        project_id: request.candidate_project_id.clone(),
        workspace_id: request.candidate_workspace_id.clone(),
        previous_registry_revision: request.expected_registry_revision,
        resulting_registry_revision: request
            .expected_registry_revision
            .checked_add(1)
            .ok_or_else(invalid)?,
        association_sha256: crate::legacy_staging::lowercase_hex(&digest),
        state: "associated".to_string(),
    };
    validate_identity_association_receipt(&receipt).map_err(|_| invalid())?;
    Ok(receipt)
}

pub(crate) struct AssociationContext<'a> {
    pub(crate) registry: &'a ProjectDatabase,
    pub(crate) provisional: &'a ProjectDatabase,
    pub(crate) candidate: &'a ProjectDatabase,
    pub(crate) peer_uid: u32,
    pub(crate) service_subject_sha256: &'a str,
}

pub(crate) fn associate_identity(
    context: AssociationContext<'_>,
    request: &MigrationIdentityAssociation,
    event: &NewAuditEvent,
) -> TransactionalStorageResult<MigrationIdentityAssociationReceipt> {
    let receipt = association_receipt(request)?;
    if event.project_id != request.candidate_project_id
        || event.workspace_id.as_deref() != Some(request.candidate_workspace_id.as_str())
        || event.actor_kind != "operator"
        || event.event_kind != "migration_applied"
        || event.outcome != "succeeded"
        || event.fingerprint != receipt.association_sha256
        || event.resource_id.as_deref() != Some(receipt.association_id.as_str())
        || event.actor_subject_id
            != crate::service_storage::task_package_subject_sha256(context.peer_uid)
    {
        return Err(invalid());
    }
    let payload = serde_json::to_vec(request).map_err(|_| invalid())?;
    let receipt_bytes = serde_json::to_vec(&receipt).map_err(|_| invalid())?;
    // Hold both canonical databases stable while committing only the registry.
    // No cross-database writes and no deletion of the provisional history.
    context.provisional.with_immediate_transaction(|source| {
        context.candidate.with_immediate_transaction(|candidate| {
            context.registry.with_immediate_transaction(|registry| {
                let previous: Option<(u32, Vec<u8>, Vec<u8>)> = registry.query_row(
                    "SELECT peer_uid, payload, receipt FROM migration_identity_associations WHERE request_id = ?1 OR provisional_project_id = ?2 OR candidate_project_id = ?3",
                    AC_params![request.request_id, request.provisional_project_id, request.candidate_project_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                ).optional()?;
                if let Some(previous) = previous {
                    if previous != (context.peer_uid, payload.clone(), receipt_bytes.clone()) { return Err(invalid()); }
                    let rolled_back: bool = registry.query_row("SELECT EXISTS(SELECT 1 FROM migration_identity_rollbacks WHERE association_id = ?1)",
                        AC_params![receipt.association_id], |row| row.get(0))?;
                    if rolled_back { return Err(invalid()); }
                    // A retry is a receipt read, not a second cutover or rewind.
                    return Ok(receipt.clone());
                }
                let revision: i64 = registry.query_row(
                    "SELECT revision FROM migration_identity_registry_revision WHERE singleton = 1", [], |row| row.get(0),
                )?;
                if revision != request.expected_registry_revision { return Err(invalid()); }
                let registration: (Vec<u8>, Vec<u8>) = registry.query_row(
                    "SELECT project_identity_fingerprint, workspace_identity_fingerprint FROM provisional_project_registrations WHERE peer_uid = ?1 AND project_id = ?2 AND workspace_id = ?3",
                    AC_params![context.peer_uid, request.provisional_project_id, request.provisional_workspace_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                if crate::legacy_staging::lowercase_hex(&registration.0) != request.project_identity_sha256
                    || crate::legacy_staging::lowercase_hex(&registration.1) != request.workspace_identity_sha256
                { return Err(invalid()); }
                // A single-workspace request cannot retire other worktrees or
                // identities. They require an explicit reconciliation policy.
                let registrations: i64 = registry.query_row(
                    "SELECT count(*) FROM provisional_project_registrations WHERE project_id IN (?1, ?2)",
                    AC_params![request.provisional_project_id, request.candidate_project_id], |row| row.get(0),
                )?;
                let identities: i64 = registry.query_row(
                    "SELECT count(*) FROM provisional_project_identities WHERE project_id IN (?1, ?2)",
                    AC_params![request.provisional_project_id, request.candidate_project_id], |row| row.get(0),
                )?;
                if registrations != 1 || identities != 1 { return Err(invalid()); }
                for (project, workspace, db) in [
                    (&request.provisional_project_id, &request.provisional_workspace_id, source),
                    (&request.candidate_project_id, &request.candidate_workspace_id, candidate),
                ] {
                    let actual: (String, String) = db.query_row(
                        "SELECT p.project_key, w.workspace_key FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
                        AC_params![project, workspace], |row| Ok((row.get(0)?, row.get(1)?)),
                    )?;
                    let registered: (String, String) = registry.query_row(
                        "SELECT p.project_key, w.workspace_key FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
                        AC_params![project, workspace], |row| Ok((row.get(0)?, row.get(1)?)),
                    )?;
                    if actual != registered { return Err(invalid()); }
                }
                let empty: bool = source.query_row(
                    "SELECT state_revision = 0 AND active_plan_id IS NULL AND current_task_id IS NULL AND NOT EXISTS(SELECT 1 FROM plans WHERE project_id = ?1) AND NOT EXISTS(SELECT 1 FROM tasks WHERE project_id = ?1) AND NOT EXISTS(SELECT 1 FROM submissions WHERE project_id = ?1) AND NOT EXISTS(SELECT 1 FROM reviews WHERE project_id = ?1) AND NOT EXISTS(SELECT 1 FROM legacy_migration_records WHERE project_id = ?1) AND (SELECT count(*) FROM workspaces WHERE project_id = ?1) = 1 FROM projects WHERE project_id = ?1",
                    AC_params![request.provisional_project_id], |row| row.get(0),
                )?;
                if request.provisional_state_policy == "require-empty" && !empty { return Err(invalid()); }
                let workspace_count: i64 = source.query_row("SELECT count(*) FROM workspaces WHERE project_id = ?1", AC_params![request.provisional_project_id], |row| row.get(0))?;
                if workspace_count != 1 { return Err(invalid()); }
                crate::legacy_cutover::validate_association_cutover(candidate, request, context.service_subject_sha256, true)?;
                crate::legacy_cutover::validate_association_cutover(registry, request, context.service_subject_sha256, false)?;
                crate::migration_identity_recovery::preserve_databases(&context, registry, &receipt.association_id)?;
                crate::migration_identity_recovery::revoke_scope_capabilities(registry, &request.provisional_project_id, &receipt.association_id, &event.occurred_at)?;
                if !context.registry.record_event_in_transaction(registry, event)?.inserted() { return Err(invalid()); }
                registry.execute(
                    "INSERT INTO migration_identity_associations(association_id, request_id, peer_uid, project_identity_fingerprint, workspace_identity_fingerprint, provisional_project_id, provisional_workspace_id, candidate_project_id, candidate_workspace_id, event_id, payload, receipt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    AC_params![receipt.association_id, request.request_id, context.peer_uid, registration.0, registration.1, request.provisional_project_id, request.provisional_workspace_id, request.candidate_project_id, request.candidate_workspace_id, event.event_id, payload, receipt_bytes],
                )?;
                if registry.execute(
                    "UPDATE migration_identity_registry_revision SET revision = ?1 WHERE singleton = 1 AND revision = ?2",
                    AC_params![receipt.resulting_registry_revision, request.expected_registry_revision],
                )? != 1 { return Err(invalid()); }
                Ok(receipt.clone())
            })
        })
    })
}

fn invalid() -> TransactionalStorageError {
    TransactionalStorageError::Integrity("migration identity association conflicts".to_string())
}

#[cfg(test)]
#[path = "tests/Migration_Identity_test.rs"]
pub(crate) mod tests;
