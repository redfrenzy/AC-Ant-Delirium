use crate::libres::Rusqlite_unit::{AC_OptionalExtension, AC_Transaction, AC_params};
use crate::libres::Sha2_unit::sha256;
use crate::libres::System_Random_unit::random_crockford_id;

use super::{
    reject_protected_administrative_references, reject_public_path_references,
    validate_agent_visible_project_path, GovernanceErrorsProjection,
    GovernanceErrorsProjectionDocument, ProjectedGovernanceViolation, TransactionalStorageError,
    TransactionalStorageResult, AGENT_PROJECTION_CONTRACT_VERSION,
};
use super::{
    AtomicProjectProjectionWriter, CanonicalGovernanceSnapshotRequest, EventWriteOutcome,
    GovernanceErrorsRefreshRequest, NewAuditEvent, ProjectDatabase, ProjectProjectionWriter,
    ProjectionMetadata, ProjectionWriteReceipt, AUDIT_EVENT_CONTRACT_VERSION,
    CANONICAL_GOVERNANCE_SNAPSHOT_VERSION,
};

use super::Current_Task_Projection_structcore::{
    json_escape, validate_projection_metadata, validate_text,
};

const SNAPSHOT_MAGIC: &[u8] = b"canonical-governance-snapshot-v1\0";
const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
pub(super) fn build_governance_errors_projection(
    document: &GovernanceErrorsProjectionDocument,
) -> TransactionalStorageResult<GovernanceErrorsProjection> {
    validate_document(document)?;
    Ok(GovernanceErrorsProjection {
        contract_version: AGENT_PROJECTION_CONTRACT_VERSION.to_string(),
        metadata: document.metadata.clone(),
        snapshot_fingerprint: None,
        serialized_contract: serialize_document(document).into_bytes(),
    })
}

pub(super) fn is_agent_correctable_violation_path(path: &str) -> bool {
    validate_agent_visible_project_path(path, "violation path").is_ok()
}

pub(super) fn replace_canonical_governance_snapshot(
    writer: &AtomicProjectProjectionWriter,
    request: &CanonicalGovernanceSnapshotRequest,
) -> TransactionalStorageResult<EventWriteOutcome> {
    replace_canonical_governance_snapshot_in_database(&writer.database, request)
}

pub(super) fn replace_canonical_governance_snapshot_in_database(
    database: &ProjectDatabase,
    request: &CanonicalGovernanceSnapshotRequest,
) -> TransactionalStorageResult<EventWriteOutcome> {
    validate_text(&request.project_id, 1, 255, "project_id")?;
    validate_text(&request.workspace_id, 1, 255, "workspace_id")?;
    validate_text(&request.observed_at, 1, 255, "observed_at")?;
    if request.violations.len() > 4_096 {
        return Err(invalid(
            "canonical governance snapshot exceeds the violation limit",
        ));
    }
    for violation in &request.violations {
        validate_violation(violation)?;
        if !is_agent_correctable_violation_path(&violation.path) {
            return Err(invalid(
                "canonical governance snapshot contains an administrative path",
            ));
        }
    }
    let payload = encode_snapshot(&request.violations)?;
    let event_id = random_crockford_id("evt_")?;
    let correlation_id = random_crockford_id("ref_")?;
    database.with_immediate_transaction(|transaction| {
        let source_revision = transaction.query_row(
            "SELECT p.state_revision FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
            AC_params![request.project_id, request.workspace_id],
            |row| row.get::<_, i64>(0),
        )?;
        let fingerprint = snapshot_fingerprint(
            &request.project_id,
            &request.workspace_id,
            source_revision,
            &payload,
        )?;
        transaction.execute(
            "INSERT INTO governance_snapshots(project_id, workspace_id, contract_version, source_revision, fingerprint, payload, observed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(project_id, workspace_id) DO UPDATE SET contract_version = excluded.contract_version, source_revision = excluded.source_revision, fingerprint = excluded.fingerprint, payload = excluded.payload, observed_at = excluded.observed_at",
            AC_params![request.project_id, request.workspace_id, CANONICAL_GOVERNANCE_SNAPSHOT_VERSION, source_revision, fingerprint, payload, request.observed_at],
        )?;
        let event = NewAuditEvent {
            contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
            event_id,
            project_id: request.project_id.clone(),
            workspace_id: Some(request.workspace_id.clone()),
            actor_kind: "service".to_string(),
            actor_subject_id: "antdelirium-service".to_string(),
            event_kind: "compilation_observed".to_string(),
            outcome: "succeeded".to_string(),
            occurred_at: request.observed_at.clone(),
            fingerprint,
            correlation_id,
            resource_id: Some("governance_errors_snapshot".to_string()),
            payload: b"canonical governance snapshot observed".to_vec(),
        };
        database.record_event_in_transaction(transaction, &event)
    })
}

