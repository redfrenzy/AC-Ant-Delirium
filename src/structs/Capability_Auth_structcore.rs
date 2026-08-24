use std::collections::HashSet;

use crate::libres::Rusqlite_unit::{AC_OptionalExtension, AC_Transaction, AC_params};
use crate::libres::Sha2_unit::{constant_time_equal, sha256};
use crate::libres::System_Random_unit::random_crockford_id;

use super::{
    AuthorizedCapability, CapabilityActor, CapabilityAuthorizationOutcome,
    CapabilityAuthorizationRequest, CapabilityChannel, CapabilityGrantAuditReceipt,
    CapabilityOperation, CapabilityRevocationAuditReceipt, InitialCapabilityProvisioning,
    InitialCapabilityProvisioningReceipt, NewCapabilityGrant, NewCapabilityRevocation,
    NewServiceSecurityEvent, ProjectDatabase, ServiceSecurityAuditReceipt,
    ServiceSecurityEventKind, TransactionalStorageError, TransactionalStorageResult,
};

type PersistedGrant = (
    Vec<u8>,
    i64,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    i64,
    i64,
);

impl ProjectDatabase {
    pub fn store_capability_grant(
        &self,
        grant: &NewCapabilityGrant,
    ) -> TransactionalStorageResult<()> {
        validate_grant(grant)?;
        self.with_immediate_transaction(|transaction| {
            insert_grant_in_transaction(transaction, grant)?;
            Ok(())
        })
    }

    pub fn provision_initial_capabilities(
        &self,
        provisioning: &InitialCapabilityProvisioning,
    ) -> TransactionalStorageResult<InitialCapabilityProvisioningReceipt> {
        validate_initial_provisioning(provisioning)?;
        self.with_immediate_transaction(|transaction| {
            let existing_receipts: i64 = transaction.query_row(
                "SELECT count(*) FROM initial_capability_provisioning",
                [],
                |row| row.get(0),
            )?;
            let existing_grants: i64 = transaction.query_row(
                "SELECT count(*) FROM capability_grants",
                [],
                |row| row.get(0),
            )?;
            let existing_authoritative_state: i64 = transaction.query_row(
                "SELECT (SELECT count(*) FROM projects) + (SELECT count(*) FROM service_security_events) + (SELECT count(*) FROM capability_management_events) + (SELECT count(*) FROM capability_subject_epochs) + (SELECT count(*) FROM revocations) + (SELECT count(*) FROM capability_request_receipts) + (SELECT count(*) FROM provisional_project_registrations) + (SELECT count(*) FROM legacy_migration_records)",
                [],
                |row| row.get(0),
            )?;
            if existing_receipts != 0 || existing_grants != 0 || existing_authoritative_state != 0 {
                return Err(TransactionalStorageError::Integrity(
                    "initial capability provisioning is no longer available".to_string(),
                ));
            }
            for grant in &provisioning.grants {
                insert_grant_in_transaction(transaction, grant)?;
            }
            let grant_count = i64::try_from(provisioning.grants.len())
                .map_err(|_| invalid_provisioning())?;
            transaction.execute(
                "INSERT INTO initial_capability_provisioning(singleton, provisioning_id, manifest_sha256, applied_at_unix, grant_count) VALUES (1, ?1, ?2, ?3, ?4)",
                AC_params![
                    provisioning.provisioning_id,
                    provisioning.manifest_sha256.as_slice(),
                    provisioning.applied_at_unix,
                    grant_count,
                ],
            )?;
            Ok(InitialCapabilityProvisioningReceipt {
                provisioning_id: provisioning.provisioning_id.clone(),
                manifest_sha256: provisioning.manifest_sha256,
                applied_at_unix: provisioning.applied_at_unix,
                grant_count: provisioning.grants.len(),
            })
        })
    }

    pub fn initial_capability_provisioning_receipt(
        &self,
    ) -> TransactionalStorageResult<Option<InitialCapabilityProvisioningReceipt>> {
        let connection = self.open_connection(false)?;
        connection
            .query_row(
                "SELECT provisioning_id, manifest_sha256, applied_at_unix, grant_count FROM initial_capability_provisioning WHERE singleton = 1",
                [],
                |row| {
                    let manifest_sha256: Vec<u8> = row.get(1)?;
                    let grant_count: i64 = row.get(3)?;
                    Ok((row.get::<_, String>(0)?, manifest_sha256, row.get::<_, i64>(2)?, grant_count))
                },
            )
            .optional()?
            .map(|(provisioning_id, digest, applied_at_unix, grant_count)| {
                let manifest_sha256: [u8; 32] = digest.try_into().map_err(|_| {
                    TransactionalStorageError::Integrity(
                        "initial provisioning digest is invalid".to_string(),
                    )
                })?;
                let grant_count = usize::try_from(grant_count).map_err(|_| {
                    TransactionalStorageError::Integrity(
                        "initial provisioning grant count is invalid".to_string(),
                    )
                })?;
                Ok(InitialCapabilityProvisioningReceipt {
                    provisioning_id,
                    manifest_sha256,
                    applied_at_unix,
                    grant_count,
                })
            })
            .transpose()
    }

