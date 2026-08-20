use std::collections::BTreeSet;

use crate::libres::Rusqlite_unit::AC_params;
use crate::libres::Sha2_unit::sha256;
use crate::libres::System_Random_unit::random_crockford_id;

use super::{
    reject_protected_administrative_references, reject_public_path_references,
    validate_agent_visible_project_path, AtomicProjectProjectionWriter,
    CanonicalTaskProjectionPayload, CurrentTaskProjection, CurrentTaskProjectionDocument,
    CurrentTaskProjectionRefreshRequest, NewAuditEvent, ProjectDatabase, ProjectProjectionWriter,
    ProjectedCurrentTask, ProjectedQueueStatus, ProjectedTaskAction, ProjectionMetadata,
    ProjectionWriteReceipt, TransactionalStorageError, TransactionalStorageResult,
    AGENT_PROJECTION_CONTRACT_VERSION, AUDIT_EVENT_CONTRACT_VERSION,
    CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION,
};

const CANONICAL_TASK_PAYLOAD_MAGIC: &[u8] = b"canonical-task-projection-v1\0";
const MAX_CANONICAL_TASK_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn build_current_task_projection(
    document: &CurrentTaskProjectionDocument,
) -> TransactionalStorageResult<CurrentTaskProjection> {
    validate_document(document)?;
    Ok(CurrentTaskProjection {
        contract_version: AGENT_PROJECTION_CONTRACT_VERSION.to_string(),
        metadata: document.metadata.clone(),
        task_ref: document
            .task
            .as_ref()
            .map(|task| super::CanonicalTaskProjectionRef {
                task_id: task.task_id.clone(),
                task_revision: task.task_revision,
            }),
        serialized_contract: serialize_document(document).into_bytes(),
    })
}

pub(super) fn encode_canonical_task_projection_payload(
    payload: &CanonicalTaskProjectionPayload,
) -> TransactionalStorageResult<Vec<u8>> {
    validate_canonical_task_payload(payload)?;
    let mut encoded = Vec::new();
    encoded.extend_from_slice(CANONICAL_TASK_PAYLOAD_MAGIC);
    encoded.push(action_tag(payload.allowed_action));
    encode_text(&mut encoded, &payload.objective)?;
    encode_text_collection(&mut encoded, &payload.acceptance_criteria)?;
    encode_text_collection(&mut encoded, &payload.relevant_paths)?;
    if encoded.len() > MAX_CANONICAL_TASK_PAYLOAD_BYTES {
        return Err(invalid("canonical task projection payload is too large"));
    }
    Ok(encoded)
}

