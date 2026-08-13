use super::{
    reject_public_path_references, validate_project_relative_path, GovernanceErrorsProjection,
    GovernanceErrorsProjectionDocument, ProjectedGovernanceViolation, TransactionalStorageError,
    TransactionalStorageResult, AGENT_PROJECTION_CONTRACT_VERSION,
};

use super::Current_Task_Projection_structcore::{
    json_escape, validate_projection_metadata, validate_text,
};

pub(super) fn build_governance_errors_projection(
    document: &GovernanceErrorsProjectionDocument,
) -> TransactionalStorageResult<GovernanceErrorsProjection> {
    validate_document(document)?;
    Ok(GovernanceErrorsProjection {
        contract_version: AGENT_PROJECTION_CONTRACT_VERSION.to_string(),
        metadata: document.metadata.clone(),
        serialized_contract: serialize_document(document).into_bytes(),
    })
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
    validate_text(&violation.rule_id, 1, 255, "rule_id")?;
    reject_public_path_references(&violation.rule_id, "rule_id")?;
    validate_text(&violation.summary, 1, 8_192, "violation summary")?;
    reject_public_path_references(&violation.summary, "violation summary")?;
    validate_project_relative_path(&violation.path, "violation path")?;
    if violation.line < 1 {
        return Err(invalid("violation line must be at least one"));
    }
    if let Some(quick_fix) = violation.quick_fix.as_deref() {
        validate_text(quick_fix, 0, 8_192, "quick_fix")?;
        reject_public_path_references(quick_fix, "quick_fix")?;
    }
    validate_text(
        &violation.public_rule_reference,
        1,
        4_096,
        "public_rule_reference",
    )?;
    reject_public_path_references(&violation.public_rule_reference, "public_rule_reference")?;
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

fn invalid(message: &str) -> TransactionalStorageError {
    TransactionalStorageError::InvalidInput(message.to_string())
}