    pub fn revoke_capability(
        &self,
        revocation: &NewCapabilityRevocation,
    ) -> TransactionalStorageResult<()> {
        validate_opaque_id(&revocation.revocation_id, "rev_")?;
        validate_opaque_id(&revocation.grant_id, "cap_")?;
        validate_nonempty(&revocation.revoked_at)?;
        validate_nonempty(&revocation.reason)?;
        let epoch = i64::try_from(revocation.revocation_epoch).map_err(|_| invalid_grant())?;
        self.with_immediate_transaction(|transaction| {
            let grant_epoch: i64 = transaction.query_row(
                "SELECT revocation_epoch FROM capability_grants WHERE grant_id = ?1",
                AC_params![revocation.grant_id],
                |row| row.get(0),
            )?;
            if grant_epoch != epoch {
                return Err(invalid_grant());
            }
            transaction.execute(
                "INSERT INTO revocations(revocation_id, grant_id, revocation_epoch, revoked_at, reason) VALUES (?1, ?2, ?3, ?4, ?5)",
                AC_params![
                    revocation.revocation_id,
                    revocation.grant_id,
                    epoch,
                    revocation.revoked_at,
                    revocation.reason,
                ],
            )?;
            Ok(())
        })
    }

    pub fn grant_managed_capability(
        &self,
        operator_uid: u32,
        grant: &NewCapabilityGrant,
        trusted_now_unix: i64,
    ) -> TransactionalStorageResult<CapabilityGrantAuditReceipt> {
        validate_managed_grant(operator_uid, grant, trusted_now_unix)?;
        self.with_immediate_transaction(|transaction| {
            insert_grant_in_transaction(transaction, grant)?;
            let audit = append_capability_management_event(
                transaction,
                "grant",
                operator_uid,
                grant,
                trusted_now_unix,
            )?;
            Ok(CapabilityGrantAuditReceipt {
                grant_id: grant.grant_id.clone(),
                audit_sequence: audit.sequence,
                event_id: audit.event_id,
                correlation_id: audit.correlation_id,
            })
        })
    }