pub(super) fn refresh_current_task_projection(
    writer: &AtomicProjectProjectionWriter,
    request: &CurrentTaskProjectionRefreshRequest,
) -> TransactionalStorageResult<ProjectionWriteReceipt> {
    validate_text(&request.project_id, 1, 255, "project_id")?;
    validate_text(&request.workspace_id, 1, 255, "workspace_id")?;
    validate_text(&request.rendered_at, 1, 255, "rendered_at")?;
    let connection = writer.database.open_connection(false)?;
    let (source_revision, current_task_id, previous_generation) = connection.query_row(
        "SELECT p.state_revision, p.current_task_id, p.projection_generation FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
        AC_params![request.project_id, request.workspace_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    )?;
    let generation = previous_generation.checked_add(1).ok_or_else(|| {
        TransactionalStorageError::Integrity("projection generation overflow".to_string())
    })?;
    let (task, queue_status) = match current_task_id.as_deref() {
        Some(task_id) => {
            let (task_revision, status, review_status, payload) = connection.query_row(
                "SELECT t.task_revision, t.status, t.review_status, t.payload FROM tasks t JOIN projects p ON p.project_id = t.project_id AND p.active_plan_id = t.plan_id WHERE t.project_id = ?1 AND t.task_id = ?2",
                AC_params![request.project_id, task_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )?;
            let payload = decode_canonical_task_projection_payload(&payload)?;
            validate_action_lifecycle(payload.allowed_action, &status, &review_status)?;
            let queue_status = queue_status_for_action(payload.allowed_action);
            (
                Some(ProjectedCurrentTask {
                    task_id: task_id.to_string(),
                    task_revision,
                    objective: payload.objective,
                    allowed_action: payload.allowed_action,
                    acceptance_criteria: payload.acceptance_criteria,
                    relevant_paths: payload.relevant_paths,
                }),
                queue_status,
            )
        }
        None => (None, ProjectedQueueStatus::NoTaskAvailable),
    };
    drop(connection);

    let mut metadata = ProjectionMetadata {
        project_id: request.project_id.clone(),
        workspace_id: request.workspace_id.clone(),
        source_revision,
        generation,
        fingerprint: "0".repeat(64),
        rendered_at: request.rendered_at.clone(),
    };
    let mut projection = build_current_task_projection(&CurrentTaskProjectionDocument {
        metadata: metadata.clone(),
        task: task.clone(),
        queue_status,
        submission_instruction: "Submit evidence with hierarchy.task.current.submit. Submission does not complete or approve the task."
            .to_string(),
    })?;
    metadata.fingerprint = lowercase_hex(&sha256(&projection.serialized_contract));
    projection = build_current_task_projection(&CurrentTaskProjectionDocument {
        metadata: metadata.clone(),
        task,
        queue_status,
        submission_instruction: "Submit evidence with hierarchy.task.current.submit. Submission does not complete or approve the task."
            .to_string(),
    })?;
    let event_id = random_crockford_id("evt_")?;
    let correlation_id = random_crockford_id("ref_")?;
    let audit_event = NewAuditEvent {
        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
        event_id,
        project_id: request.project_id.clone(),
        workspace_id: Some(request.workspace_id.clone()),
        actor_kind: "service".to_string(),
        actor_subject_id: "antdelirium-service".to_string(),
        event_kind: "projection_rendered".to_string(),
        outcome: "succeeded".to_string(),
        occurred_at: request.rendered_at.clone(),
        fingerprint: metadata.fingerprint,
        correlation_id,
        resource_id: Some("current_task".to_string()),
        payload: b"current task projection rendered".to_vec(),
    };
    writer.write_current_task(&projection, &audit_event)
}

pub(super) fn render_current_task_projection(
    database: &ProjectDatabase,
    project_id: &str,
    workspace_id: &str,
    rendered_at: &str,
) -> TransactionalStorageResult<Vec<u8>> {
    validate_text(project_id, 1, 255, "project_id")?;
    validate_text(workspace_id, 1, 255, "workspace_id")?;
    validate_text(rendered_at, 1, 255, "rendered_at")?;
    let connection = database.open_connection(false)?;
    let (source_revision, current_task_id, previous_generation) = connection.query_row(
        "SELECT p.state_revision, p.current_task_id, p.projection_generation FROM projects p JOIN workspaces w ON w.project_id = p.project_id WHERE p.project_id = ?1 AND w.workspace_id = ?2 AND w.status = 'active'",
        AC_params![project_id, workspace_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    )?;
    let generation = previous_generation.checked_add(1).ok_or_else(|| {
        TransactionalStorageError::Integrity("projection generation overflow".to_string())
    })?;
    let (task, queue_status) = match current_task_id.as_deref() {
        Some(task_id) => {
            let (task_revision, status, review_status, payload) = connection.query_row(
                "SELECT t.task_revision, t.status, t.review_status, t.payload FROM tasks t JOIN projects p ON p.project_id = t.project_id AND p.active_plan_id = t.plan_id WHERE t.project_id = ?1 AND t.task_id = ?2",
                AC_params![project_id, task_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )?;
            let payload = decode_canonical_task_projection_payload(&payload)?;
            validate_action_lifecycle(payload.allowed_action, &status, &review_status)?;
            (
                Some(ProjectedCurrentTask {
                    task_id: task_id.to_string(),
                    task_revision,
                    objective: payload.objective,
                    allowed_action: payload.allowed_action,
                    acceptance_criteria: payload.acceptance_criteria,
                    relevant_paths: payload.relevant_paths,
                }),
                queue_status_for_action(payload.allowed_action),
            )
        }
        None => (None, ProjectedQueueStatus::NoTaskAvailable),
    };
    let mut metadata = ProjectionMetadata {
        project_id: project_id.to_string(),
        workspace_id: workspace_id.to_string(),
        source_revision,
        generation,
        fingerprint: "0".repeat(64),
        rendered_at: rendered_at.to_string(),
    };
    let instruction = "Submit evidence with hierarchy.task.current.submit. Submission does not complete or approve the task.".to_string();
    let mut projection = build_current_task_projection(&CurrentTaskProjectionDocument {
        metadata: metadata.clone(),
        task: task.clone(),
        queue_status,
        submission_instruction: instruction.clone(),
    })?;
    metadata.fingerprint = lowercase_hex(&sha256(&projection.serialized_contract));
    projection = build_current_task_projection(&CurrentTaskProjectionDocument {
        metadata,
        task,
        queue_status,
        submission_instruction: instruction,
    })?;
    Ok(projection.serialized_contract)
}

fn validate_document(document: &CurrentTaskProjectionDocument) -> TransactionalStorageResult<()> {
    validate_projection_metadata(&document.metadata)?;
    validate_text(
        &document.submission_instruction,
        1,
        4_096,
        "submission instruction",
    )?;
    reject_public_path_references(&document.submission_instruction, "submission instruction")?;
    reject_protected_administrative_references(
        &document.submission_instruction,
        "submission instruction",
    )?;
    match document.task.as_ref() {
        Some(task) => validate_task(task)?,
        None if document.queue_status.contract_value() == "task_available" => {
            return Err(invalid("task_available projection must contain a task"));
        }
        None => {}
    }
    Ok(())
}

pub(super) fn validate_projection_metadata(
    metadata: &super::ProjectionMetadata,
) -> TransactionalStorageResult<()> {
    validate_opaque_id(&metadata.project_id, "prj_", "project_ref")?;
    validate_opaque_id(&metadata.workspace_id, "wsp_", "workspace_ref")?;
    if metadata.source_revision < 0 || metadata.generation < 0 {
        return Err(invalid("projection revisions must not be negative"));
    }
    validate_fingerprint(&metadata.fingerprint)?;
    validate_text(&metadata.rendered_at, 1, 255, "rendered_at")?;
    Ok(())
}

fn validate_task(task: &ProjectedCurrentTask) -> TransactionalStorageResult<()> {
    validate_opaque_id(&task.task_id, "tsk_", "task_id")?;
    if task.task_revision < 1 {
        return Err(invalid("task_revision must be at least one"));
    }
    validate_text(&task.objective, 1, 16_384, "task objective")?;
    reject_public_path_references(&task.objective, "task objective")?;
    reject_protected_administrative_references(&task.objective, "task objective")?;
    if task.acceptance_criteria.len() > 128 || task.relevant_paths.len() > 256 {
        return Err(invalid(
            "projection collection exceeds the v1 contract limit",
        ));
    }
    for criterion in &task.acceptance_criteria {
        validate_text(criterion, 1, 4_096, "acceptance criterion")?;
        reject_public_path_references(criterion, "acceptance criterion")?;
        reject_protected_administrative_references(criterion, "acceptance criterion")?;
    }
    let mut unique_paths = BTreeSet::new();
    for path in &task.relevant_paths {
        validate_agent_visible_project_path(path, "relevant path")?;
        if !unique_paths.insert(path) {
            return Err(invalid("relevant_paths must be unique"));
        }
    }
    Ok(())
}

fn validate_opaque_id(value: &str, prefix: &str, field: &str) -> TransactionalStorageResult<()> {
    let token = value.strip_prefix(prefix).ok_or_else(|| {
        invalid(&format!(
            "{field} must use the canonical {prefix} identifier"
        ))
    })?;
    if token.len() != 26 || !token.bytes().all(|byte| {
        byte.is_ascii_digit()
            || matches!(byte, b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z')
    }) {
        return Err(invalid(&format!(
            "{field} is not a valid opaque identifier"
        )));
    }
    Ok(())
}

fn validate_fingerprint(value: &str) -> TransactionalStorageResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "projection fingerprint must be 64 lowercase hexadecimal characters",
        ));
    }
    Ok(())
}

pub(super) fn validate_text(
    value: &str,
    minimum: usize,
    maximum: usize,
    field: &str,
) -> TransactionalStorageResult<()> {
    let length = value.chars().count();
    if length < minimum || length > maximum || value.contains('\0') {
        return Err(invalid(&format!("{field} violates the v1 text contract")));
    }
    Ok(())
}

fn serialize_document(document: &CurrentTaskProjectionDocument) -> String {
    let task = document
        .task
        .as_ref()
        .map(serialize_task)
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\n  \"contract_version\": \"agent-projection-v1\",\n  \"artifact_kind\": \"current_task\",\n  \"project_ref\": \"{}\",\n  \"workspace_ref\": \"{}\",\n  \"source_revision\": {},\n  \"task\": {},\n  \"queue_status\": \"{}\",\n  \"submission\": {{\n    \"contract_version\": \"submission-v1\",\n    \"instruction\": \"{}\"\n  }}\n}}\n",
        json_escape(&document.metadata.project_id),
        json_escape(&document.metadata.workspace_id),
        document.metadata.source_revision,
        task,
        document.queue_status.contract_value(),
        json_escape(&document.submission_instruction),
    )
}

