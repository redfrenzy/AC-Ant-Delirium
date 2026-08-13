use super::{
    reject_public_path_references, validate_serialized_public_document,
    AuthorizedSubmissionMetadata, CurrentTaskSubmissionBinding, NewAuditEvent, NewSubmission,
    SubmissionCreateRequest, SubmissionEvidence, SubmissionStore, SubmissionWriteOutcome,
    TransactionalStorageError, TransactionalStorageResult, SUBMISSION_CONTRACT_VERSION,
};

pub(super) fn parse_current_task_submission_binding(
    projection: &[u8],
) -> TransactionalStorageResult<CurrentTaskSubmissionBinding> {
    validate_serialized_public_document(projection, "current task projection")?;
    let document = std::str::from_utf8(projection)
        .map_err(|_| invalid("current task projection must be valid UTF-8"))?;
    if extract_first_json_string(document, "contract_version")? != "agent-projection-v1"
        || extract_json_string(document, "artifact_kind")? != "current_task"
    {
        return Err(invalid(
            "current task projection uses an incompatible contract",
        ));
    }

    let binding = CurrentTaskSubmissionBinding {
        project_id: extract_json_string(document, "project_ref")?,
        workspace_id: extract_json_string(document, "workspace_ref")?,
        task_id: extract_json_string(document, "task_id")?,
        task_revision: extract_json_integer(document, "task_revision")?,
    };
    validate_binding(&binding)?;
    Ok(binding)
}

pub(super) fn build_note_submission_request(
    binding: &CurrentTaskSubmissionBinding,
    request_id: &str,
    evidence: &str,
) -> TransactionalStorageResult<SubmissionCreateRequest> {
    validate_binding(binding)?;
    validate_opaque_id(request_id, "req_", "request_id")?;
    validate_text(evidence, 1, 8_192, "submission evidence")?;
    reject_public_path_references(evidence, "submission evidence")?;
    let request = SubmissionCreateRequest {
        contract_version: SUBMISSION_CONTRACT_VERSION.to_string(),
        request_id: request_id.to_string(),
        project_id: binding.project_id.clone(),
        workspace_id: binding.workspace_id.clone(),
        task_id: binding.task_id.clone(),
        task_revision: binding.task_revision,
        base_revision: None,
        evidence: vec![SubmissionEvidence::Note {
            text: evidence.to_string(),
        }],
    };
    validate_request(&request)?;
    Ok(request)
}

pub(super) fn serialize_submission_request(
    request: &SubmissionCreateRequest,
) -> TransactionalStorageResult<Vec<u8>> {
    validate_request(request)?;
    let base_revision = request
        .base_revision
        .as_ref()
        .map(|revision| format!(",\n  \"base_revision\": \"{}\"", json_escape(revision)))
        .unwrap_or_default();
    let evidence = request
        .evidence
        .iter()
        .map(|item| match item {
            SubmissionEvidence::Note { text } => {
                format!(
                    "{{\"kind\": \"note\", \"text\": \"{}\"}}",
                    json_escape(text)
                )
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "{{\n  \"contract_version\": \"submission-v1\",\n  \"request_id\": \"{}\",\n  \"project_id\": \"{}\",\n  \"workspace_id\": \"{}\",\n  \"task_id\": \"{}\",\n  \"task_revision\": {}{},\n  \"evidence\": [{}]\n}}\n",
        json_escape(&request.request_id),
        json_escape(&request.project_id),
        json_escape(&request.workspace_id),
        json_escape(&request.task_id),
        request.task_revision,
        base_revision,
        evidence,
    )
    .into_bytes())
}

pub(super) fn record_authorized_submission<S: SubmissionStore>(
    store: &S,
    request: &SubmissionCreateRequest,
    metadata: &AuthorizedSubmissionMetadata,
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<SubmissionWriteOutcome> {
    let payload = serialize_submission_request(request)?;
    validate_opaque_id(&metadata.submission_id, "sub_", "submission_id")?;
    validate_fingerprint(&metadata.fingerprint)?;
    validate_text(&metadata.received_at, 1, 255, "received_at")?;
    let submission = NewSubmission {
        contract_version: request.contract_version.clone(),
        submission_id: metadata.submission_id.clone(),
        request_id: request.request_id.clone(),
        project_id: request.project_id.clone(),
        workspace_id: request.workspace_id.clone(),
        task_id: request.task_id.clone(),
        task_revision: request.task_revision,
        base_revision: request.base_revision.clone(),
        fingerprint: metadata.fingerprint.clone(),
        payload,
        received_at: metadata.received_at.clone(),
    };
    store.record_submission(&submission, audit_event)
}

fn validate_request(request: &SubmissionCreateRequest) -> TransactionalStorageResult<()> {
    if request.contract_version != SUBMISSION_CONTRACT_VERSION {
        return Err(invalid("submission request uses an incompatible contract"));
    }
    validate_opaque_id(&request.request_id, "req_", "request_id")?;
    validate_binding(&CurrentTaskSubmissionBinding {
        project_id: request.project_id.clone(),
        workspace_id: request.workspace_id.clone(),
        task_id: request.task_id.clone(),
        task_revision: request.task_revision,
    })?;
    if let Some(base_revision) = request.base_revision.as_deref() {
        validate_text(base_revision, 1, 255, "base_revision")?;
        reject_public_path_references(base_revision, "base_revision")?;
    }
    if request.evidence.is_empty() || request.evidence.len() > 128 {
        return Err(invalid("submission evidence count violates submission-v1"));
    }
    for evidence in &request.evidence {
        match evidence {
            SubmissionEvidence::Note { text } => {
                validate_text(text, 1, 8_192, "submission evidence")?;
                reject_public_path_references(text, "submission evidence")?;
            }
        }
    }
    Ok(())
}

fn validate_binding(binding: &CurrentTaskSubmissionBinding) -> TransactionalStorageResult<()> {
    validate_opaque_id(&binding.project_id, "prj_", "project_id")?;
    validate_opaque_id(&binding.workspace_id, "wsp_", "workspace_id")?;
    validate_opaque_id(&binding.task_id, "tsk_", "task_id")?;
    if binding.task_revision < 1 {
        return Err(invalid("task_revision must be at least one"));
    }
    Ok(())
}

fn validate_opaque_id(value: &str, prefix: &str, field: &str) -> TransactionalStorageResult<()> {
    let token = value
        .strip_prefix(prefix)
        .ok_or_else(|| invalid(&format!("{field} uses an invalid identifier")))?;
    if token.len() != 26 || !token.bytes().all(is_crockford_byte) {
        return Err(invalid(&format!("{field} uses an invalid identifier")));
    }
    Ok(())
}

fn is_crockford_byte(byte: u8) -> bool {
    byte.is_ascii_digit()
        || matches!(byte, b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z')
}

fn validate_text(
    value: &str,
    minimum: usize,
    maximum: usize,
    field: &str,
) -> TransactionalStorageResult<()> {
    let length = value.chars().count();
    if length < minimum || length > maximum || value.chars().any(char::is_control) {
        return Err(invalid(&format!("{field} violates its text contract")));
    }
    Ok(())
}

fn validate_fingerprint(value: &str) -> TransactionalStorageResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("submission fingerprint must be lowercase SHA-256"));
    }
    Ok(())
}