pub(super) fn initialize_empty_governance_snapshot(
    database: &ProjectDatabase,
    project_id: &str,
    workspace_id: &str,
    observed_at: &str,
) -> TransactionalStorageResult<()> {
    validate_text(project_id, 1, 255, "project_id")?;
    validate_text(workspace_id, 1, 255, "workspace_id")?;
    validate_text(observed_at, 1, 255, "observed_at")?;
    let payload = encode_snapshot(&[])?;
    database.with_immediate_transaction(|transaction| {
        let source_revision = transaction.query_row(
            "SELECT p.state_revision FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
            AC_params![project_id, workspace_id],
            |row| row.get::<_, i64>(0),
        )?;
        let fingerprint = snapshot_fingerprint(
            project_id,
            workspace_id,
            source_revision,
            &payload,
        )?;
        transaction.execute(
            "INSERT INTO governance_snapshots(project_id, workspace_id, contract_version, source_revision, fingerprint, payload, observed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(project_id, workspace_id) DO NOTHING",
            AC_params![project_id, workspace_id, CANONICAL_GOVERNANCE_SNAPSHOT_VERSION, source_revision, fingerprint, payload, observed_at],
        )?;
        Ok(())
    })
}

pub(super) fn carry_forward_canonical_governance_snapshot(
    transaction: &AC_Transaction<'_>,
    project_id: &str,
    workspace_id: &str,
    expected_source_revision: i64,
    next_source_revision: i64,
    observed_at: &str,
) -> TransactionalStorageResult<()> {
    if expected_source_revision < 0
        || expected_source_revision.checked_add(1) != Some(next_source_revision)
    {
        return Err(TransactionalStorageError::InvalidInput(
            "canonical governance snapshot revision transition is invalid".to_string(),
        ));
    }
    let snapshot = transaction.query_row(
        "SELECT contract_version, source_revision, fingerprint, payload FROM governance_snapshots WHERE project_id = ?1 AND workspace_id = ?2",
        AC_params![project_id, workspace_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        },
    )?;
    if snapshot.0 != CANONICAL_GOVERNANCE_SNAPSHOT_VERSION
        || snapshot.1 != expected_source_revision
        || snapshot_fingerprint(
            project_id,
            workspace_id,
            expected_source_revision,
            &snapshot.3,
        )? != snapshot.2
    {
        return Err(TransactionalStorageError::Integrity(
            "canonical governance snapshot cannot follow the state transition".to_string(),
        ));
    }
    let next_fingerprint =
        snapshot_fingerprint(project_id, workspace_id, next_source_revision, &snapshot.3)?;
    let updated = transaction.execute(
        "UPDATE governance_snapshots SET source_revision = ?3, fingerprint = ?4, observed_at = ?5 WHERE project_id = ?1 AND workspace_id = ?2 AND source_revision = ?6 AND fingerprint = ?7",
        AC_params![
            project_id,
            workspace_id,
            next_source_revision,
            next_fingerprint,
            observed_at,
            expected_source_revision,
            snapshot.2,
        ],
    )?;
    if updated != 1 {
        return Err(TransactionalStorageError::Integrity(
            "canonical governance snapshot changed during the state transition".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn refresh_governance_errors_projection(
    writer: &AtomicProjectProjectionWriter,
    request: &GovernanceErrorsRefreshRequest,
) -> TransactionalStorageResult<ProjectionWriteReceipt> {
    validate_text(&request.project_id, 1, 255, "project_id")?;
    validate_text(&request.workspace_id, 1, 255, "workspace_id")?;
    validate_text(&request.rendered_at, 1, 255, "rendered_at")?;
    let connection = writer.database.open_connection(false)?;
    let (source_revision, previous_generation) = connection.query_row(
        "SELECT p.state_revision, p.projection_generation FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
        AC_params![request.project_id, request.workspace_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let snapshot = connection
        .query_row(
            "SELECT contract_version, source_revision, fingerprint, payload FROM governance_snapshots WHERE project_id = ?1 AND workspace_id = ?2",
            AC_params![request.project_id, request.workspace_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, Vec<u8>>(3)?)),
        )
        .optional()?
        .ok_or_else(|| TransactionalStorageError::Integrity("canonical governance snapshot is unavailable".to_string()))?;
    if snapshot.0 != CANONICAL_GOVERNANCE_SNAPSHOT_VERSION || snapshot.1 != source_revision {
        return Err(TransactionalStorageError::Integrity(
            "canonical governance snapshot does not match the current revision".to_string(),
        ));
    }
    if snapshot_fingerprint(
        &request.project_id,
        &request.workspace_id,
        source_revision,
        &snapshot.3,
    )? != snapshot.2
    {
        return Err(TransactionalStorageError::Integrity(
            "canonical governance snapshot fingerprint is invalid".to_string(),
        ));
    }
    let violations = decode_snapshot(&snapshot.3)?;
    let generation = previous_generation.checked_add(1).ok_or_else(|| {
        TransactionalStorageError::Integrity("projection generation overflow".to_string())
    })?;
    drop(connection);

    let mut metadata = ProjectionMetadata {
        project_id: request.project_id.clone(),
        workspace_id: request.workspace_id.clone(),
        source_revision,
        generation,
        fingerprint: "0".repeat(64),
        rendered_at: request.rendered_at.clone(),
    };
    let mut projection = build_governance_errors_projection(&GovernanceErrorsProjectionDocument {
        metadata: metadata.clone(),
        violations: violations.clone(),
    })?;
    metadata.fingerprint = lowercase_hex(&sha256(&projection.serialized_contract));
    projection = build_governance_errors_projection(&GovernanceErrorsProjectionDocument {
        metadata: metadata.clone(),
        violations,
    })?;
    projection.snapshot_fingerprint = Some(snapshot.2);
    let event = NewAuditEvent {
        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
        event_id: random_crockford_id("evt_")?,
        project_id: request.project_id.clone(),
        workspace_id: Some(request.workspace_id.clone()),
        actor_kind: "service".to_string(),
        actor_subject_id: "antdelirium-service".to_string(),
        event_kind: "projection_rendered".to_string(),
        outcome: "succeeded".to_string(),
        occurred_at: request.rendered_at.clone(),
        fingerprint: metadata.fingerprint,
        correlation_id: random_crockford_id("ref_")?,
        resource_id: Some("governance_errors".to_string()),
        payload: b"governance errors projection rendered".to_vec(),
    };
    writer.write_governance_errors(&projection, &event)
}

pub(super) fn render_governance_errors_projection(
    database: &ProjectDatabase,
    project_id: &str,
    workspace_id: &str,
    rendered_at: &str,
) -> TransactionalStorageResult<Vec<u8>> {
    validate_text(project_id, 1, 255, "project_id")?;
    validate_text(workspace_id, 1, 255, "workspace_id")?;
    validate_text(rendered_at, 1, 255, "rendered_at")?;
    let connection = database.open_connection(false)?;
    let (source_revision, previous_generation) = connection.query_row(
        "SELECT p.state_revision, p.projection_generation FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
        AC_params![project_id, workspace_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let snapshot = connection
        .query_row(
            "SELECT contract_version, source_revision, fingerprint, payload FROM governance_snapshots WHERE project_id = ?1 AND workspace_id = ?2",
            AC_params![project_id, workspace_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, Vec<u8>>(3)?)),
        )
        .optional()?
        .ok_or_else(|| TransactionalStorageError::Integrity("canonical governance snapshot is unavailable".to_string()))?;
    if snapshot.0 != CANONICAL_GOVERNANCE_SNAPSHOT_VERSION || snapshot.1 != source_revision {
        return Err(TransactionalStorageError::Integrity(
            "canonical governance snapshot does not match the current revision".to_string(),
        ));
    }
    if snapshot_fingerprint(project_id, workspace_id, source_revision, &snapshot.3)? != snapshot.2 {
        return Err(TransactionalStorageError::Integrity(
            "canonical governance snapshot fingerprint is invalid".to_string(),
        ));
    }
    let violations = decode_snapshot(&snapshot.3)?;
    let generation = previous_generation.checked_add(1).ok_or_else(|| {
        TransactionalStorageError::Integrity("projection generation overflow".to_string())
    })?;
    let mut metadata = ProjectionMetadata {
        project_id: project_id.to_string(),
        workspace_id: workspace_id.to_string(),
        source_revision,
        generation,
        fingerprint: "0".repeat(64),
        rendered_at: rendered_at.to_string(),
    };
    let mut projection = build_governance_errors_projection(&GovernanceErrorsProjectionDocument {
        metadata: metadata.clone(),
        violations: violations.clone(),
    })?;
    metadata.fingerprint = lowercase_hex(&sha256(&projection.serialized_contract));
    projection = build_governance_errors_projection(&GovernanceErrorsProjectionDocument {
        metadata,
        violations,
    })?;
    Ok(projection.serialized_contract)
}

fn validate_document(
    document: &GovernanceErrorsProjectionDocument,
) -> TransactionalStorageResult<()> {
    validate_projection_metadata(&document.metadata)?;
    if document.violations.len() > 4_096 {
        return Err(invalid(
            "governance error projection exceeds the v1 violation limit",
        ));
    }
    for violation in &document.violations {
        validate_violation(violation)?;
    }
    Ok(())
}

fn validate_violation(violation: &ProjectedGovernanceViolation) -> TransactionalStorageResult<()> {
    validate_text(&violation.violation_id, 1, 255, "violation_id")?;
    reject_public_path_references(&violation.violation_id, "violation_id")?;
    reject_protected_administrative_references(&violation.violation_id, "violation_id")?;
    validate_text(&violation.rule_id, 1, 255, "rule_id")?;
    reject_public_path_references(&violation.rule_id, "rule_id")?;
    reject_protected_administrative_references(&violation.rule_id, "rule_id")?;
    validate_text(&violation.summary, 1, 8_192, "violation summary")?;
    reject_public_path_references(&violation.summary, "violation summary")?;
    reject_protected_administrative_references(&violation.summary, "violation summary")?;
    validate_agent_visible_project_path(&violation.path, "violation path")?;
    if violation.line < 1 {
        return Err(invalid("violation line must be at least one"));
    }
    if let Some(quick_fix) = violation.quick_fix.as_deref() {
        validate_text(quick_fix, 0, 8_192, "quick_fix")?;
        reject_public_path_references(quick_fix, "quick_fix")?;
        reject_protected_administrative_references(quick_fix, "quick_fix")?;
    }
    validate_text(
        &violation.public_rule_reference,
        1,
        4_096,
        "public_rule_reference",
    )?;
    reject_public_path_references(&violation.public_rule_reference, "public_rule_reference")?;
    reject_protected_administrative_references(
        &violation.public_rule_reference,
        "public_rule_reference",
    )?;
    Ok(())
}

fn serialize_document(document: &GovernanceErrorsProjectionDocument) -> String {
    let violations = document
        .violations
        .iter()
        .map(serialize_violation)
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{\n  \"contract_version\": \"agent-projection-v1\",\n  \"artifact_kind\": \"governance_errors\",\n  \"project_ref\": \"{}\",\n  \"workspace_ref\": \"{}\",\n  \"source_revision\": {},\n  \"violations\": [\n{}\n  ]\n}}\n",
        json_escape(&document.metadata.project_id),
        json_escape(&document.metadata.workspace_id),
        document.metadata.source_revision,
        violations,
    )
}

fn serialize_violation(violation: &ProjectedGovernanceViolation) -> String {
    let quick_fix = violation
        .quick_fix
        .as_ref()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .unwrap_or_else(|| "null".to_string());
    format!(
        "    {{\n      \"violation_id\": \"{}\",\n      \"rule_id\": \"{}\",\n      \"summary\": \"{}\",\n      \"path\": \"{}\",\n      \"line\": {},\n      \"quick_fix\": {},\n      \"public_rule_reference\": \"{}\"\n    }}",
        json_escape(&violation.violation_id),
        json_escape(&violation.rule_id),
        json_escape(&violation.summary),
        json_escape(&violation.path),
        violation.line,
        quick_fix,
        json_escape(&violation.public_rule_reference),
    )
}

fn encode_snapshot(
    violations: &[ProjectedGovernanceViolation],
) -> TransactionalStorageResult<Vec<u8>> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(SNAPSHOT_MAGIC);
    encode_usize(&mut encoded, violations.len())?;
    for violation in violations {
        encode_text(&mut encoded, &violation.violation_id)?;
        encode_text(&mut encoded, &violation.rule_id)?;
        encode_text(&mut encoded, &violation.summary)?;
        encode_text(&mut encoded, &violation.path)?;
        encode_usize(&mut encoded, violation.line)?;
        match violation.quick_fix.as_deref() {
            Some(value) => {
                encoded.push(1);
                encode_text(&mut encoded, value)?;
            }
            None => encoded.push(0),
        }
        encode_text(&mut encoded, &violation.public_rule_reference)?;
    }
    if encoded.len() > MAX_SNAPSHOT_BYTES {
        return Err(invalid("canonical governance snapshot is too large"));
    }
    Ok(encoded)
}

fn decode_snapshot(
    payload: &[u8],
) -> TransactionalStorageResult<Vec<ProjectedGovernanceViolation>> {
    if payload.len() > MAX_SNAPSHOT_BYTES || !payload.starts_with(SNAPSHOT_MAGIC) {
        return Err(invalid(
            "canonical governance snapshot uses an incompatible contract",
        ));
    }
    let mut cursor = SnapshotCursor {
        payload,
        offset: SNAPSHOT_MAGIC.len(),
    };
    let count = cursor.read_usize()?;
    if count > 4_096 {
        return Err(invalid(
            "canonical governance snapshot exceeds the violation limit",
        ));
    }
    let mut violations = Vec::with_capacity(count);
    for _ in 0..count {
        let violation_id = cursor.read_text()?;
        let rule_id = cursor.read_text()?;
        let summary = cursor.read_text()?;
        let path = cursor.read_text()?;
        let line = cursor.read_usize()?;
        let quick_fix = match cursor.read_byte()? {
            0 => None,
            1 => Some(cursor.read_text()?),
            _ => {
                return Err(invalid(
                    "canonical governance snapshot has an invalid option tag",
                ))
            }
        };
        let public_rule_reference = cursor.read_text()?;
        let violation = ProjectedGovernanceViolation {
            violation_id,
            rule_id,
            summary,
            path,
            line,
            quick_fix,
            public_rule_reference,
        };
        validate_violation(&violation)?;
        if !is_agent_correctable_violation_path(&violation.path) {
            return Err(invalid(
                "canonical governance snapshot contains an administrative path",
            ));
        }
        violations.push(violation);
    }
    if cursor.offset != payload.len() {
        return Err(invalid(
            "canonical governance snapshot contains trailing data",
        ));
    }
    Ok(violations)
}

fn encode_usize(output: &mut Vec<u8>, value: usize) -> TransactionalStorageResult<()> {
    let value = u32::try_from(value)
        .map_err(|_| invalid("canonical governance snapshot length is unsupported"))?;
    output.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn encode_text(output: &mut Vec<u8>, value: &str) -> TransactionalStorageResult<()> {
    encode_usize(output, value.len())?;
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

struct SnapshotCursor<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl SnapshotCursor<'_> {
    fn read_byte(&mut self) -> TransactionalStorageResult<u8> {
        let byte = self
            .payload
            .get(self.offset)
            .copied()
            .ok_or_else(|| invalid("canonical governance snapshot is truncated"))?;
        self.offset += 1;
        Ok(byte)
    }

    fn read_usize(&mut self) -> TransactionalStorageResult<usize> {
        let end = self
            .offset
            .checked_add(4)
            .ok_or_else(|| invalid("canonical governance snapshot is truncated"))?;
        let bytes: [u8; 4] = self
            .payload
            .get(self.offset..end)
            .ok_or_else(|| invalid("canonical governance snapshot is truncated"))?
            .try_into()
            .map_err(|_| invalid("canonical governance snapshot is truncated"))?;
        self.offset = end;
        usize::try_from(u32::from_le_bytes(bytes))
            .map_err(|_| invalid("canonical governance snapshot length is unsupported"))
    }

    fn read_text(&mut self) -> TransactionalStorageResult<String> {
        let length = self.read_usize()?;
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| invalid("canonical governance snapshot is truncated"))?;
        let bytes = self
            .payload
            .get(self.offset..end)
            .ok_or_else(|| invalid("canonical governance snapshot is truncated"))?;
        self.offset = end;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| invalid("canonical governance snapshot text must be valid UTF-8"))
    }
}

fn lowercase_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn snapshot_fingerprint(
    project_id: &str,
    workspace_id: &str,
    source_revision: i64,
    payload: &[u8],
) -> TransactionalStorageResult<String> {
    let mut scoped = Vec::with_capacity(
        CANONICAL_GOVERNANCE_SNAPSHOT_VERSION.len()
            + project_id.len()
            + workspace_id.len()
            + payload.len()
            + 32,
    );
    encode_text(&mut scoped, CANONICAL_GOVERNANCE_SNAPSHOT_VERSION)?;
    encode_text(&mut scoped, project_id)?;
    encode_text(&mut scoped, workspace_id)?;
    scoped.extend_from_slice(&source_revision.to_le_bytes());
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| invalid("canonical governance snapshot is too large"))?;
    scoped.extend_from_slice(&payload_len.to_le_bytes());
    scoped.extend_from_slice(payload);
    Ok(lowercase_hex(&sha256(&scoped)))
}

fn invalid(message: &str) -> TransactionalStorageError {
    TransactionalStorageError::InvalidInput(message.to_string())
}