    pub fn revoke_managed_capability(
        &self,
        operator_uid: u32,
        grant_id: &str,
        revocation_epoch: u64,
        trusted_now_unix: i64,
    ) -> TransactionalStorageResult<CapabilityRevocationAuditReceipt> {
        validate_opaque_id(grant_id, "cap_")?;
        if operator_uid == 0 || trusted_now_unix < 0 {
            return Err(invalid_grant());
        }
        let epoch = i64::try_from(revocation_epoch).map_err(|_| invalid_grant())?;
        self.with_immediate_transaction(|transaction| {
            let persisted: PersistedGrant = transaction.query_row(
                "SELECT secret_hash, subject_uid, actor_kind, channel_kind, operation, project_id, workspace_id, task_id, not_before_unix, expires_at_unix, revocation_epoch FROM capability_grants WHERE grant_id = ?1",
                AC_params![grant_id],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                        row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                        row.get(10)?,
                    ))
                },
            )?;
            let grant = managed_grant_from_persisted(grant_id, &persisted)?;
            validate_managed_grant_policy(operator_uid, &grant)?;
            if persisted.10 != epoch {
                return Err(invalid_grant());
            }
            let revocation_id = random_security_id("rev_")?;
            transaction.execute(
                "INSERT INTO revocations(revocation_id, grant_id, revocation_epoch, revoked_at, reason) VALUES (?1, ?2, ?3, ?4, 'operator-request')",
                AC_params![
                    revocation_id,
                    grant_id,
                    epoch,
                    format!("unix:{trusted_now_unix}"),
                ],
            )?;
            let audit = append_capability_management_event(
                transaction,
                "revoke",
                operator_uid,
                &grant,
                trusted_now_unix,
            )?;
            Ok(CapabilityRevocationAuditReceipt {
                grant_id: grant_id.to_string(),
                revocation_id,
                audit_sequence: audit.sequence,
                event_id: audit.event_id,
                correlation_id: audit.correlation_id,
            })
        })
    }

    pub fn capability_management_event_count(&self) -> TransactionalStorageResult<i64> {
        let connection = self.open_connection(false)?;
        Ok(connection.query_row(
            "SELECT count(*) FROM capability_management_events",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn advance_capability_epoch(
        &self,
        subject_uid: u32,
        actor: CapabilityActor,
        expected_epoch: u64,
        next_epoch: u64,
    ) -> TransactionalStorageResult<()> {
        if next_epoch <= expected_epoch {
            return Err(invalid_grant());
        }
        let expected_epoch = i64::try_from(expected_epoch).map_err(|_| invalid_grant())?;
        let next_epoch = i64::try_from(next_epoch).map_err(|_| invalid_grant())?;
        self.with_immediate_transaction(|transaction| {
            let updated = transaction.execute(
                "UPDATE capability_subject_epochs SET epoch = ?3 WHERE subject_uid = ?1 AND actor_kind = ?2 AND epoch = ?4",
                AC_params![
                    i64::from(subject_uid),
                    actor.storage_value(),
                    next_epoch,
                    expected_epoch,
                ],
            )?;
            if updated != 1 {
                return Err(invalid_grant());
            }
            Ok(())
        })
    }

    pub fn authorize_capability(
        &self,
        trusted_peer_uid: u32,
        trusted_channel: CapabilityChannel,
        expected_connection_nonce: &[u8; 32],
        trusted_now_unix: i64,
        request: &CapabilityAuthorizationRequest,
    ) -> TransactionalStorageResult<CapabilityAuthorizationOutcome> {
        if trusted_now_unix < 0 {
            return Err(TransactionalStorageError::Integrity(
                "trusted service clock is invalid".to_string(),
            ));
        }
        let denial_event =
            authorization_denied_event(trusted_peer_uid, trusted_channel, trusted_now_unix)?;
        self.with_immediate_transaction(|transaction| {
            if expected_connection_nonce.iter().all(|byte| *byte == 0)
                || !valid_authorization_request(request)
            {
                return self.deny_and_audit(transaction, &denial_event);
            }
            let presented_hash = sha256(&request.presented_secret);
            let persisted = transaction
                .query_row(
                    "SELECT secret_hash, subject_uid, actor_kind, channel_kind, operation, project_id, workspace_id, task_id, not_before_unix, expires_at_unix, revocation_epoch FROM capability_grants WHERE grant_id = ?1",
                    AC_params![request.grant_id],
                    |row| {
                        Ok((
                            row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                            row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                            row.get(10)?,
                        ))
                    },
                )
                .optional()?;
            let Some(persisted) = persisted else {
                std::hint::black_box(constant_time_equal(&presented_hash, &[0_u8; 32]));
                return self.deny_and_audit(transaction, &denial_event);
            };
            let Some(authorized) = authorize_persisted_grant(
                trusted_peer_uid,
                trusted_channel,
                expected_connection_nonce,
                trusted_now_unix,
                &presented_hash,
                request,
                &persisted,
            ) else {
                return self.deny_and_audit(transaction, &denial_event);
            };

            let epoch: Option<i64> = transaction
                .query_row(
                    "SELECT epoch FROM capability_subject_epochs WHERE subject_uid = ?1 AND actor_kind = ?2",
                    AC_params![i64::from(trusted_peer_uid), authorized.actor.storage_value()],
                    |row| row.get(0),
                )
                .optional()?;
            if epoch != Some(i64::try_from(authorized.revocation_epoch).unwrap_or(-1)) {
                return self.deny_and_audit(transaction, &denial_event);
            }
            let revoked: i64 = transaction.query_row(
                "SELECT count(*) FROM revocations WHERE grant_id = ?1",
                AC_params![request.grant_id],
                |row| row.get(0),
            )?;
            if revoked != 0 {
                return self.deny_and_audit(transaction, &denial_event);
            }

            let existing: Option<Vec<u8>> = transaction
                .query_row(
                    "SELECT request_fingerprint FROM capability_request_receipts WHERE grant_id = ?1 AND request_id = ?2",
                    AC_params![request.grant_id, request.request_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(existing) = existing {
                if constant_time_equal(&existing, &request.request_fingerprint) {
                    return Ok(CapabilityAuthorizationOutcome::Replay(authorized));
                }
                return self.deny_and_audit(transaction, &denial_event);
            }
            transaction.execute(
                "INSERT INTO capability_request_receipts(grant_id, request_id, request_fingerprint, authorized_at_unix) VALUES (?1, ?2, ?3, ?4)",
                AC_params![
                    request.grant_id,
                    request.request_id,
                    request.request_fingerprint.as_slice(),
                    trusted_now_unix,
                ],
            )?;
            Ok(CapabilityAuthorizationOutcome::Fresh(authorized))
        })
    }

    pub fn record_authorized_request_rejection(
        &self,
        capability: &AuthorizedCapability,
        trusted_now_unix: i64,
    ) -> TransactionalStorageResult<ServiceSecurityAuditReceipt> {
        if trusted_now_unix < 0 {
            return Err(TransactionalStorageError::Integrity(
                "trusted service clock is invalid".to_string(),
            ));
        }
        let event = request_rejected_event(capability, trusted_now_unix)?;
        self.with_immediate_transaction(|transaction| {
            self.record_service_security_event_in_transaction(transaction, &event)
        })
    }

    pub fn service_security_event_count(&self) -> TransactionalStorageResult<i64> {
        let connection = self.open_connection(false)?;
        Ok(
            connection.query_row("SELECT count(*) FROM service_security_events", [], |row| {
                row.get(0)
            })?,
        )
    }

    pub fn authorized_capability_is_current(
        &self,
        capability: &AuthorizedCapability,
        trusted_now_unix: i64,
    ) -> TransactionalStorageResult<bool> {
        if trusted_now_unix < 0 {
            return Ok(false);
        }
        let connection = self.open_connection(false)?;
        let persisted: Option<PersistedGrant> = connection
            .query_row(
                "SELECT secret_hash, subject_uid, actor_kind, channel_kind, operation, project_id, workspace_id, task_id, not_before_unix, expires_at_unix, revocation_epoch FROM capability_grants WHERE grant_id = ?1",
                AC_params![capability.grant_id],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                        row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                        row.get(10)?,
                    ))
                },
            )
            .optional()?;
        let Some(persisted) = persisted else {
            return Ok(false);
        };
        let Some(actor) = CapabilityActor::from_storage(&persisted.2) else {
            return Ok(false);
        };
        let Some(channel) = CapabilityChannel::from_storage(&persisted.3) else {
            return Ok(false);
        };
        let Some(operation) = CapabilityOperation::from_storage(&persisted.4) else {
            return Ok(false);
        };
        let Ok(subject_uid) = u32::try_from(persisted.1) else {
            return Ok(false);
        };
        let Ok(revocation_epoch) = u64::try_from(persisted.10) else {
            return Ok(false);
        };
        if subject_uid != capability.subject_uid
            || actor != capability.actor
            || channel != capability.channel
            || operation != capability.operation
            || persisted.5 != capability.project_id
            || persisted.6 != capability.workspace_id
            || persisted.7 != capability.task_id
            || revocation_epoch != capability.revocation_epoch
            || trusted_now_unix < persisted.8
            || trusted_now_unix >= persisted.9
            || !actor_operation_allowed(actor, channel, operation)
        {
            return Ok(false);
        }
        let current_epoch: Option<i64> = connection
            .query_row(
                "SELECT epoch FROM capability_subject_epochs WHERE subject_uid = ?1 AND actor_kind = ?2",
                AC_params![i64::from(subject_uid), actor.storage_value()],
                |row| row.get(0),
            )
            .optional()?;
        let revocation_count: i64 = connection.query_row(
            "SELECT count(*) FROM revocations WHERE grant_id = ?1",
            AC_params![capability.grant_id],
            |row| row.get(0),
        )?;
        let receipt_count: i64 = connection.query_row(
            "SELECT count(*) FROM capability_request_receipts WHERE grant_id = ?1 AND request_id = ?2",
            AC_params![capability.grant_id, capability.request_id],
            |row| row.get(0),
        )?;
        Ok(current_epoch == Some(persisted.10) && revocation_count == 0 && receipt_count == 1)
    }

    fn deny_and_audit(
        &self,
        transaction: &AC_Transaction<'_>,
        event: &NewServiceSecurityEvent,
    ) -> TransactionalStorageResult<CapabilityAuthorizationOutcome> {
        self.record_service_security_event_in_transaction(transaction, event)?;
        Ok(CapabilityAuthorizationOutcome::Denied)
    }

    fn record_service_security_event_in_transaction(
        &self,
        transaction: &AC_Transaction<'_>,
        event: &NewServiceSecurityEvent,
    ) -> TransactionalStorageResult<ServiceSecurityAuditReceipt> {
        validate_service_security_event(event)?;
        let last_sequence: i64 = transaction.query_row(
            "SELECT last_sequence FROM service_security_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        let sequence = last_sequence.checked_add(1).ok_or_else(|| {
            TransactionalStorageError::Integrity(
                "service security audit sequence exhausted its integer range".to_string(),
            )
        })?;
        transaction.execute(
            "INSERT INTO service_security_events(sequence, event_id, correlation_id, event_kind, outcome, peer_uid, channel_kind, actor_kind, operation, project_id, workspace_id, task_id, occurred_at_unix) VALUES (?1, ?2, ?3, ?4, 'rejected', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            AC_params![
                sequence,
                event.event_id,
                event.correlation_id,
                event.event_kind.storage_value(),
                i64::from(event.peer_uid),
                event.channel.storage_value(),
                event.actor.map(CapabilityActor::storage_value),
                event.operation.map(CapabilityOperation::storage_value),
                event.project_id,
                event.workspace_id,
                event.task_id,
                event.occurred_at_unix,
            ],
        )?;
        let updated = transaction.execute(
            "UPDATE service_security_state SET last_sequence = ?1 WHERE singleton = 1 AND last_sequence = ?2",
            AC_params![sequence, last_sequence],
        )?;
        if updated != 1 {
            return Err(TransactionalStorageError::Integrity(
                "service security audit sequence changed during append".to_string(),
            ));
        }
        Ok(ServiceSecurityAuditReceipt {
            sequence,
            event_id: event.event_id.clone(),
            correlation_id: event.correlation_id.clone(),
        })
    }
}