fn extract_json_string(document: &str, key: &str) -> TransactionalStorageResult<String> {
    let start = unique_json_value_start(document, key)?;
    extract_json_string_at(document, key, start)
}

fn extract_first_json_string(document: &str, key: &str) -> TransactionalStorageResult<String> {
    let start = first_json_value_start(document, key)?;
    extract_json_string_at(document, key, start)
}

fn extract_json_string_at(
    document: &str,
    key: &str,
    start: usize,
) -> TransactionalStorageResult<String> {
    let bytes = document.as_bytes();
    if bytes.get(start) != Some(&b'\"') {
        return Err(invalid(&format!("{key} must be a JSON string")));
    }
    let end = json_string_end(bytes, start)?;
    let value = &document[start + 1..end - 1];
    if value.contains('\\') {
        return Err(invalid(&format!("{key} must use canonical JSON text")));
    }
    Ok(value.to_string())
}

fn first_json_value_start(document: &str, key: &str) -> TransactionalStorageResult<usize> {
    let bytes = document.as_bytes();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor] != b'\"' {
            cursor += 1;
            continue;
        }
        let end = json_string_end(bytes, cursor)?;
        let candidate = &document[cursor + 1..end - 1];
        let mut value_start = skip_whitespace(bytes, end);
        if candidate == key && bytes.get(value_start) == Some(&b':') {
            value_start = skip_whitespace(bytes, value_start + 1);
            return Ok(value_start);
        }
        cursor = end;
    }
    Err(invalid(&format!("{key} is missing")))
}

fn extract_json_integer(document: &str, key: &str) -> TransactionalStorageResult<i64> {
    let start = unique_json_value_start(document, key)?;
    let bytes = document.as_bytes();
    let end = bytes[start..]
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .map(|offset| start + offset)
        .unwrap_or(bytes.len());
    if start == end {
        return Err(invalid(&format!("{key} must be a positive JSON integer")));
    }
    document[start..end]
        .parse::<i64>()
        .map_err(|_| invalid(&format!("{key} must be a positive JSON integer")))
}

fn unique_json_value_start(document: &str, key: &str) -> TransactionalStorageResult<usize> {
    let bytes = document.as_bytes();
    let mut cursor = 0usize;
    let mut found = None;
    while cursor < bytes.len() {
        if bytes[cursor] != b'\"' {
            cursor += 1;
            continue;
        }
        let end = json_string_end(bytes, cursor)?;
        let candidate = &document[cursor + 1..end - 1];
        let mut value_start = skip_whitespace(bytes, end);
        if candidate == key && bytes.get(value_start) == Some(&b':') {
            value_start = skip_whitespace(bytes, value_start + 1);
            if found.replace(value_start).is_some() {
                return Err(invalid(&format!("{key} must appear exactly once")));
            }
        }
        cursor = end;
    }
    found.ok_or_else(|| invalid(&format!("{key} is missing")))
}

fn json_string_end(bytes: &[u8], start: usize) -> TransactionalStorageResult<usize> {
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\"' => return Ok(cursor + 1),
            b'\\' => {
                cursor += 2;
            }
            _ => cursor += 1,
        }
    }
    Err(invalid("projection contains an unterminated JSON string"))
}

fn skip_whitespace(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        cursor += 1;
    }
    cursor
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn invalid(message: &str) -> TransactionalStorageError {
    TransactionalStorageError::InvalidInput(message.to_string())
}
