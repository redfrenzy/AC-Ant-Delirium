use super::{
    reject_public_path_references, AssignedReviewDecisionRequest, AuthorizedReviewAssignment,
    AuthorizedReviewMetadata, NewAuditEvent, NewReview, ReviewDecision, ReviewStore,
    ReviewWriteOutcome, TransactionalStorageError, TransactionalStorageResult,
};

pub(super) fn build_assigned_review_decision(
    request_id: &str,
    submission_id: &str,
    decision: ReviewDecision,
    justification: &str,
) -> TransactionalStorageResult<AssignedReviewDecisionRequest> {
    let request = AssignedReviewDecisionRequest {
        request_id: request_id.to_string(),
        submission_id: submission_id.to_string(),
        decision,
        justification: justification.to_string(),
    };
    validate_request(&request)?;
    Ok(request)
}

pub(super) fn serialize_assigned_review_decision(
    request: &AssignedReviewDecisionRequest,
) -> TransactionalStorageResult<Vec<u8>> {
    validate_request(request)?;
    Ok(format!(
        "{{\n  \"operation\": \"review.decide.assigned\",\n  \"request_id\": \"{}\",\n  \"submission_id\": \"{}\",\n  \"decision\": \"{}\",\n  \"justification\": \"{}\"\n}}\n",
        json_escape(&request.request_id),
        json_escape(&request.submission_id),
        request.decision.storage_value(),
        json_escape(&request.justification),
    )
    .into_bytes())
}

pub(super) fn record_authorized_review<S: ReviewStore>(
    store: &S,
    request: &AssignedReviewDecisionRequest,
    assignment: &AuthorizedReviewAssignment,
    metadata: &AuthorizedReviewMetadata,
    audit_event: &NewAuditEvent,
) -> TransactionalStorageResult<ReviewWriteOutcome> {
    validate_request(request)?;
    validate_assignment(assignment)?;
    if request.submission_id != assignment.assigned_submission_id {
        return Err(invalid("review request is outside the assigned submission"));
    }
    validate_opaque_id(&metadata.review_id, "rev_", "review_id")?;
    validate_fingerprint(&metadata.fingerprint)?;
    validate_text(&metadata.created_at, 1, 255, "created_at")?;
    let review = NewReview {
        review_id: metadata.review_id.clone(),
        project_id: assignment.project_id.clone(),
        submission_id: request.submission_id.clone(),
        reviewer_id: assignment.reviewer_id.clone(),
        decision: request.decision,
        fingerprint: metadata.fingerprint.clone(),
        payload: serialize_assigned_review_decision(request)?,
        created_at: metadata.created_at.clone(),
    };
    store.record_review(&review, audit_event)
}

fn validate_request(request: &AssignedReviewDecisionRequest) -> TransactionalStorageResult<()> {
    validate_opaque_id(&request.request_id, "req_", "request_id")?;
    validate_opaque_id(&request.submission_id, "sub_", "submission_id")?;
    validate_text(&request.justification, 1, 8_192, "review justification")?;
    reject_public_path_references(&request.justification, "review justification")
}

fn validate_assignment(assignment: &AuthorizedReviewAssignment) -> TransactionalStorageResult<()> {
    validate_opaque_id(&assignment.project_id, "prj_", "project_id")?;
    validate_opaque_id(
        &assignment.assigned_submission_id,
        "sub_",
        "assigned_submission_id",
    )?;
    validate_text(&assignment.reviewer_id, 1, 255, "reviewer_id")?;
    reject_public_path_references(&assignment.reviewer_id, "reviewer_id")
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
        return Err(invalid("review fingerprint must be lowercase SHA-256"));
    }
    Ok(())
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