fn authorization_denied_event(
    peer_uid: u32,
    channel: CapabilityChannel,
    occurred_at_unix: i64,
) -> TransactionalStorageResult<NewServiceSecurityEvent> {
    Ok(NewServiceSecurityEvent {
        event_id: random_security_id("evt_")?,
        correlation_id: random_security_id("ref_")?,
        event_kind: ServiceSecurityEventKind::AuthorizationDenied,
        peer_uid,
        channel,
        actor: None,
        operation: None,
        project_id: None,
        workspace_id: None,
        task_id: None,
        occurred_at_unix,
    })
}

fn request_rejected_event(
    capability: &AuthorizedCapability,
    occurred_at_unix: i64,
) -> TransactionalStorageResult<NewServiceSecurityEvent> {
    Ok(NewServiceSecurityEvent {
        event_id: random_security_id("evt_")?,
        correlation_id: random_security_id("ref_")?,
        event_kind: ServiceSecurityEventKind::RequestRejected,
        peer_uid: capability.subject_uid,
        channel: capability.channel,
        actor: Some(capability.actor),
        operation: Some(capability.operation),
        project_id: capability.project_id.clone(),
        workspace_id: capability.workspace_id.clone(),
        task_id: capability.task_id.clone(),
        occurred_at_unix,
    })
}

