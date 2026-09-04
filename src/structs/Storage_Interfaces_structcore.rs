use std::collections::BTreeSet;
use std::path::Path;

use crate::libres::Rusqlite_unit::{AC_OptionalExtension, AC_Transaction, AC_params};

use super::{
    encode_canonical_task_projection_payload, materialize_project_guard,
    validate_serialized_public_document, AssignedSubmission, AtomicProjectProjectionWriter,
    AuditStore, BackupCheckpointRecord, BackupCheckpointStore, CanonicalMutationOutcome,
    CanonicalStateSnapshot, CanonicalStateStore, CanonicalStateTransition, CanonicalTaskAdvance,
    CanonicalTaskAdvanceOutcome, CanonicalTaskPackage, CanonicalTaskProjectionPayload,
    CanonicalTaskProjectionRef, CanonicalTaskState, CurrentTaskProjection, EventWriteOutcome,
    GovernanceErrorsProjection, NewAuditEvent, NewReview, NewSubmission, ProjectDatabase,
    ProjectProjectionWriter, ProjectStateRoot, ProjectedTaskAction, ProjectionArtifactKind,
    ProjectionMetadata, ProjectionWriteReceipt, ReviewStore, ReviewWriteOutcome, StorageProject,
    StorageWorkspace, SubmissionAssignment, SubmissionStore, SubmissionWriteOutcome,
    TransactionalStorageError, TransactionalStorageResult, AGENT_PROJECTION_CONTRACT_VERSION,
    CANONICAL_STATE_CONTRACT_VERSION, CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION,
    GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION, SUBMISSION_CONTRACT_VERSION,
};

pub(super) fn materialize_limited_project_projections(
    project_root: &Path,
    current_task: &[u8],
    governance_errors: &[u8],
) -> TransactionalStorageResult<()> {
    validate_limited_projection(current_task, "current_task")?;
    validate_limited_projection(governance_errors, "governance_errors")?;
    let state_root = ProjectStateRoot::with_default_config(
        project_root.to_path_buf(),
        project_root.to_path_buf(),
        project_root.to_path_buf(),
    );
    materialize_project_guard(&state_root).map_err(|_| {
        TransactionalStorageError::InvalidInput(
            "project guard bootstrap does not satisfy lifecycle-v1".to_string(),
        )
    })?;
    super::Transactional_Storage_structcore::atomic_replace_if_changed(
        &state_root.todo_projection_path(),
        current_task,
    )?;
    super::Transactional_Storage_structcore::atomic_replace_if_changed(
        &state_root.governance_error_projection_path(),
        governance_errors,
    )?;
    Ok(())
}

fn validate_limited_projection(
    document: &[u8],
    expected_artifact: &str,
) -> TransactionalStorageResult<()> {
    validate_serialized_public_document(document, "limited projection")?;
    let value: serde_json::Value = serde_json::from_slice(document).map_err(|_| {
        TransactionalStorageError::InvalidInput("limited projection is not valid JSON".to_string())
    })?;
    let object = value.as_object().ok_or_else(|| {
        TransactionalStorageError::InvalidInput("limited projection must be an object".to_string())
    })?;
    if object
        .get("contract_version")
        .and_then(serde_json::Value::as_str)
        != Some(AGENT_PROJECTION_CONTRACT_VERSION)
        || object
            .get("artifact_kind")
            .and_then(serde_json::Value::as_str)
            != Some(expected_artifact)
    {
        return Err(TransactionalStorageError::InvalidInput(
            "limited projection contract does not match its destination".to_string(),
        ));
    }
    Ok(())
}

impl CanonicalStateStore for ProjectDatabase {
    fn register_project(&self, project: &StorageProject) -> TransactionalStorageResult<()> {
        self.ensure_project(project)
    }

