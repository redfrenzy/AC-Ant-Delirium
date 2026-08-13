use super::{GovernanceErrorsProjection, ProjectionMetadata, TransactionalStorageResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedGovernanceViolation {
    pub violation_id: String,
    pub rule_id: String,
    pub summary: String,
    pub path: String,
    pub line: usize,
    pub quick_fix: Option<String>,
    pub public_rule_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceErrorsProjectionDocument {
    pub metadata: ProjectionMetadata,
    pub violations: Vec<ProjectedGovernanceViolation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceErrorsProjectionContext {
    pub metadata: ProjectionMetadata,
    pub violation_ids: Vec<String>,
}

pub fn build_governance_errors_projection(
    document: &GovernanceErrorsProjectionDocument,
) -> TransactionalStorageResult<GovernanceErrorsProjection> {
    super::Governance_Errors_Projection_structcore::build_governance_errors_projection(document)
}