fn serialize_task(task: &ProjectedCurrentTask) -> String {
    format!(
        "{{\n    \"task_id\": \"{}\",\n    \"task_revision\": {},\n    \"objective\": \"{}\",\n    \"allowed_action\": \"{}\",\n    \"acceptance_criteria\": {},\n    \"relevant_paths\": {}\n  }}",
        json_escape(&task.task_id),
        task.task_revision,
        json_escape(&task.objective),
        task.allowed_action.contract_value(),
        serialize_string_array(&task.acceptance_criteria),
        serialize_string_array(&task.relevant_paths),
    )
}

fn serialize_string_array(values: &[String]) -> String {
    let values = values
        .iter()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{values}]")
}

fn validate_canonical_task_payload(
    payload: &CanonicalTaskProjectionPayload,
) -> TransactionalStorageResult<()> {
    if payload.contract_version != CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION {
        return Err(invalid(
            "canonical task projection payload uses an incompatible contract",
        ));
    }
    validate_task(&ProjectedCurrentTask {
        task_id: "tsk_00000000000000000000000000".to_string(),
        task_revision: 1,
        objective: payload.objective.clone(),
        allowed_action: payload.allowed_action,
        acceptance_criteria: payload.acceptance_criteria.clone(),
        relevant_paths: payload.relevant_paths.clone(),
    })
}