fn random_security_id(prefix: &str) -> TransactionalStorageResult<String> {
    random_crockford_id(prefix).map_err(|_| {
        TransactionalStorageError::Integrity("system entropy is unavailable".to_string())
    })
}

fn validate_service_security_event(
    event: &NewServiceSecurityEvent,
) -> TransactionalStorageResult<()> {
    validate_opaque_id(&event.event_id, "evt_")?;
    validate_opaque_id(&event.correlation_id, "ref_")?;
    if event.occurred_at_unix < 0
        || !valid_optional_id(event.project_id.as_deref(), "prj_")
        || !valid_optional_id(event.workspace_id.as_deref(), "wsp_")
        || !valid_optional_id(event.task_id.as_deref(), "tsk_")
        || (event.workspace_id.is_some() && event.project_id.is_none())
        || (event.task_id.is_some() && (event.project_id.is_none() || event.workspace_id.is_none()))
    {
        return Err(invalid_grant());
    }
    match event.event_kind {
        ServiceSecurityEventKind::AuthorizationDenied
            if event.actor.is_none()
                && event.operation.is_none()
                && event.project_id.is_none()
                && event.workspace_id.is_none()
                && event.task_id.is_none() =>
        {
            Ok(())
        }
        ServiceSecurityEventKind::RequestRejected
            if event.actor.is_some() && event.operation.is_some() =>
        {
            Ok(())
        }
        _ => Err(invalid_grant()),
    }
}

fn authorize_persisted_grant(
    trusted_peer_uid: u32,
    trusted_channel: CapabilityChannel,
    expected_connection_nonce: &[u8; 32],
    trusted_now_unix: i64,
    presented_hash: &[u8; 32],
    request: &CapabilityAuthorizationRequest,
    persisted: &PersistedGrant,
) -> Option<AuthorizedCapability> {
    let actor = CapabilityActor::from_storage(&persisted.2)?;
    let channel = CapabilityChannel::from_storage(&persisted.3)?;
    let operation = CapabilityOperation::from_storage(&persisted.4)?;
    let subject_uid = u32::try_from(persisted.1).ok()?;
    let revocation_epoch = u64::try_from(persisted.10).ok()?;
    if !constant_time_equal(presented_hash, &persisted.0)
        || subject_uid != trusted_peer_uid
        || channel != trusted_channel
        || operation != request.operation
        || persisted.5 != request.project_id
        || persisted.6 != request.workspace_id
        || persisted.7 != request.task_id
        || trusted_now_unix < persisted.8
        || trusted_now_unix >= persisted.9
        || !constant_time_equal(
            expected_connection_nonce,
            &request.presented_connection_nonce,
        )
        || !actor_operation_allowed(actor, channel, operation)
    {
        return None;
    }
    Some(AuthorizedCapability {
        grant_id: request.grant_id.clone(),
        subject_uid,
        actor,
        channel,
        operation,
        project_id: persisted.5.clone(),
        workspace_id: persisted.6.clone(),
        task_id: persisted.7.clone(),
        request_id: request.request_id.clone(),
        revocation_epoch,
    })
}