    fn register_workspace(&self, workspace: &StorageWorkspace) -> TransactionalStorageResult<()> {
        validate_required("workspace_id", &workspace.workspace_id)?;
        validate_required("project_id", &workspace.project_id)?;
        validate_required("workspace_key", &workspace.workspace_key)?;
        validate_required("created_at", &workspace.created_at)?;
        self.with_immediate_transaction(|transaction| {
            transaction.execute(
                "INSERT INTO workspaces(workspace_id, project_id, workspace_key, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(workspace_id) DO NOTHING",
                AC_params![workspace.workspace_id, workspace.project_id, workspace.workspace_key, workspace.created_at],
            )?;
            let persisted = transaction.query_row(
                "SELECT project_id, workspace_key FROM workspaces WHERE workspace_id = ?1",
                AC_params![workspace.workspace_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?;
            if persisted.0 != workspace.project_id || persisted.1 != workspace.workspace_key {
                return Err(TransactionalStorageError::Integrity(
                    "workspace_id is already bound to another project or workspace_key".to_string(),
                ));
            }
            Ok(())
        })
    }

    fn load_canonical_state(
        &self,
        project_id: &str,
        workspace_id: &str,
    ) -> TransactionalStorageResult<CanonicalStateSnapshot> {
        validate_required("project_id", project_id)?;
        validate_required("workspace_id", workspace_id)?;
        let connection = self.open_connection(false)?;
        load_canonical_state_from_connection(&connection, project_id, workspace_id)
    }

    fn apply_canonical_transition(
        &self,
        transition: &CanonicalStateTransition,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<CanonicalMutationOutcome> {
        validate_transition(transition, audit_event)?;
        self.with_immediate_transaction(|transaction| {
            let audit = self.record_event_in_transaction(transaction, audit_event)?;
            if audit.inserted() {
                let updated = transaction.execute(
                    "UPDATE projects SET active_plan_id = ?3, current_task_id = ?4, state_revision = state_revision + 1 WHERE project_id = ?1 AND state_revision = ?2",
                    AC_params![
                        transition.project_id,
                        transition.expected_state_revision,
                        transition.active_plan_id,
                        transition.current_task_id,
                    ],
                )?;
                if updated != 1 {
                    return Err(TransactionalStorageError::Integrity(
                        "canonical state revision changed before the transition was applied"
                            .to_string(),
                    ));
                }
            }
            let state = load_canonical_state_from_transaction(
                transaction,
                &transition.project_id,
                &transition.workspace_id,
            )?;
            if !audit.inserted()
                && (state.active_plan_id != transition.active_plan_id
                    || state.current_task_id != transition.current_task_id)
            {
                return Err(TransactionalStorageError::Integrity(
                    "duplicate transition event does not match the persisted canonical state"
                        .to_string(),
                ));
            }
            Ok(CanonicalMutationOutcome { state, audit })
        })
    }

    fn install_task_package(
        &self,
        package: &CanonicalTaskPackage,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<CanonicalMutationOutcome> {
        let encoded_tasks = validate_and_encode_task_package(package, audit_event)?;
        let expected_revision =
            package
                .expected_state_revision
                .checked_add(1)
                .ok_or_else(|| {
                    TransactionalStorageError::InvalidInput(
                        "task package state revision overflow".to_string(),
                    )
                })?;
        self.with_immediate_transaction(|transaction| {
            let audit = self.record_event_in_transaction(transaction, audit_event)?;
            if audit.inserted() {
                let current = transaction.query_row(
                    "SELECT state_revision, active_plan_id, current_task_id FROM projects WHERE project_id = ?1",
                    AC_params![package.project_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    },
                )?;
                let workspace_project = transaction.query_row(
                    "SELECT project_id FROM workspaces WHERE workspace_id = ?1 AND status = 'active'",
                    AC_params![package.workspace_id],
                    |row| row.get::<_, String>(0),
                )?;
                let plan_count = transaction.query_row(
                    "SELECT count(*) FROM plans WHERE project_id = ?1",
                    AC_params![package.project_id],
                    |row| row.get::<_, i64>(0),
                )?;
                let task_count = transaction.query_row(
                    "SELECT count(*) FROM tasks WHERE project_id = ?1",
                    AC_params![package.project_id],
                    |row| row.get::<_, i64>(0),
                )?;
                if workspace_project != package.project_id
                    || current.0 != package.expected_state_revision
                    || current.1.is_some()
                    || current.2.is_some()
                    || plan_count != 0
                    || task_count != 0
                {
                    return Err(TransactionalStorageError::Integrity(
                        "task package may initialize only an empty canonical project".to_string(),
                    ));
                }

                transaction.execute(
                    "INSERT INTO plans(plan_id, project_id, plan_revision, status, payload, created_at) VALUES (?1, ?2, 1, 'active', ?3, ?4)",
                    AC_params![
                        package.plan_id,
                        package.project_id,
                        GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION.as_bytes(),
                        audit_event.occurred_at,
                    ],
                )?;
                for (ordinal, (task, payload)) in
                    package.tasks.iter().zip(encoded_tasks.iter()).enumerate()
                {
                    let ordinal = i64::try_from(ordinal).map_err(|_| {
                        TransactionalStorageError::InvalidInput(
                            "task package ordinal is unsupported".to_string(),
                        )
                    })?;
                    transaction.execute(
                        "INSERT INTO tasks(task_id, project_id, plan_id, task_revision, status, review_status, payload, created_at, task_ordinal) VALUES (?1, ?2, ?3, ?4, 'available', 'not_required', ?5, ?6, ?7)",
                        AC_params![
                            task.task_id,
                            package.project_id,
                            package.plan_id,
                            task.task_revision,
                            payload,
                            audit_event.occurred_at,
                            ordinal,
                        ],
                    )?;
                }
                let updated = transaction.execute(
                    "UPDATE projects SET active_plan_id = ?2, current_task_id = ?3, state_revision = state_revision + 1 WHERE project_id = ?1 AND state_revision = ?4 AND active_plan_id IS NULL AND current_task_id IS NULL",
                    AC_params![
                        package.project_id,
                        package.plan_id,
                        package.tasks[0].task_id,
                        package.expected_state_revision,
                    ],
                )?;
                if updated != 1 {
                    return Err(TransactionalStorageError::Integrity(
                        "canonical state changed while installing the task package".to_string(),
                    ));
                }
                super::Governance_Errors_Projection_structcore::carry_forward_canonical_governance_snapshot(
                    transaction,
                    &package.project_id,
                    &package.workspace_id,
                    package.expected_state_revision,
                    expected_revision,
                    &audit_event.occurred_at,
                )?;
            }

            let state = load_canonical_state_from_transaction(
                transaction,
                &package.project_id,
                &package.workspace_id,
            )?;
            if state.state_revision != expected_revision
                || state.active_plan_id.as_deref() != Some(package.plan_id.as_str())
                || state.current_task_id.as_deref() != Some(package.tasks[0].task_id.as_str())
                || state.tasks.len() != package.tasks.len()
                || package.tasks.iter().any(|requested| {
                    !state.tasks.iter().any(|persisted| {
                        persisted.task_id == requested.task_id
                            && persisted.task_revision == requested.task_revision
                            && persisted.status == "available"
                            && persisted.review_status == "not_required"
                    })
                })
            {
                return Err(TransactionalStorageError::Integrity(
                    "persisted task package does not match the requested canonical state"
                        .to_string(),
                ));
            }
            Ok(CanonicalMutationOutcome { state, audit })
        })
    }

    fn advance_accepted_task(
        &self,
        advance: &CanonicalTaskAdvance,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<CanonicalTaskAdvanceOutcome> {
        self.advance_accepted_task_transactional(advance, audit_event, None)
    }

    fn advance_accepted_task_with_checkpoint(
        &self,
        advance: &CanonicalTaskAdvance,
        audit_event: &NewAuditEvent,
        checkpoint: &BackupCheckpointRecord,
    ) -> TransactionalStorageResult<CanonicalTaskAdvanceOutcome> {
        self.advance_accepted_task_transactional(advance, audit_event, Some(checkpoint))
    }
}

impl ProjectDatabase {
    fn advance_accepted_task_transactional(
        &self,
        advance: &CanonicalTaskAdvance,
        audit_event: &NewAuditEvent,
        checkpoint: Option<&BackupCheckpointRecord>,
    ) -> TransactionalStorageResult<CanonicalTaskAdvanceOutcome> {
        validate_task_advance(advance, audit_event)?;
        let expected_revision =
            advance
                .expected_state_revision
                .checked_add(1)
                .ok_or_else(|| {
                    TransactionalStorageError::InvalidInput(
                        "canonical task advance revision overflow".to_string(),
                    )
                })?;
        if let Some(checkpoint) = checkpoint {
            validate_advance_checkpoint(advance, expected_revision, checkpoint)?;
        }
        self.with_immediate_transaction(|transaction| {
            if let Some(checkpoint) = checkpoint {
                record_backup_checkpoint_in_transaction(transaction, checkpoint)?;
            }
            let audit = self.record_event_in_transaction(transaction, audit_event)?;
            let task = transaction.query_row(
                "SELECT plan_id, task_ordinal, status, review_status FROM tasks WHERE project_id = ?1 AND task_id = ?2",
                AC_params![advance.project_id, advance.task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )?;
            let next_task_id = transaction
                .query_row(
                    "SELECT task_id FROM tasks WHERE project_id = ?1 AND plan_id = ?2 AND task_ordinal > ?3 AND status = 'available' ORDER BY task_ordinal ASC LIMIT 1",
                    AC_params![advance.project_id, task.0, task.1],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if audit.inserted() {
                let project = transaction.query_row(
                    "SELECT state_revision, active_plan_id, current_task_id FROM projects WHERE project_id = ?1",
                    AC_params![advance.project_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    },
                )?;
                let workspace_project = transaction.query_row(
                    "SELECT project_id FROM workspaces WHERE workspace_id = ?1 AND status = 'active'",
                    AC_params![advance.workspace_id],
                    |row| row.get::<_, String>(0),
                )?;
                let review = transaction.query_row(
                    "SELECT r.outcome, s.project_id, s.workspace_id, s.task_id FROM reviews r JOIN submissions s ON s.submission_id = r.submission_id AND s.project_id = r.project_id WHERE r.review_id = ?1 AND r.project_id = ?2",
                    AC_params![advance.review_id, advance.project_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )?;
                if workspace_project != advance.project_id
                    || project.0 != advance.expected_state_revision
                    || project.1.as_deref() != Some(task.0.as_str())
                    || project.2.as_deref() != Some(advance.task_id.as_str())
                    || task.2 != "available"
                    || task.3 != "not_required"
                    || review.0 != "accepted"
                    || review.1 != advance.project_id
                    || review.2.as_deref() != Some(advance.workspace_id.as_str())
                    || review.3 != advance.task_id
                {
                    return Err(TransactionalStorageError::Integrity(
                        "canonical task advance is not backed by the accepted assigned review"
                            .to_string(),
                    ));
                }
                let completed = transaction.execute(
                    "UPDATE tasks SET status = 'completed', review_status = 'accepted' WHERE project_id = ?1 AND task_id = ?2 AND status = 'available' AND review_status = 'not_required'",
                    AC_params![advance.project_id, advance.task_id],
                )?;
                if completed != 1 {
                    return Err(TransactionalStorageError::Integrity(
                        "canonical task changed before accepted advancement".to_string(),
                    ));
                }
                if let Some(next_task_id) = next_task_id.as_deref() {
                    let updated = transaction.execute(
                        "UPDATE projects SET current_task_id = ?3, state_revision = state_revision + 1 WHERE project_id = ?1 AND state_revision = ?2 AND current_task_id = ?4 AND active_plan_id = ?5",
                        AC_params![
                            advance.project_id,
                            advance.expected_state_revision,
                            next_task_id,
                            advance.task_id,
                            task.0,
                        ],
                    )?;
                    if updated != 1 {
                        return Err(TransactionalStorageError::Integrity(
                            "canonical project changed before accepted advancement".to_string(),
                        ));
                    }
                } else {
                    let completed_plan = transaction.execute(
                        "UPDATE plans SET status = 'completed' WHERE project_id = ?1 AND plan_id = ?2 AND status = 'active'",
                        AC_params![advance.project_id, task.0],
                    )?;
                    let updated = transaction.execute(
                        "UPDATE projects SET active_plan_id = NULL, current_task_id = NULL, state_revision = state_revision + 1 WHERE project_id = ?1 AND state_revision = ?2 AND current_task_id = ?3 AND active_plan_id = ?4",
                        AC_params![
                            advance.project_id,
                            advance.expected_state_revision,
                            advance.task_id,
                            task.0,
                        ],
                    )?;
                    if completed_plan != 1 || updated != 1 {
                        return Err(TransactionalStorageError::Integrity(
                            "canonical final task could not complete its active plan".to_string(),
                        ));
                    }
                }
                super::Governance_Errors_Projection_structcore::carry_forward_canonical_governance_snapshot(
                    transaction,
                    &advance.project_id,
                    &advance.workspace_id,
                    advance.expected_state_revision,
                    expected_revision,
                    &audit_event.occurred_at,
                )?;
            }
            let state = load_canonical_state_from_transaction(
                transaction,
                &advance.project_id,
                &advance.workspace_id,
            )?;
            let completed = state
                .tasks
                .iter()
                .find(|candidate| candidate.task_id == advance.task_id)
                .ok_or_else(|| {
                    TransactionalStorageError::Integrity(
                        "advanced canonical task is unavailable".to_string(),
                    )
                })?;
            if state.state_revision != expected_revision
                || state.current_task_id != next_task_id
                || completed.status != "completed"
                || completed.review_status != "accepted"
                || (next_task_id.is_none() && state.active_plan_id.is_some())
                || (next_task_id.is_some()
                    && state.active_plan_id.as_deref() != Some(task.0.as_str()))
            {
                return Err(TransactionalStorageError::Integrity(
                    "persisted accepted advancement does not match canonical state".to_string(),
                ));
            }
            Ok(CanonicalTaskAdvanceOutcome {
                state,
                audit,
                completed_task_id: advance.task_id.clone(),
                next_task_id,
            })
        })
    }
}

fn validate_task_advance(
    advance: &CanonicalTaskAdvance,
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<()> {
    validate_opaque_identifier(&advance.project_id, "prj_", "project_id")?;
    validate_opaque_identifier(&advance.workspace_id, "wsp_", "workspace_id")?;
    validate_opaque_identifier(&advance.task_id, "tsk_", "task_id")?;
    validate_opaque_identifier(&advance.review_id, "rev_", "review_id")?;
    if advance.expected_state_revision < 0 {
        return Err(TransactionalStorageError::InvalidInput(
            "canonical task advance revision is invalid".to_string(),
        ));
    }
    validate_audit_binding(audit_event, &advance.project_id, "state_transitioned")?;
    if audit_event.workspace_id.as_deref() != Some(advance.workspace_id.as_str())
        || audit_event.resource_id.as_deref() != Some(advance.task_id.as_str())
        || !matches!(audit_event.actor_kind.as_str(), "ci" | "operator")
    {
        return Err(TransactionalStorageError::InvalidInput(
            "canonical task advance audit binding is invalid".to_string(),
        ));
    }
    Ok(())
}

fn validate_advance_checkpoint(
    advance: &CanonicalTaskAdvance,
    expected_revision: i64,
    checkpoint: &BackupCheckpointRecord,
) -> TransactionalStorageResult<()> {
    validate_backup_checkpoint(checkpoint)?;
    if checkpoint.transition_kind != "advance"
        || checkpoint.transition_id != advance.review_id
        || checkpoint.project_id != advance.project_id
        || checkpoint.workspace_id != advance.workspace_id
        || checkpoint.task_id.as_deref() != Some(advance.task_id.as_str())
        || checkpoint.task_revision.is_none()
        || checkpoint.state_revision != expected_revision
    {
        return Err(TransactionalStorageError::InvalidInput(
            "advance checkpoint is not bound to the canonical transition".to_string(),
        ));
    }
    Ok(())
}

fn validate_and_encode_task_package(
    package: &CanonicalTaskPackage,
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<Vec<Vec<u8>>> {
    if package.contract_version != GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION
        || package.expected_state_revision < 0
        || package.tasks.is_empty()
        || package.tasks.len() > 256
    {
        return Err(TransactionalStorageError::InvalidInput(
            "task package violates governance-task-package-v1".to_string(),
        ));
    }
    validate_opaque_identifier(&package.project_id, "prj_", "project_id")?;
    validate_opaque_identifier(&package.workspace_id, "wsp_", "workspace_id")?;
    validate_opaque_identifier(&package.plan_id, "pln_", "plan_id")?;
    validate_audit_binding(audit_event, &package.project_id, "state_transitioned")?;
    if audit_event.workspace_id.as_deref() != Some(package.workspace_id.as_str())
        || audit_event.resource_id.as_deref() != Some(package.plan_id.as_str())
        || audit_event.actor_kind != "operator"
    {
        return Err(TransactionalStorageError::InvalidInput(
            "task package audit event is not bound to the operator request".to_string(),
        ));
    }

    let mut task_ids = BTreeSet::new();
    let mut encoded = Vec::with_capacity(package.tasks.len());
    for task in &package.tasks {
        validate_opaque_identifier(&task.task_id, "tsk_", "task_id")?;
        if task.task_revision < 1 || !task_ids.insert(task.task_id.as_str()) {
            return Err(TransactionalStorageError::InvalidInput(
                "task package contains an invalid or duplicate task".to_string(),
            ));
        }
        encoded.push(encode_canonical_task_projection_payload(
            &CanonicalTaskProjectionPayload {
                contract_version: CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION.to_string(),
                objective: task.objective.clone(),
                allowed_action: ProjectedTaskAction::Implement,
                acceptance_criteria: task.acceptance_criteria.clone(),
                relevant_paths: task.relevant_paths.clone(),
            },
        )?);
    }
    Ok(encoded)
}

fn validate_opaque_identifier(
    value: &str,
    prefix: &str,
    field: &str,
) -> TransactionalStorageResult<()> {
    let valid = value.strip_prefix(prefix).is_some_and(|token| {
        token.len() == 26
            && token.bytes().all(|byte| {
                byte.is_ascii_digit()
                    || matches!(
                        byte,
                        b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z'
                    )
            })
    });
    if !valid {
        return Err(TransactionalStorageError::InvalidInput(format!(
            "{field} is not a canonical opaque identifier"
        )));
    }
    Ok(())
}

impl SubmissionStore for ProjectDatabase {
    fn record_submission(
        &self,
        submission: &NewSubmission,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<SubmissionWriteOutcome> {
        record_submission_transactionally(self, submission, audit_event, None)
    }

    fn record_submission_with_checkpoint(
        &self,
        submission: &NewSubmission,
        audit_event: &NewAuditEvent,
        checkpoint: &BackupCheckpointRecord,
    ) -> TransactionalStorageResult<SubmissionWriteOutcome> {
        record_submission_transactionally(self, submission, audit_event, Some(checkpoint))
    }

    fn submission_id_for_request(
        &self,
        project_id: &str,
        request_id: &str,
        fingerprint: &str,
    ) -> TransactionalStorageResult<Option<String>> {
        validate_opaque_identifier(project_id, "prj_", "project_id")?;
        validate_required("request_id", request_id)?;
        validate_fingerprint(fingerprint)?;
        let connection = self.open_connection(false)?;
        let existing = connection
            .query_row(
                "SELECT submission_id, fingerprint FROM submissions WHERE project_id = ?1 AND request_id = ?2",
                AC_params![project_id, request_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        match existing {
            Some((submission_id, persisted_fingerprint))
                if persisted_fingerprint == fingerprint =>
            {
                Ok(Some(submission_id))
            }
            Some(_) => Err(TransactionalStorageError::Integrity(
                "request_id was reused with a different submission fingerprint".to_string(),
            )),
            None => Ok(None),
        }
    }

    fn submission_count(&self, project_id: &str) -> TransactionalStorageResult<i64> {
        validate_required("project_id", project_id)?;
        let connection = self.open_connection(false)?;
        Ok(connection.query_row(
            "SELECT count(*) FROM submissions WHERE project_id = ?1",
            AC_params![project_id],
            |row| row.get(0),
        )?)
    }

    fn submission_assignment(
        &self,
        project_id: &str,
        submission_id: &str,
    ) -> TransactionalStorageResult<Option<SubmissionAssignment>> {
        validate_opaque_identifier(project_id, "prj_", "project_id")?;
        validate_opaque_identifier(submission_id, "sub_", "submission_id")?;
        let connection = self.open_connection(false)?;
        Ok(connection
            .query_row(
                "SELECT project_id, workspace_id, task_id FROM submissions WHERE project_id = ?1 AND submission_id = ?2",
                AC_params![project_id, submission_id],
                |row| {
                    Ok(SubmissionAssignment {
                        project_id: row.get(0)?,
                        workspace_id: row.get(1)?,
                        task_id: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    fn read_assigned_submission(
        &self,
        project_id: &str,
        submission_id: &str,
    ) -> TransactionalStorageResult<Option<AssignedSubmission>> {
        validate_opaque_identifier(project_id, "prj_", "project_id")?;
        validate_opaque_identifier(submission_id, "sub_", "submission_id")?;
        let connection = self.open_connection(false)?;
        Ok(connection
            .query_row(
                "SELECT project_id, workspace_id, task_id, payload FROM submissions WHERE project_id = ?1 AND submission_id = ?2",
                AC_params![project_id, submission_id],
                |row| {
                    Ok(AssignedSubmission {
                        assignment: SubmissionAssignment {
                            project_id: row.get(0)?,
                            workspace_id: row.get(1)?,
                            task_id: row.get(2)?,
                        },
                        payload: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }
}

fn record_submission_transactionally(
    database: &ProjectDatabase,
    submission: &NewSubmission,
    audit_event: &NewAuditEvent,
    checkpoint: Option<&BackupCheckpointRecord>,
) -> TransactionalStorageResult<SubmissionWriteOutcome> {
    validate_submission(submission, audit_event)?;
    if let Some(checkpoint) = checkpoint {
        validate_submission_checkpoint(submission, checkpoint)?;
    }
    database.with_immediate_transaction(|transaction| {
        if let Some(checkpoint) = checkpoint {
            if record_backup_checkpoint_in_transaction(transaction, checkpoint)? {
                require_checkpoint_current_state(transaction, checkpoint)?;
            }
        }
        if let Some(outcome) = find_duplicate_submission(transaction, submission)? {
            return Ok(outcome);
        }
        validate_submission_relationships(transaction, submission)?;
        transaction.execute(
            "INSERT INTO submissions(contract_version, submission_id, project_id, workspace_id, task_id, task_revision, base_revision, request_id, fingerprint, payload, received_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            AC_params![
                submission.contract_version,
                submission.submission_id,
                submission.project_id,
                submission.workspace_id,
                submission.task_id,
                submission.task_revision,
                submission.base_revision,
                submission.request_id,
                submission.fingerprint,
                submission.payload,
                submission.received_at,
            ],
        )?;
        let audit = database.record_event_in_transaction(transaction, audit_event)?;
        if !audit.inserted() {
            return Err(TransactionalStorageError::Integrity(
                "new submission resolved to an existing audit event".to_string(),
            ));
        }
        Ok(SubmissionWriteOutcome::Inserted {
            submission_id: submission.submission_id.clone(),
            event_sequence: audit.sequence(),
        })
    })
}

fn validate_submission_checkpoint(
    submission: &NewSubmission,
    checkpoint: &BackupCheckpointRecord,
) -> TransactionalStorageResult<()> {
    validate_backup_checkpoint(checkpoint)?;
    if checkpoint.transition_kind != "submission"
        || checkpoint.transition_id != submission.submission_id
        || checkpoint.project_id != submission.project_id
        || checkpoint.workspace_id != submission.workspace_id
        || checkpoint.task_id.as_deref() != Some(submission.task_id.as_str())
        || checkpoint.task_revision != Some(submission.task_revision)
    {
        return Err(TransactionalStorageError::InvalidInput(
            "submission checkpoint is not bound to the immutable submission".to_string(),
        ));
    }
    Ok(())
}

impl ReviewStore for ProjectDatabase {
    fn record_review(
        &self,
        review: &NewReview,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<ReviewWriteOutcome> {
        record_review_transactionally(self, review, audit_event, None)
    }

    fn record_review_with_checkpoint(
        &self,
        review: &NewReview,
        audit_event: &NewAuditEvent,
        checkpoint: &BackupCheckpointRecord,
    ) -> TransactionalStorageResult<ReviewWriteOutcome> {
        record_review_transactionally(self, review, audit_event, Some(checkpoint))
    }

    fn review_id_for_submission(
        &self,
        project_id: &str,
        submission_id: &str,
    ) -> TransactionalStorageResult<Option<String>> {
        validate_opaque_identifier(project_id, "prj_", "project_id")?;
        validate_opaque_identifier(submission_id, "sub_", "submission_id")?;
        let connection = self.open_connection(false)?;
        Ok(connection
            .query_row(
                "SELECT review_id FROM reviews WHERE project_id = ?1 AND submission_id = ?2",
                AC_params![project_id, submission_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    fn review_assignment(
        &self,
        project_id: &str,
        review_id: &str,
    ) -> TransactionalStorageResult<Option<SubmissionAssignment>> {
        validate_opaque_identifier(project_id, "prj_", "project_id")?;
        validate_opaque_identifier(review_id, "rev_", "review_id")?;
        let connection = self.open_connection(false)?;
        Ok(connection
            .query_row(
                "SELECT s.project_id, s.workspace_id, s.task_id FROM reviews r JOIN submissions s ON s.project_id = r.project_id AND s.submission_id = r.submission_id WHERE r.project_id = ?1 AND r.review_id = ?2",
                AC_params![project_id, review_id],
                |row| {
                    Ok(SubmissionAssignment {
                        project_id: row.get(0)?,
                        workspace_id: row.get(1)?,
                        task_id: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }
}

fn record_review_transactionally(
    database: &ProjectDatabase,
    review: &NewReview,
    audit_event: &NewAuditEvent,
    checkpoint: Option<&BackupCheckpointRecord>,
) -> TransactionalStorageResult<ReviewWriteOutcome> {
    validate_review(review, audit_event)?;
    if let Some(checkpoint) = checkpoint {
        validate_review_checkpoint(review, checkpoint)?;
    }
    database.with_immediate_transaction(|transaction| {
        if let Some(checkpoint) = checkpoint {
            let assignment = transaction.query_row(
                "SELECT workspace_id, task_id, task_revision FROM submissions WHERE project_id = ?1 AND submission_id = ?2",
                AC_params![review.project_id, review.submission_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )?;
            if assignment.0.as_deref() != Some(checkpoint.workspace_id.as_str())
                || checkpoint.task_id.as_deref() != Some(assignment.1.as_str())
                || checkpoint.task_revision != Some(assignment.2)
            {
                return Err(TransactionalStorageError::InvalidInput(
                    "review checkpoint does not match the assigned submission".to_string(),
                ));
            }
            if record_backup_checkpoint_in_transaction(transaction, checkpoint)? {
                require_checkpoint_current_state(transaction, checkpoint)?;
            }
        }
        if let Some(outcome) = find_duplicate_review(transaction, review)? {
            return Ok(outcome);
        }
        validate_review_relationships(transaction, review)?;
        transaction.execute(
            "INSERT INTO reviews(review_id, project_id, submission_id, reviewer_id, outcome, payload, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            AC_params![
                review.review_id,
                review.project_id,
                review.submission_id,
                review.reviewer_id,
                review.decision.storage_value(),
                review.payload,
                review.created_at,
            ],
        )?;
        let audit = database.record_event_in_transaction(transaction, audit_event)?;
        if !audit.inserted() {
            return Err(TransactionalStorageError::Integrity(
                "new review resolved to an existing audit event".to_string(),
            ));
        }
        Ok(ReviewWriteOutcome::Inserted {
            review_id: review.review_id.clone(),
            event_sequence: audit.sequence(),
        })
    })
}

fn validate_review_checkpoint(
    review: &NewReview,
    checkpoint: &BackupCheckpointRecord,
) -> TransactionalStorageResult<()> {
    validate_backup_checkpoint(checkpoint)?;
    if checkpoint.transition_kind != "review"
        || checkpoint.transition_id != review.review_id
        || checkpoint.project_id != review.project_id
        || checkpoint.task_id.is_none()
        || checkpoint.task_revision.is_none()
    {
        return Err(TransactionalStorageError::InvalidInput(
            "review checkpoint is not bound to the immutable review".to_string(),
        ));
    }
    Ok(())
}

fn require_checkpoint_current_state(
    transaction: &AC_Transaction<'_>,
    checkpoint: &BackupCheckpointRecord,
) -> TransactionalStorageResult<()> {
    let state_revision = transaction.query_row(
        "SELECT state_revision FROM projects WHERE project_id = ?1",
        AC_params![checkpoint.project_id],
        |row| row.get::<_, i64>(0),
    )?;
    if state_revision != checkpoint.state_revision {
        return Err(TransactionalStorageError::Integrity(
            "new backup checkpoint does not match the current canonical revision".to_string(),
        ));
    }
    Ok(())
}

impl BackupCheckpointStore for ProjectDatabase {
    fn record_backup_checkpoint(
        &self,
        checkpoint: &BackupCheckpointRecord,
    ) -> TransactionalStorageResult<bool> {
        validate_backup_checkpoint(checkpoint)?;
        self.with_immediate_transaction(|transaction| {
            record_backup_checkpoint_in_transaction(transaction, checkpoint)
        })
    }

    fn backup_checkpoint(
        &self,
        project_id: &str,
        transition_kind: &str,
        transition_id: &str,
    ) -> TransactionalStorageResult<Option<BackupCheckpointRecord>> {
        validate_opaque_identifier(project_id, "prj_", "project_id")?;
        validate_backup_transition_identity(transition_kind, transition_id)?;
        let connection = self.open_connection(false)?;
        Ok(connection
            .query_row(
                "SELECT transition_kind, transition_id, project_id, workspace_id, task_id, task_revision, state_revision, commit_id, parent_commit_id, source_fingerprint, entry_count, byte_count, recorded_at FROM backup_checkpoints WHERE project_id = ?1 AND transition_kind = ?2 AND transition_id = ?3",
                AC_params![project_id, transition_kind, transition_id],
                backup_checkpoint_from_row,
            )
            .optional()?)
    }

    fn backup_checkpoint_recovery_required(
        &self,
        project_id: &str,
        workspace_id: &str,
    ) -> TransactionalStorageResult<bool> {
        validate_opaque_identifier(project_id, "prj_", "project_id")?;
        validate_opaque_identifier(workspace_id, "wsp_", "workspace_id")?;
        let connection = self.open_connection(false)?;
        let required = connection.query_row(
            "SELECT CASE WHEN NOT EXISTS (SELECT 1 FROM backup_checkpoints WHERE project_id = ?1 AND transition_kind = 'baseline' AND transition_id = 'baseline') OR EXISTS (SELECT 1 FROM submissions s WHERE s.project_id = ?1 AND s.workspace_id = ?2 AND NOT EXISTS (SELECT 1 FROM backup_checkpoints b WHERE b.project_id = s.project_id AND b.transition_kind = 'submission' AND b.transition_id = s.submission_id)) OR EXISTS (SELECT 1 FROM reviews r JOIN submissions s ON s.project_id = r.project_id AND s.submission_id = r.submission_id WHERE r.project_id = ?1 AND s.workspace_id = ?2 AND NOT EXISTS (SELECT 1 FROM backup_checkpoints b WHERE b.project_id = r.project_id AND b.transition_kind = 'review' AND b.transition_id = r.review_id)) THEN 1 ELSE 0 END",
            AC_params![project_id, workspace_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(required == 1)
    }
}

fn record_backup_checkpoint_in_transaction(
    transaction: &AC_Transaction<'_>,
    checkpoint: &BackupCheckpointRecord,
) -> TransactionalStorageResult<bool> {
    validate_backup_checkpoint(checkpoint)?;
    let existing = transaction
        .query_row(
            "SELECT transition_kind, transition_id, project_id, workspace_id, task_id, task_revision, state_revision, commit_id, parent_commit_id, source_fingerprint, entry_count, byte_count, recorded_at FROM backup_checkpoints WHERE project_id = ?1 AND transition_kind = ?2 AND transition_id = ?3",
            AC_params![
                checkpoint.project_id,
                checkpoint.transition_kind,
                checkpoint.transition_id,
            ],
            backup_checkpoint_from_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        if same_backup_checkpoint_binding(&existing, checkpoint) {
            return Ok(false);
        }
        return Err(TransactionalStorageError::Integrity(
            "backup checkpoint identity is already bound to different content".to_string(),
        ));
    }
    let workspace_project = transaction.query_row(
        "SELECT project_id FROM workspaces WHERE workspace_id = ?1 AND status = 'active'",
        AC_params![checkpoint.workspace_id],
        |row| row.get::<_, String>(0),
    )?;
    if workspace_project != checkpoint.project_id {
        return Err(TransactionalStorageError::Integrity(
            "backup checkpoint workspace does not belong to the project".to_string(),
        ));
    }
    if let Some(task_id) = checkpoint.task_id.as_deref() {
        let task = transaction.query_row(
            "SELECT project_id, task_revision FROM tasks WHERE task_id = ?1",
            AC_params![task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        if task.0 != checkpoint.project_id || Some(task.1) != checkpoint.task_revision {
            return Err(TransactionalStorageError::Integrity(
                "backup checkpoint task does not match canonical state".to_string(),
            ));
        }
    }
    transaction.execute(
        "INSERT INTO backup_checkpoints(transition_kind, transition_id, project_id, workspace_id, task_id, task_revision, state_revision, commit_id, parent_commit_id, source_fingerprint, entry_count, byte_count, recorded_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        AC_params![
            checkpoint.transition_kind,
            checkpoint.transition_id,
            checkpoint.project_id,
            checkpoint.workspace_id,
            checkpoint.task_id,
            checkpoint.task_revision,
            checkpoint.state_revision,
            checkpoint.commit_id,
            checkpoint.parent_commit_id,
            checkpoint.source_fingerprint,
            checkpoint.entry_count,
            checkpoint.byte_count,
            checkpoint.recorded_at,
        ],
    )?;
    Ok(true)
}

fn same_backup_checkpoint_binding(
    left: &BackupCheckpointRecord,
    right: &BackupCheckpointRecord,
) -> bool {
    left.transition_kind == right.transition_kind
        && left.transition_id == right.transition_id
        && left.project_id == right.project_id
        && left.workspace_id == right.workspace_id
        && left.task_id == right.task_id
        && left.task_revision == right.task_revision
        && left.state_revision == right.state_revision
        && left.commit_id == right.commit_id
        && left.parent_commit_id == right.parent_commit_id
        && left.source_fingerprint == right.source_fingerprint
        && left.entry_count == right.entry_count
        && left.byte_count == right.byte_count
}

fn backup_checkpoint_from_row(
    row: &crate::libres::Rusqlite_unit::AC_Row<'_>,
) -> Result<BackupCheckpointRecord, crate::libres::Rusqlite_unit::AC_Error> {
    Ok(BackupCheckpointRecord {
        transition_kind: row.get(0)?,
        transition_id: row.get(1)?,
        project_id: row.get(2)?,
        workspace_id: row.get(3)?,
        task_id: row.get(4)?,
        task_revision: row.get(5)?,
        state_revision: row.get(6)?,
        commit_id: row.get(7)?,
        parent_commit_id: row.get(8)?,
        source_fingerprint: row.get(9)?,
        entry_count: row.get(10)?,
        byte_count: row.get(11)?,
        recorded_at: row.get(12)?,
    })
}

fn validate_backup_checkpoint(
    checkpoint: &BackupCheckpointRecord,
) -> TransactionalStorageResult<()> {
    validate_backup_transition_identity(&checkpoint.transition_kind, &checkpoint.transition_id)?;
    validate_opaque_identifier(&checkpoint.project_id, "prj_", "project_id")?;
    validate_opaque_identifier(&checkpoint.workspace_id, "wsp_", "workspace_id")?;
    if let Some(task_id) = checkpoint.task_id.as_deref() {
        validate_opaque_identifier(task_id, "tsk_", "task_id")?;
    }
    if checkpoint.task_id.is_some() != checkpoint.task_revision.is_some()
        || checkpoint
            .task_revision
            .is_some_and(|revision| revision < 1)
        || checkpoint.state_revision < 0
        || checkpoint.entry_count < 1
        || checkpoint.byte_count < 0
        || !valid_lowercase_hex(&checkpoint.commit_id, 40)
        || checkpoint
            .parent_commit_id
            .as_deref()
            .is_some_and(|id| !valid_lowercase_hex(id, 40))
    {
        return Err(TransactionalStorageError::InvalidInput(
            "backup checkpoint metadata is invalid".to_string(),
        ));
    }
    validate_fingerprint(&checkpoint.source_fingerprint)?;
    validate_required("recorded_at", &checkpoint.recorded_at)
}

fn validate_backup_transition_identity(
    transition_kind: &str,
    transition_id: &str,
) -> TransactionalStorageResult<()> {
    let valid = match transition_kind {
        "baseline" => transition_id == "baseline",
        "submission" => validate_opaque_identifier(transition_id, "sub_", "transition_id").is_ok(),
        "review" | "advance" => {
            validate_opaque_identifier(transition_id, "rev_", "transition_id").is_ok()
        }
        _ => false,
    };
    if !valid {
        return Err(TransactionalStorageError::InvalidInput(
            "backup checkpoint transition identity is invalid".to_string(),
        ));
    }
    Ok(())
}

fn valid_lowercase_hex(value: &str, expected_length: usize) -> bool {
    value.len() == expected_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl AuditStore for ProjectDatabase {
    fn append_audit_event(
        &self,
        event: &NewAuditEvent,
    ) -> TransactionalStorageResult<EventWriteOutcome> {
        self.record_event(event)
    }

    fn audit_event_count(&self, project_id: &str) -> TransactionalStorageResult<i64> {
        self.event_count(project_id)
    }

    fn last_audit_sequence(&self, project_id: &str) -> TransactionalStorageResult<i64> {
        self.last_event_sequence(project_id)
    }
}

impl AtomicProjectProjectionWriter {
    pub fn new(
        database: ProjectDatabase,
        state_root: super::ProjectStateRoot,
    ) -> TransactionalStorageResult<Self> {
        if database.database_path() != state_root.database_path() {
            return Err(TransactionalStorageError::InvalidInput(
                "projection writer database does not belong to the supplied project state root"
                    .to_string(),
            ));
        }
        materialize_project_guard(&state_root).map_err(|_| {
            TransactionalStorageError::InvalidInput(
                "project guard bootstrap does not satisfy lifecycle-v1".to_string(),
            )
        })?;
        Ok(Self {
            database,
            state_root,
        })
    }

    fn persist_projection(
        &self,
        artifact_kind: ProjectionArtifactKind,
        metadata: &ProjectionMetadata,
        current_task_ref: Option<&CanonicalTaskProjectionRef>,
        governance_snapshot_fingerprint: Option<&str>,
        serialized_contract: &[u8],
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<ProjectionWriteReceipt> {
        validate_projection(metadata, serialized_contract, audit_event)?;
        let destination = match artifact_kind {
            ProjectionArtifactKind::CurrentTask => self.state_root.todo_projection_path(),
            ProjectionArtifactKind::GovernanceErrors => {
                self.state_root.governance_error_projection_path()
            }
        };
        let (audit, generation) = self.database.with_immediate_transaction(|transaction| {
            rusqlite_query::QueryConnection::ensure_workspace_scope(
                transaction,
                &metadata.project_id,
                &metadata.workspace_id,
            )?;
            if artifact_kind == ProjectionArtifactKind::CurrentTask {
                validate_current_task_projection_binding(
                    transaction,
                    &metadata.project_id,
                    current_task_ref,
                )?;
            } else {
                validate_governance_projection_binding(
                    transaction,
                    &metadata.project_id,
                    &metadata.workspace_id,
                    metadata.source_revision,
                    governance_snapshot_fingerprint,
                )?;
            }
            let state_revision = transaction.query_row(
                "SELECT state_revision FROM projects WHERE project_id = ?1",
                AC_params![metadata.project_id],
                |row| row.get::<_, i64>(0),
            )?;
            if state_revision != metadata.source_revision {
                return Err(TransactionalStorageError::Integrity(
                    "projection source_revision does not match canonical state".to_string(),
                ));
            }
            let audit = self
                .database
                .record_event_in_transaction(transaction, audit_event)?;
            let generation = if audit.inserted() {
                validate_projection_generation(transaction, artifact_kind, metadata)?;
                transaction.execute(
                    "INSERT INTO projection_versions(project_id, artifact_kind, contract_version, generation, source_revision, fingerprint, rendered_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(project_id, artifact_kind) DO UPDATE SET contract_version = excluded.contract_version, generation = excluded.generation, source_revision = excluded.source_revision, fingerprint = excluded.fingerprint, rendered_at = excluded.rendered_at",
                    AC_params![
                        metadata.project_id,
                        artifact_kind.storage_name(),
                        AGENT_PROJECTION_CONTRACT_VERSION,
                        metadata.generation,
                        metadata.source_revision,
                        metadata.fingerprint,
                        metadata.rendered_at,
                    ],
                )?;
                transaction.execute(
                    "UPDATE projects SET projection_generation = MAX(projection_generation, ?2) WHERE project_id = ?1",
                    AC_params![metadata.project_id, metadata.generation],
                )?;
                metadata.generation
            } else {
                validate_persisted_projection(transaction, artifact_kind, metadata)?
            };
            super::Transactional_Storage_structcore::atomic_replace_if_changed(
                &destination,
                serialized_contract,
            )?;
            Ok((audit, generation))
        })?;
        Ok(ProjectionWriteReceipt {
            artifact_kind,
            generation,
            audit,
        })
    }
}

impl ProjectProjectionWriter for AtomicProjectProjectionWriter {
    fn write_current_task(
        &self,
        projection: &CurrentTaskProjection,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<ProjectionWriteReceipt> {
        validate_contract_version(
            &projection.contract_version,
            AGENT_PROJECTION_CONTRACT_VERSION,
        )?;
        self.persist_projection(
            ProjectionArtifactKind::CurrentTask,
            &projection.metadata,
            projection.task_ref.as_ref(),
            None,
            &projection.serialized_contract,
            audit_event,
        )
    }

    fn write_governance_errors(
        &self,
        projection: &GovernanceErrorsProjection,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<ProjectionWriteReceipt> {
        validate_contract_version(
            &projection.contract_version,
            AGENT_PROJECTION_CONTRACT_VERSION,
        )?;
        self.persist_projection(
            ProjectionArtifactKind::GovernanceErrors,
            &projection.metadata,
            None,
            projection.snapshot_fingerprint.as_deref(),
            &projection.serialized_contract,
            audit_event,
        )
    }

    fn current_task_path(&self) -> std::path::PathBuf {
        self.state_root.todo_projection_path()
    }

    fn governance_errors_path(&self) -> std::path::PathBuf {
        self.state_root.governance_error_projection_path()
    }
}

fn validate_governance_projection_binding(
    transaction: &AC_Transaction<'_>,
    project_id: &str,
    workspace_id: &str,
    source_revision: i64,
    snapshot_fingerprint: Option<&str>,
) -> TransactionalStorageResult<()> {
    let fingerprint = snapshot_fingerprint.ok_or_else(|| {
        TransactionalStorageError::Integrity(
            "governance projection is not bound to a canonical snapshot".to_string(),
        )
    })?;
    let matched = transaction
        .query_row(
            "SELECT 1 FROM governance_snapshots WHERE project_id = ?1 AND workspace_id = ?2 AND source_revision = ?3 AND fingerprint = ?4",
            AC_params![project_id, workspace_id, source_revision, fingerprint],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if matched != Some(1) {
        return Err(TransactionalStorageError::Integrity(
            "governance projection does not match the canonical snapshot".to_string(),
        ));
    }
    Ok(())
}

fn load_canonical_state_from_connection(
    connection: &crate::libres::Rusqlite_unit::AC_Connection,
    project_id: &str,
    workspace_id: &str,
) -> TransactionalStorageResult<CanonicalStateSnapshot> {
    rusqlite_query::QueryConnection::ensure_workspace_scope(connection, project_id, workspace_id)?;
    let project = connection.query_row(
        "SELECT state_revision, active_plan_id, current_task_id, last_event_sequence, projection_generation FROM projects WHERE project_id = ?1",
        AC_params![project_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )?;
    let tasks = load_tasks(connection, project_id)?;
    Ok(build_snapshot(project_id, workspace_id, project, tasks))
}

fn load_canonical_state_from_transaction(
    transaction: &AC_Transaction<'_>,
    project_id: &str,
    workspace_id: &str,
) -> TransactionalStorageResult<CanonicalStateSnapshot> {
    rusqlite_query::QueryConnection::ensure_workspace_scope(transaction, project_id, workspace_id)?;
    let project = transaction.query_row(
        "SELECT state_revision, active_plan_id, current_task_id, last_event_sequence, projection_generation FROM projects WHERE project_id = ?1",
        AC_params![project_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )?;
    let tasks = load_tasks(transaction, project_id)?;
    Ok(build_snapshot(project_id, workspace_id, project, tasks))
}

fn load_tasks(
    connection: &impl rusqlite_query::QueryConnection,
    project_id: &str,
) -> TransactionalStorageResult<Vec<CanonicalTaskState>> {
    connection.query_tasks(project_id)
}

mod rusqlite_query {
    use super::*;
    use crate::libres::Rusqlite_unit::AC_Connection;

    pub trait QueryConnection {
        fn ensure_workspace_scope(
            &self,
            project_id: &str,
            workspace_id: &str,
        ) -> TransactionalStorageResult<()>;

        fn query_tasks(
            &self,
            project_id: &str,
        ) -> TransactionalStorageResult<Vec<CanonicalTaskState>>;
    }

    macro_rules! implement_query_connection {
        ($connection:ty) => {
            impl QueryConnection for $connection {
                fn ensure_workspace_scope(
                    &self,
                    project_id: &str,
                    workspace_id: &str,
                ) -> TransactionalStorageResult<()> {
                    let workspace_project = self.query_row(
                        "SELECT project_id FROM workspaces WHERE workspace_id = ?1",
                        AC_params![workspace_id],
                        |row| row.get::<_, String>(0),
                    )?;
                    if workspace_project != project_id {
                        return Err(TransactionalStorageError::InvalidInput(
                            "workspace does not belong to the requested project".to_string(),
                        ));
                    }
                    Ok(())
                }

                fn query_tasks(
                    &self,
                    project_id: &str,
                ) -> TransactionalStorageResult<Vec<CanonicalTaskState>> {
                    let mut statement = self.prepare(
                        "SELECT task_id, task_revision, status, review_status FROM tasks WHERE project_id = ?1 ORDER BY task_id",
                    )?;
                    let rows = statement.query_map(AC_params![project_id], |row| {
                        Ok(CanonicalTaskState {
                            task_id: row.get(0)?,
                            task_revision: row.get(1)?,
                            status: row.get(2)?,
                            review_status: row.get(3)?,
                        })
                    })?;
                    let mut tasks = Vec::new();
                    for row in rows {
                        tasks.push(row?);
                    }
                    Ok(tasks)
                }
            }
        };
    }

    implement_query_connection!(AC_Connection);
    implement_query_connection!(AC_Transaction<'_>);
}

fn build_snapshot(
    project_id: &str,
    workspace_id: &str,
    project: (i64, Option<String>, Option<String>, i64, i64),
    tasks: Vec<CanonicalTaskState>,
) -> CanonicalStateSnapshot {
    CanonicalStateSnapshot {
        contract_version: CANONICAL_STATE_CONTRACT_VERSION.to_string(),
        project_id: project_id.to_string(),
        workspace_id: workspace_id.to_string(),
        state_revision: project.0,
        active_plan_id: project.1,
        current_task_id: project.2,
        tasks,
        last_event_sequence: project.3,
        projection_generation: project.4,
    }
}

fn validate_transition(
    transition: &CanonicalStateTransition,
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<()> {
    validate_required("project_id", &transition.project_id)?;
    validate_required("workspace_id", &transition.workspace_id)?;
    if transition.expected_state_revision < 0 {
        return Err(TransactionalStorageError::InvalidInput(
            "expected_state_revision must not be negative".to_string(),
        ));
    }
    validate_audit_binding(audit_event, &transition.project_id, "state_transitioned")
}

fn validate_submission(
    submission: &NewSubmission,
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<()> {
    validate_contract_version(&submission.contract_version, SUBMISSION_CONTRACT_VERSION)?;
    for (field, value) in [
        ("submission_id", submission.submission_id.as_str()),
        ("request_id", submission.request_id.as_str()),
        ("project_id", submission.project_id.as_str()),
        ("workspace_id", submission.workspace_id.as_str()),
        ("task_id", submission.task_id.as_str()),
        ("received_at", submission.received_at.as_str()),
    ] {
        validate_required(field, value)?;
    }
    if submission.task_revision < 1 {
        return Err(TransactionalStorageError::InvalidInput(
            "task_revision must be at least one".to_string(),
        ));
    }
    validate_fingerprint(&submission.fingerprint)?;
    validate_audit_binding(audit_event, &submission.project_id, "submission_recorded")?;
    if audit_event.workspace_id.as_deref() != Some(submission.workspace_id.as_str()) {
        return Err(TransactionalStorageError::InvalidInput(
            "submission audit event must identify the same workspace".to_string(),
        ));
    }
    if audit_event.fingerprint != submission.fingerprint
        || audit_event.resource_id.as_deref() != Some(submission.submission_id.as_str())
    {
        return Err(TransactionalStorageError::InvalidInput(
            "submission audit event must bind the submission id and fingerprint".to_string(),
        ));
    }
    Ok(())
}

fn validate_review(
    review: &NewReview,
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<()> {
    for (field, value) in [
        ("review_id", review.review_id.as_str()),
        ("project_id", review.project_id.as_str()),
        ("submission_id", review.submission_id.as_str()),
        ("reviewer_id", review.reviewer_id.as_str()),
        ("created_at", review.created_at.as_str()),
    ] {
        validate_required(field, value)?;
    }
    if review.payload.is_empty() {
        return Err(TransactionalStorageError::InvalidInput(
            "review payload must not be empty".to_string(),
        ));
    }
    validate_fingerprint(&review.fingerprint)?;
    validate_audit_binding(audit_event, &review.project_id, "review_recorded")?;
    if audit_event.fingerprint != review.fingerprint
        || audit_event.resource_id.as_deref() != Some(review.review_id.as_str())
        || !matches!(audit_event.actor_kind.as_str(), "reviewer" | "operator")
        || audit_event.actor_subject_id != review.reviewer_id
    {
        return Err(TransactionalStorageError::InvalidInput(
            "review audit event does not match the authorized review".to_string(),
        ));
    }
    Ok(())
}

fn validate_projection(
    metadata: &ProjectionMetadata,
    serialized_contract: &[u8],
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<()> {
    validate_required("project_id", &metadata.project_id)?;
    validate_required("workspace_id", &metadata.workspace_id)?;
    validate_required("rendered_at", &metadata.rendered_at)?;
    validate_fingerprint(&metadata.fingerprint)?;
    if metadata.source_revision < 0 || metadata.generation < 0 {
        return Err(TransactionalStorageError::InvalidInput(
            "projection revisions must not be negative".to_string(),
        ));
    }
    if serialized_contract.is_empty() {
        return Err(TransactionalStorageError::InvalidInput(
            "serialized projection contract must not be empty".to_string(),
        ));
    }
    super::validate_serialized_agent_projection_document(
        serialized_contract,
        "serialized projection contract",
    )?;
    validate_audit_binding(audit_event, &metadata.project_id, "projection_rendered")?;
    if audit_event.workspace_id.as_deref() != Some(metadata.workspace_id.as_str()) {
        return Err(TransactionalStorageError::InvalidInput(
            "projection audit event must identify the same workspace".to_string(),
        ));
    }
    if audit_event.fingerprint != metadata.fingerprint {
        return Err(TransactionalStorageError::InvalidInput(
            "projection audit event must use the projection fingerprint".to_string(),
        ));
    }
    Ok(())
}

fn validate_audit_binding(
    event: &NewAuditEvent,
    project_id: &str,
    event_kind: &str,
) -> TransactionalStorageResult<()> {
    super::Transactional_Storage_structcore::validate_event(event)?;
    if event.project_id != project_id || event.event_kind != event_kind {
        return Err(TransactionalStorageError::InvalidInput(format!(
            "audit event must be {event_kind} for the same project"
        )));
    }
    Ok(())
}

fn validate_projection_generation(
    transaction: &AC_Transaction<'_>,
    artifact_kind: ProjectionArtifactKind,
    metadata: &ProjectionMetadata,
) -> TransactionalStorageResult<()> {
    let project_generation = transaction.query_row(
        "SELECT projection_generation FROM projects WHERE project_id = ?1",
        AC_params![metadata.project_id],
        |row| row.get::<_, i64>(0),
    )?;
    if metadata.generation <= project_generation {
        return Err(TransactionalStorageError::Integrity(
            "projection generation must advance the project generation".to_string(),
        ));
    }
    let previous = transaction
        .query_row(
            "SELECT generation FROM projection_versions WHERE project_id = ?1 AND artifact_kind = ?2",
            AC_params![metadata.project_id, artifact_kind.storage_name()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if previous.is_some_and(|generation| metadata.generation <= generation) {
        return Err(TransactionalStorageError::Integrity(
            "projection generation must advance monotonically".to_string(),
        ));
    }
    Ok(())
}

fn validate_current_task_projection_binding(
    transaction: &AC_Transaction<'_>,
    project_id: &str,
    projected: Option<&CanonicalTaskProjectionRef>,
) -> TransactionalStorageResult<()> {
    let current_task_id = transaction.query_row(
        "SELECT current_task_id FROM projects WHERE project_id = ?1",
        AC_params![project_id],
        |row| row.get::<_, Option<String>>(0),
    )?;
    match (current_task_id.as_deref(), projected) {
        (None, None) => Ok(()),
        (Some(current_task_id), Some(projected)) if current_task_id == projected.task_id => {
            let revision = transaction
                .query_row(
                    "SELECT t.task_revision FROM tasks t JOIN projects p ON p.project_id = t.project_id AND p.active_plan_id = t.plan_id WHERE t.project_id = ?1 AND t.task_id = ?2",
                    AC_params![project_id, projected.task_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    TransactionalStorageError::Integrity(
                        "canonical current task does not belong to the active plan".to_string(),
                    )
                })?;
            if revision != projected.task_revision {
                return Err(TransactionalStorageError::Integrity(
                    "projected task revision does not match canonical current task".to_string(),
                ));
            }
            Ok(())
        }
        _ => Err(TransactionalStorageError::Integrity(
            "projection does not match the canonical current task".to_string(),
        )),
    }
}

fn validate_required(field: &str, value: &str) -> TransactionalStorageResult<()> {
    if value.trim().is_empty() {
        return Err(TransactionalStorageError::InvalidInput(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

fn validate_contract_version(found: &str, expected: &str) -> TransactionalStorageResult<()> {
    if found != expected {
        return Err(TransactionalStorageError::InvalidInput(format!(
            "unsupported contract_version {found}; expected {expected}"
        )));
    }
    Ok(())
}

fn validate_fingerprint(fingerprint: &str) -> TransactionalStorageResult<()> {
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(TransactionalStorageError::InvalidInput(
            "fingerprint must contain exactly 64 lowercase hexadecimal characters".to_string(),
        ));
    }
    Ok(())
}

fn find_duplicate_submission(
    transaction: &AC_Transaction<'_>,
    submission: &NewSubmission,
) -> TransactionalStorageResult<Option<SubmissionWriteOutcome>> {
    let by_request = transaction
        .query_row(
            "SELECT submission_id, fingerprint FROM submissions WHERE project_id = ?1 AND request_id = ?2",
            AC_params![submission.project_id, submission.request_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((submission_id, fingerprint)) = by_request {
        if fingerprint != submission.fingerprint {
            return Err(TransactionalStorageError::Integrity(
                "request_id was reused with a different submission fingerprint".to_string(),
            ));
        }
        return persisted_submission_outcome(transaction, &submission.project_id, submission_id);
    }
    let by_fingerprint = transaction
        .query_row(
            "SELECT submission_id FROM submissions WHERE project_id = ?1 AND fingerprint = ?2",
            AC_params![submission.project_id, submission.fingerprint],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match by_fingerprint {
        Some(submission_id) => {
            persisted_submission_outcome(transaction, &submission.project_id, submission_id)
        }
        None => Ok(None),
    }
}

fn persisted_submission_outcome(
    transaction: &AC_Transaction<'_>,
    project_id: &str,
    submission_id: String,
) -> TransactionalStorageResult<Option<SubmissionWriteOutcome>> {
    let sequence = transaction
        .query_row(
            "SELECT sequence FROM events WHERE project_id = ?1 AND event_kind = 'submission_recorded' AND resource_id = ?2",
            AC_params![project_id, submission_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| {
            TransactionalStorageError::Integrity(
                "persisted submission is missing its audit event".to_string(),
            )
        })?;
    Ok(Some(SubmissionWriteOutcome::Duplicate {
        submission_id,
        event_sequence: sequence,
    }))
}

fn find_duplicate_review(
    transaction: &AC_Transaction<'_>,
    review: &NewReview,
) -> TransactionalStorageResult<Option<ReviewWriteOutcome>> {
    let persisted = transaction
        .query_row(
            "SELECT review_id, reviewer_id, outcome, payload FROM reviews WHERE project_id = ?1 AND submission_id = ?2",
            AC_params![review.project_id, review.submission_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((review_id, reviewer_id, outcome, payload)) = persisted else {
        return Ok(None);
    };
    if reviewer_id != review.reviewer_id
        || outcome != review.decision.storage_value()
        || payload != review.payload
    {
        return Err(TransactionalStorageError::Integrity(
            "submission already has a different immutable review".to_string(),
        ));
    }
    let sequence = transaction
        .query_row(
            "SELECT sequence FROM events WHERE project_id = ?1 AND event_kind = 'review_recorded' AND resource_id = ?2",
            AC_params![review.project_id, review_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| {
            TransactionalStorageError::Integrity(
                "persisted review is missing its audit event".to_string(),
            )
        })?;
    Ok(Some(ReviewWriteOutcome::Duplicate {
        review_id,
        event_sequence: sequence,
    }))
}

fn validate_submission_relationships(
    transaction: &AC_Transaction<'_>,
    submission: &NewSubmission,
) -> TransactionalStorageResult<()> {
    let workspace_project = transaction.query_row(
        "SELECT project_id FROM workspaces WHERE workspace_id = ?1",
        AC_params![submission.workspace_id],
        |row| row.get::<_, String>(0),
    )?;
    if workspace_project != submission.project_id {
        return Err(TransactionalStorageError::InvalidInput(
            "submission workspace does not belong to the project".to_string(),
        ));
    }
    let task = transaction.query_row(
        "SELECT project_id, task_revision FROM tasks WHERE task_id = ?1",
        AC_params![submission.task_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )?;
    if task.0 != submission.project_id || task.1 != submission.task_revision {
        return Err(TransactionalStorageError::InvalidInput(
            "submission task identity or revision is stale".to_string(),
        ));
    }
    Ok(())
}

fn validate_review_relationships(
    transaction: &AC_Transaction<'_>,
    review: &NewReview,
) -> TransactionalStorageResult<()> {
    let submission_project = transaction.query_row(
        "SELECT project_id FROM submissions WHERE submission_id = ?1",
        AC_params![review.submission_id],
        |row| row.get::<_, String>(0),
    )?;
    if submission_project != review.project_id {
        return Err(TransactionalStorageError::InvalidInput(
            "review submission does not belong to the authorized project".to_string(),
        ));
    }
    Ok(())
}

fn validate_persisted_projection(
    transaction: &AC_Transaction<'_>,
    artifact_kind: ProjectionArtifactKind,
    metadata: &ProjectionMetadata,
) -> TransactionalStorageResult<i64> {
    let persisted = transaction
        .query_row(
            "SELECT contract_version, generation, source_revision, fingerprint FROM projection_versions WHERE project_id = ?1 AND artifact_kind = ?2",
            AC_params![metadata.project_id, artifact_kind.storage_name()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    match persisted {
        Some((contract_version, generation, source_revision, fingerprint))
            if contract_version == AGENT_PROJECTION_CONTRACT_VERSION
                && source_revision == metadata.source_revision
                && fingerprint == metadata.fingerprint =>
        {
            Ok(generation)
        }
        _ => Err(TransactionalStorageError::Integrity(
            "duplicate projection event does not match persisted projection metadata".to_string(),
        )),
    }
}