fn action_tag(action: ProjectedTaskAction) -> u8 {
    match action {
        ProjectedTaskAction::Implement => 1,
        ProjectedTaskAction::Review => 2,
        ProjectedTaskAction::PlanReview => 3,
        ProjectedTaskAction::Wait => 4,
    }
}

fn action_from_tag(tag: u8) -> TransactionalStorageResult<ProjectedTaskAction> {
    match tag {
        1 => Ok(ProjectedTaskAction::Implement),
        2 => Ok(ProjectedTaskAction::Review),
        3 => Ok(ProjectedTaskAction::PlanReview),
        4 => Ok(ProjectedTaskAction::Wait),
        _ => Err(invalid(
            "canonical task projection payload contains an invalid action",
        )),
    }
}

fn encode_text(output: &mut Vec<u8>, value: &str) -> TransactionalStorageResult<()> {
    let length = u32::try_from(value.len())
        .map_err(|_| invalid("canonical task projection text is too large"))?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn encode_text_collection(
    output: &mut Vec<u8>,
    values: &[String],
) -> TransactionalStorageResult<()> {
    let count = u32::try_from(values.len())
        .map_err(|_| invalid("canonical task projection collection is too large"))?;
    output.extend_from_slice(&count.to_be_bytes());
    for value in values {
        encode_text(output, value)?;
    }
    Ok(())
}

fn decode_canonical_task_projection_payload(
    encoded: &[u8],
) -> TransactionalStorageResult<CanonicalTaskProjectionPayload> {
    if encoded.len() > MAX_CANONICAL_TASK_PAYLOAD_BYTES {
        return Err(invalid("canonical task projection payload is too large"));
    }
    let mut cursor = PayloadCursor::new(encoded);
    if cursor.take(CANONICAL_TASK_PAYLOAD_MAGIC.len())? != CANONICAL_TASK_PAYLOAD_MAGIC {
        return Err(invalid(
            "canonical task projection payload uses an incompatible contract",
        ));
    }
    let allowed_action = action_from_tag(cursor.byte()?)?;
    let objective = cursor.text()?;
    let acceptance_criteria = cursor.text_collection(128)?;
    let relevant_paths = cursor.text_collection(256)?;
    if !cursor.is_finished() {
        return Err(invalid(
            "canonical task projection payload contains trailing data",
        ));
    }
    let payload = CanonicalTaskProjectionPayload {
        contract_version: CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION.to_string(),
        objective,
        allowed_action,
        acceptance_criteria,
        relevant_paths,
    };
    validate_canonical_task_payload(&payload)?;
    Ok(payload)
}

struct PayloadCursor<'a> {
    encoded: &'a [u8],
    offset: usize,
}