fn validate_grant(grant: &NewCapabilityGrant) -> TransactionalStorageResult<()> {
    validate_opaque_id(&grant.grant_id, "cap_")?;
    validate_nonempty(&grant.granted_at)?;
    if grant.expires_at_unix <= grant.not_before_unix
        || grant.secret_hash.iter().all(|byte| *byte == 0)
        || !actor_operation_allowed(grant.actor, grant.channel, grant.operation)
        || !valid_grant_scope(grant)
    {
        return Err(invalid_grant());
    }
    Ok(())
}

fn validate_initial_provisioning(
    provisioning: &InitialCapabilityProvisioning,
) -> TransactionalStorageResult<()> {
    validate_opaque_id(&provisioning.provisioning_id, "prv_")
        .map_err(|_| invalid_provisioning())?;
    if provisioning.applied_at_unix < 0
        || provisioning.manifest_sha256.iter().all(|byte| *byte == 0)
        || !(2..=16).contains(&provisioning.grants.len())
    {
        return Err(invalid_provisioning());
    }

    let mut grant_ids = HashSet::new();
    let mut secret_hashes = HashSet::new();
    let mut has_state_admin = false;
    let mut has_installation_admin = false;
    for grant in &provisioning.grants {
        validate_grant(grant).map_err(|_| invalid_provisioning())?;
        if !grant_ids.insert(grant.grant_id.as_str())
            || !secret_hashes.insert(grant.secret_hash)
            || grant.subject_uid == 0
            || grant.not_before_unix > provisioning.applied_at_unix
            || grant.expires_at_unix <= provisioning.applied_at_unix
            || !matches!(
                grant.actor,
                CapabilityActor::Operator | CapabilityActor::Ci | CapabilityActor::Reviewer
            )
        {
            return Err(invalid_provisioning());
        }
        has_state_admin |= grant.actor == CapabilityActor::Operator
            && grant.channel == CapabilityChannel::Administrative
            && grant.operation == CapabilityOperation::StateAdmin
            && grant.project_id.is_none()
            && grant.workspace_id.is_none()
            && grant.task_id.is_none();
        has_installation_admin |= grant.actor == CapabilityActor::Operator
            && grant.channel == CapabilityChannel::Administrative
            && grant.operation == CapabilityOperation::InstallationAdmin
            && grant.project_id.is_none()
            && grant.workspace_id.is_none()
            && grant.task_id.is_none();
    }
    if !has_state_admin || !has_installation_admin {
        return Err(invalid_provisioning());
    }
    Ok(())
}

fn insert_grant_in_transaction(
    transaction: &AC_Transaction<'_>,
    grant: &NewCapabilityGrant,
) -> TransactionalStorageResult<()> {
    let subject_uid = i64::from(grant.subject_uid);
    let epoch = i64::try_from(grant.revocation_epoch).map_err(|_| invalid_grant())?;
    transaction.execute(
        "INSERT INTO capability_subject_epochs(subject_uid, actor_kind, epoch) VALUES (?1, ?2, ?3) ON CONFLICT(subject_uid, actor_kind) DO NOTHING",
        AC_params![subject_uid, grant.actor.storage_value(), epoch],
    )?;
    let persisted_epoch: i64 = transaction.query_row(
        "SELECT epoch FROM capability_subject_epochs WHERE subject_uid = ?1 AND actor_kind = ?2",
        AC_params![subject_uid, grant.actor.storage_value()],
        |row| row.get(0),
    )?;
    if persisted_epoch != epoch {
        return Err(invalid_grant());
    }
    transaction.execute(
        "INSERT INTO capability_grants(grant_id, secret_hash, subject_uid, actor_kind, channel_kind, operation, project_id, workspace_id, task_id, not_before_unix, expires_at_unix, revocation_epoch, granted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        AC_params![
            grant.grant_id,
            grant.secret_hash.as_slice(),
            subject_uid,
            grant.actor.storage_value(),
            grant.channel.storage_value(),
            grant.operation.storage_value(),
            grant.project_id,
            grant.workspace_id,
            grant.task_id,
            grant.not_before_unix,
            grant.expires_at_unix,
            epoch,
            grant.granted_at,
        ],
    )?;
    Ok(())
}

