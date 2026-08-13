use std::collections::BTreeSet;

use super::{
    reject_public_path_references, validate_project_relative_path, CurrentTaskProjection,
    CurrentTaskProjectionDocument, ProjectedCurrentTask, TransactionalStorageError,
    TransactionalStorageResult, AGENT_PROJECTION_CONTRACT_VERSION,
};

pub(super) fn build_current_task_projection(
    document: &CurrentTaskProjectionDocument,
) -> TransactionalStorageResult<CurrentTaskProjection> {
    validate_document(document)?;
    Ok(CurrentTaskProjection {
        contract_version: AGENT_PROJECTION_CONTRACT_VERSION.to_string(),
        metadata: document.metadata.clone(),
        serialized_contract: serialize_document(document).into_bytes(),
    })
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
    if task.acceptance_criteria.len() > 128 || task.relevant_paths.len() > 256 {
        return Err(invalid(
            "projection collection exceeds the v1 contract limit",
        ));
    }
    for criterion in &task.acceptance_criteria {
        validate_text(criterion, 1, 4_096, "acceptance criterion")?;
        reject_public_path_references(criterion, "acceptance criterion")?;
    }
    let mut unique_paths = BTreeSet::new();
    for path in &task.relevant_paths {
        validate_project_relative_path(path, "relevant path")?;
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