impl<'a> PayloadCursor<'a> {
    fn new(encoded: &'a [u8]) -> Self {
        Self { encoded, offset: 0 }
    }

    fn take(&mut self, length: usize) -> TransactionalStorageResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .filter(|end| *end <= self.encoded.len())
            .ok_or_else(|| invalid("canonical task projection payload is truncated"))?;
        let value = &self.encoded[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> TransactionalStorageResult<u8> {
        Ok(self.take(1)?[0])
    }

    fn unsigned(&mut self) -> TransactionalStorageResult<usize> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| invalid("canonical task projection payload is truncated"))?;
        usize::try_from(u32::from_be_bytes(bytes))
            .map_err(|_| invalid("canonical task projection length is unsupported"))
    }

    fn text(&mut self) -> TransactionalStorageResult<String> {
        let length = self.unsigned()?;
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| invalid("canonical task projection text must be valid UTF-8"))
    }

    fn text_collection(&mut self, maximum: usize) -> TransactionalStorageResult<Vec<String>> {
        let count = self.unsigned()?;
        if count > maximum {
            return Err(invalid(
                "canonical task projection collection exceeds its limit",
            ));
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.text()?);
        }
        Ok(values)
    }

    fn is_finished(&self) -> bool {
        self.offset == self.encoded.len()
    }
}

fn validate_action_lifecycle(
    action: ProjectedTaskAction,
    status: &str,
    review_status: &str,
) -> TransactionalStorageResult<()> {
    let valid = match action {
        ProjectedTaskAction::Implement | ProjectedTaskAction::PlanReview => {
            status == "available" && matches!(review_status, "not_required" | "rejected")
        }
        ProjectedTaskAction::Review => status == "submitted" && review_status == "pending",
        ProjectedTaskAction::Wait => status == "blocked",
    };
    if !valid {
        return Err(TransactionalStorageError::Integrity(
            "canonical task action does not match its lifecycle state".to_string(),
        ));
    }
    Ok(())
}

fn queue_status_for_action(action: ProjectedTaskAction) -> ProjectedQueueStatus {
    match action {
        ProjectedTaskAction::Implement => ProjectedQueueStatus::TaskAvailable,
        ProjectedTaskAction::Review | ProjectedTaskAction::PlanReview => {
            ProjectedQueueStatus::AwaitingReview
        }
        ProjectedTaskAction::Wait => ProjectedQueueStatus::Blocked,
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

pub(super) fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn invalid(message: &str) -> TransactionalStorageError {
    TransactionalStorageError::InvalidInput(message.to_string())
}