fn validate_managed_grant(
    operator_uid: u32,
    grant: &NewCapabilityGrant,
    trusted_now_unix: i64,
) -> TransactionalStorageResult<()> {
    validate_managed_grant_policy(operator_uid, grant)?;
    if trusted_now_unix < 0
        || grant.not_before_unix != trusted_now_unix
        || grant.granted_at != format!("unix:{trusted_now_unix}")
    {
        return Err(invalid_grant());
    }
    Ok(())
}

fn validate_managed_grant_policy(
    operator_uid: u32,
    grant: &NewCapabilityGrant,
) -> TransactionalStorageResult<()> {
    validate_grant(grant)?;
    if operator_uid == 0
        || grant.subject_uid == 0
        || !matches!(
            grant.actor,
            CapabilityActor::Wrapper
                | CapabilityActor::Agent
                | CapabilityActor::Reviewer
                | CapabilityActor::Ci
        )
        || !matches!(
            grant.operation,
            CapabilityOperation::CompileObserve
                | CapabilityOperation::ProjectionRefresh
                | CapabilityOperation::TaskCurrentRead
                | CapabilityOperation::SubmissionCreate
                | CapabilityOperation::SubmissionReadAssigned
                | CapabilityOperation::ReviewDecideAssigned
                | CapabilityOperation::AttestationCreate
        )
    {
        return Err(invalid_grant());
    }
    Ok(())
}

fn managed_grant_from_persisted(
    grant_id: &str,
    persisted: &PersistedGrant,
) -> TransactionalStorageResult<NewCapabilityGrant> {
    let secret_hash: [u8; 32] = persisted
        .0
        .clone()
        .try_into()
        .map_err(|_| invalid_grant())?;
    let subject_uid = u32::try_from(persisted.1).map_err(|_| invalid_grant())?;
    let actor = CapabilityActor::from_storage(&persisted.2).ok_or_else(invalid_grant)?;
    let channel = CapabilityChannel::from_storage(&persisted.3).ok_or_else(invalid_grant)?;
    let operation = CapabilityOperation::from_storage(&persisted.4).ok_or_else(invalid_grant)?;
    let revocation_epoch = u64::try_from(persisted.10).map_err(|_| invalid_grant())?;
    Ok(NewCapabilityGrant {
        grant_id: grant_id.to_string(),
        secret_hash,
        subject_uid,
        actor,
        channel,
        operation,
        project_id: persisted.5.clone(),
        workspace_id: persisted.6.clone(),
        task_id: persisted.7.clone(),
        not_before_unix: persisted.8,
        expires_at_unix: persisted.9,
        revocation_epoch,
        granted_at: "persisted".to_string(),
    })
}

fn append_capability_management_event(
    transaction: &AC_Transaction<'_>,
    action: &str,
    operator_uid: u32,
    grant: &NewCapabilityGrant,
    occurred_at_unix: i64,
) -> TransactionalStorageResult<ServiceSecurityAuditReceipt> {
    if !matches!(action, "grant" | "revoke") {
        return Err(invalid_grant());
    }
    let last_sequence: i64 = transaction.query_row(
        "SELECT last_sequence FROM service_security_state WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    let sequence = last_sequence.checked_add(1).ok_or_else(invalid_grant)?;
    let event_id = random_security_id("evt_")?;
    let correlation_id = random_security_id("ref_")?;
    transaction.execute(
        "INSERT INTO capability_management_events(sequence, event_id, correlation_id, action_kind, operator_uid, grant_id, subject_uid, actor_kind, channel_kind, operation, project_id, workspace_id, task_id, occurred_at_unix) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        AC_params![
            sequence,
            event_id,
            correlation_id,
            action,
            i64::from(operator_uid),
            grant.grant_id,
            i64::from(grant.subject_uid),
            grant.actor.storage_value(),
            grant.channel.storage_value(),
            grant.operation.storage_value(),
            grant.project_id,
            grant.workspace_id,
            grant.task_id,
            occurred_at_unix,
        ],
    )?;
    let updated = transaction.execute(
        "UPDATE service_security_state SET last_sequence = ?1 WHERE singleton = 1 AND last_sequence = ?2",
        AC_params![sequence, last_sequence],
    )?;
    if updated != 1 {
        return Err(invalid_grant());
    }
    Ok(ServiceSecurityAuditReceipt {
        sequence,
        event_id,
        correlation_id,
    })
}

