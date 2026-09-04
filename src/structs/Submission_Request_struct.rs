use super::{
    BackupCheckpointRecord, NewAuditEvent, SubmissionStore, SubmissionWriteOutcome,
    TransactionalStorageResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentTaskSubmissionBinding {
    pub project_id: String,
    pub workspace_id: String,
    pub task_id: String,
    pub task_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmissionEvidence {
    Note { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionCreateRequest {
    pub contract_version: String,
    pub request_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub task_id: String,
    pub task_revision: i64,
    pub base_revision: Option<String>,
    pub evidence: Vec<SubmissionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedSubmissionMetadata {
    pub submission_id: String,
    pub fingerprint: String,
    pub received_at: String,
}

pub fn parse_current_task_submission_binding(
    projection: &[u8],
) -> TransactionalStorageResult<CurrentTaskSubmissionBinding> {
    super::Submission_Request_structcore::parse_current_task_submission_binding(projection)
}

pub fn build_note_submission_request(
    binding: &CurrentTaskSubmissionBinding,
    request_id: &str,
    evidence: &str,
) -> TransactionalStorageResult<SubmissionCreateRequest> {
    super::Submission_Request_structcore::build_note_submission_request(
        binding, request_id, evidence,
    )
}

pub fn serialize_submission_request(
    request: &SubmissionCreateRequest,
) -> TransactionalStorageResult<Vec<u8>> {
    super::Submission_Request_structcore::serialize_submission_request(request)
}

pub fn record_authorized_submission<S: SubmissionStore>(
    store: &S,
    request: &SubmissionCreateRequest,
    metadata: &AuthorizedSubmissionMetadata,
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<SubmissionWriteOutcome> {
    super::Submission_Request_structcore::record_authorized_submission(
        store,
        request,
        metadata,
        audit_event,
    )
}

pub fn record_authorized_submission_with_checkpoint<S: SubmissionStore>(
    store: &S,
    request: &SubmissionCreateRequest,
    metadata: &AuthorizedSubmissionMetadata,
    audit_event: &NewAuditEvent,
    checkpoint: &BackupCheckpointRecord,
) -> TransactionalStorageResult<SubmissionWriteOutcome> {
    super::Submission_Request_structcore::record_authorized_submission_with_checkpoint(
        store,
        request,
        metadata,
        audit_event,
        checkpoint,
    )
}
