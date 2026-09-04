use super::{
    BackupCheckpointRecord, NewAuditEvent, ReviewDecision, ReviewStore, ReviewWriteOutcome,
    TransactionalStorageResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignedReviewDecisionRequest {
    pub request_id: String,
    pub submission_id: String,
    pub decision: ReviewDecision,
    pub justification: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedReviewAssignment {
    pub project_id: String,
    pub reviewer_id: String,
    pub assigned_submission_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedReviewMetadata {
    pub review_id: String,
    pub fingerprint: String,
    pub created_at: String,
}

pub fn build_assigned_review_decision(
    request_id: &str,
    submission_id: &str,
    decision: ReviewDecision,
    justification: &str,
) -> TransactionalStorageResult<AssignedReviewDecisionRequest> {
    super::Review_Request_structcore::build_assigned_review_decision(
        request_id,
        submission_id,
        decision,
        justification,
    )
}

pub fn serialize_assigned_review_decision(
    request: &AssignedReviewDecisionRequest,
) -> TransactionalStorageResult<Vec<u8>> {
    super::Review_Request_structcore::serialize_assigned_review_decision(request)
}

pub fn record_authorized_review<S: ReviewStore>(
    store: &S,
    request: &AssignedReviewDecisionRequest,
    assignment: &AuthorizedReviewAssignment,
    metadata: &AuthorizedReviewMetadata,
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<ReviewWriteOutcome> {
    super::Review_Request_structcore::record_authorized_review(
        store,
        request,
        assignment,
        metadata,
        audit_event,
    )
}

pub fn record_authorized_review_with_checkpoint<S: ReviewStore>(
    store: &S,
    request: &AssignedReviewDecisionRequest,
    assignment: &AuthorizedReviewAssignment,
    metadata: &AuthorizedReviewMetadata,
    audit_event: &NewAuditEvent,
    checkpoint: &BackupCheckpointRecord,
) -> TransactionalStorageResult<ReviewWriteOutcome> {
    super::Review_Request_structcore::record_authorized_review_with_checkpoint(
        store,
        request,
        assignment,
        metadata,
        audit_event,
        checkpoint,
    )
}