fn valid_authorization_request(request: &CapabilityAuthorizationRequest) -> bool {
    validate_opaque_id(&request.grant_id, "cap_").is_ok()
        && validate_opaque_id(&request.request_id, "req_").is_ok()
        && request
            .presented_connection_nonce
            .iter()
            .any(|byte| *byte != 0)
        && request.presented_secret.iter().any(|byte| *byte != 0)
        && request.request_fingerprint.iter().any(|byte| *byte != 0)
}

fn actor_operation_allowed(
    actor: CapabilityActor,
    channel: CapabilityChannel,
    operation: CapabilityOperation,
) -> bool {
    use CapabilityActor as Actor;
    use CapabilityChannel as Channel;
    use CapabilityOperation as Operation;
    matches!(
        (actor, channel, operation),
        (Actor::Wrapper, Channel::Limited, Operation::CompileObserve)
            | (
                Actor::Wrapper,
                Channel::Limited,
                Operation::ProjectionRefresh
            )
            | (Actor::Agent, Channel::Limited, Operation::TaskCurrentRead)
            | (Actor::Agent, Channel::Limited, Operation::SubmissionCreate)
            | (
                Actor::Agent,
                Channel::Limited,
                Operation::SubmissionReadAssigned
            )
            | (
                Actor::Reviewer,
                Channel::Limited,
                Operation::TaskCurrentRead
            )
            | (
                Actor::Reviewer,
                Channel::Limited,
                Operation::SubmissionReadAssigned
            )
            | (
                Actor::Reviewer,
                Channel::Limited,
                Operation::ReviewDecideAssigned
            )
            | (Actor::Ci, Channel::Limited, Operation::CompileObserve)
            | (Actor::Ci, Channel::Limited, Operation::AttestationCreate)
            | (
                Actor::Ci,
                Channel::Administrative,
                Operation::AttestationCreate
            )
            | (
                Actor::Ci,
                Channel::Administrative,
                Operation::InstallationAdmin
            )
            | (
                Actor::Service,
                Channel::Limited,
                Operation::ProjectionRefresh
            )
            | (
                Actor::Service,
                Channel::Administrative,
                Operation::StateTransition
            )
            | (
                Actor::Operator,
                Channel::Administrative,
                Operation::StateTransition
            )
            | (
                Actor::Operator,
                Channel::Administrative,
                Operation::StateAdmin
            )
            | (
                Actor::Operator,
                Channel::Administrative,
                Operation::InstallationAdmin
            )
            | (
                Actor::Operator,
                Channel::Administrative,
                Operation::DiagnosticPrivate
            )
    )
}

fn valid_grant_scope(grant: &NewCapabilityGrant) -> bool {
    if !valid_optional_id(grant.project_id.as_deref(), "prj_")
        || !valid_optional_id(grant.workspace_id.as_deref(), "wsp_")
        || !valid_optional_id(grant.task_id.as_deref(), "tsk_")
        || (grant.workspace_id.is_some() && grant.project_id.is_none())
        || (grant.task_id.is_some() && grant.project_id.is_none())
    {
        return false;
    }
    use CapabilityOperation as Operation;
    match grant.operation {
        Operation::CompileObserve
        | Operation::ProjectionRefresh
        | Operation::TaskCurrentRead
        | Operation::AttestationCreate => {
            grant.project_id.is_some() && grant.workspace_id.is_some()
        }
        Operation::SubmissionCreate
        | Operation::SubmissionReadAssigned
        | Operation::ReviewDecideAssigned
        | Operation::StateTransition => {
            grant.project_id.is_some() && grant.workspace_id.is_some() && grant.task_id.is_some()
        }
        Operation::StateAdmin | Operation::InstallationAdmin | Operation::DiagnosticPrivate => true,
    }
}

fn valid_optional_id(value: Option<&str>, prefix: &str) -> bool {
    value
        .map(|value| validate_opaque_id(value, prefix).is_ok())
        .unwrap_or(true)
}

fn validate_opaque_id(value: &str, prefix: &str) -> TransactionalStorageResult<()> {
    let token = value.strip_prefix(prefix).ok_or_else(invalid_grant)?;
    if token.len() != 26
        || !token.bytes().all(|byte| {
            matches!(byte, b'0'..=b'9' | b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z')
        })
    {
        return Err(invalid_grant());
    }
    Ok(())
}

fn validate_nonempty(value: &str) -> TransactionalStorageResult<()> {
    if value.trim().is_empty() {
        return Err(invalid_grant());
    }
    Ok(())
}

fn invalid_grant() -> TransactionalStorageError {
    TransactionalStorageError::InvalidInput("invalid capability authorization contract".to_string())
}

fn invalid_provisioning() -> TransactionalStorageError {
    TransactionalStorageError::InvalidInput(
        "invalid initial capability provisioning contract".to_string(),
    )
}
