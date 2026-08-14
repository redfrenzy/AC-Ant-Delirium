use super::TransactionalStorageResult;

pub const PUBLIC_PATH_REDACTION: &str = "[project-root]";

pub fn sanitize_public_text(value: &str) -> String {
    super::Public_Path_Policy_structcore::sanitize_public_text(value)
}

pub fn reject_public_path_references(value: &str, field: &str) -> TransactionalStorageResult<()> {
    super::Public_Path_Policy_structcore::reject_public_path_references(value, field)
}

pub fn validate_project_relative_path(value: &str, field: &str) -> TransactionalStorageResult<()> {
    super::Public_Path_Policy_structcore::validate_project_relative_path(value, field)
}

pub fn validate_agent_visible_project_path(
    value: &str,
    field: &str,
) -> TransactionalStorageResult<()> {
    super::Public_Path_Policy_structcore::validate_agent_visible_project_path(value, field)
}

pub fn reject_protected_administrative_references(
    value: &str,
    field: &str,
) -> TransactionalStorageResult<()> {
    super::Public_Path_Policy_structcore::reject_protected_administrative_references(value, field)
}

pub fn validate_serialized_public_document(
    value: &[u8],
    field: &str,
) -> TransactionalStorageResult<()> {
    super::Public_Path_Policy_structcore::validate_serialized_public_document(value, field)
}

pub fn validate_serialized_agent_projection_document(
    value: &[u8],
    field: &str,
) -> TransactionalStorageResult<()> {
    super::Public_Path_Policy_structcore::validate_serialized_agent_projection_document(
        value, field,
    )
}
