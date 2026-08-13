use crate::libres::Rusqlite_unit::{AC_OptionalExtension, AC_Transaction, AC_params};

use super::{
    atomic_replace, materialize_project_guard, AtomicProjectProjectionWriter, AuditStore,
    CanonicalMutationOutcome, CanonicalStateSnapshot, CanonicalStateStore,
    CanonicalStateTransition, CanonicalTaskProjectionRef, CanonicalTaskState,
    CurrentTaskProjection, EventWriteOutcome, GovernanceErrorsProjection, NewAuditEvent, NewReview,
    NewSubmission, ProjectDatabase, ProjectProjectionWriter, ProjectionArtifactKind,
    ProjectionMetadata, ProjectionWriteReceipt, ReviewStore, ReviewWriteOutcome, StorageProject,
    StorageWorkspace, SubmissionStore, SubmissionWriteOutcome, TransactionalStorageError,
    TransactionalStorageResult, AGENT_PROJECTION_CONTRACT_VERSION,
    CANONICAL_STATE_CONTRACT_VERSION, SUBMISSION_CONTRACT_VERSION,
};

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
}

impl SubmissionStore for ProjectDatabase {
    fn record_submission(
        &self,
        submission: &NewSubmission,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<SubmissionWriteOutcome> {
        validate_submission(submission, audit_event)?;
        self.with_immediate_transaction(|transaction| {
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
            let audit = self.record_event_in_transaction(transaction, audit_event)?;
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

    fn submission_count(&self, project_id: &str) -> TransactionalStorageResult<i64> {
        validate_required("project_id", project_id)?;
        let connection = self.open_connection(false)?;
        Ok(connection.query_row(
            "SELECT count(*) FROM submissions WHERE project_id = ?1",
            AC_params![project_id],
            |row| row.get(0),
        )?)
    }
}

impl ReviewStore for ProjectDatabase {
    fn record_review(
        &self,
        review: &NewReview,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<ReviewWriteOutcome> {
        validate_review(review, audit_event)?;
        self.with_immediate_transaction(|transaction| {
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
            let audit = self.record_event_in_transaction(transaction, audit_event)?;
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
        let audit = self.database.with_immediate_transaction(|transaction| {
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
            if audit.inserted() {
                validate_projection_generation(transaction, artifact_kind, metadata)?;
                atomic_replace(&destination, serialized_contract)?;
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
            } else {
                validate_persisted_projection(transaction, artifact_kind, metadata)?;
                atomic_replace(&destination, serialized_contract)?;
            }
            Ok(audit)
        })?;
        Ok(ProjectionWriteReceipt {
            artifact_kind,
            generation: metadata.generation,
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
    super::validate_serialized_public_document(
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
    if review_id != review.review_id
        || reviewer_id != review.reviewer_id
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
            AC_params![review.project_id, review.review_id],
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
) -> TransactionalStorageResult<()> {
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
    if persisted
        != Some((
            AGENT_PROJECTION_CONTRACT_VERSION.to_string(),
            metadata.generation,
            metadata.source_revision,
            metadata.fingerprint.clone(),
        ))
    {
        return Err(TransactionalStorageError::Integrity(
            "duplicate projection event does not match persisted projection metadata".to_string(),
        ));
    }
    Ok(())
}
