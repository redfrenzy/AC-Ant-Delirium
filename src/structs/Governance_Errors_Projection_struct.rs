use super::{
    AtomicProjectProjectionWriter, AuditViolation, EventWriteOutcome, GovernanceErrorsProjection,
    ProjectDatabase, ProjectionMetadata, ProjectionWriteReceipt, TransactionalStorageResult,
};

pub const CANONICAL_GOVERNANCE_SNAPSHOT_VERSION: &str = "canonical-governance-snapshot-v1";

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
pub struct CanonicalGovernanceSnapshotRequest {
    pub project_id: String,
    pub workspace_id: String,
    pub observed_at: String,
    pub violations: Vec<ProjectedGovernanceViolation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceErrorsRefreshRequest {
    pub project_id: String,
    pub workspace_id: String,
    pub rendered_at: String,
}

pub fn build_governance_errors_projection(
    document: &GovernanceErrorsProjectionDocument,
) -> TransactionalStorageResult<GovernanceErrorsProjection> {
    super::Governance_Errors_Projection_structcore::build_governance_errors_projection(document)
}

pub fn is_agent_correctable_violation_path(path: &str) -> bool {
    super::Governance_Errors_Projection_structcore::is_agent_correctable_violation_path(path)
}

pub fn project_audit_violations(
    project_id: &str,
    workspace_id: &str,
    violations: &[AuditViolation],
) -> TransactionalStorageResult<Vec<ProjectedGovernanceViolation>> {
    super::Governance_Errors_Projection_structcore::project_audit_violations(
        project_id,
        workspace_id,
        violations,
    )
}

impl AtomicProjectProjectionWriter {
    pub fn replace_canonical_governance_snapshot(
        &self,
        request: &CanonicalGovernanceSnapshotRequest,
    ) -> TransactionalStorageResult<EventWriteOutcome> {
        super::Governance_Errors_Projection_structcore::replace_canonical_governance_snapshot(
            self, request,
        )
    }

    pub fn refresh_governance_errors_projection(
        &self,
        request: &GovernanceErrorsRefreshRequest,
    ) -> TransactionalStorageResult<ProjectionWriteReceipt> {
        super::Governance_Errors_Projection_structcore::refresh_governance_errors_projection(
            self, request,
        )
    }
}

impl ProjectDatabase {
    pub fn initialize_empty_governance_snapshot(
        &self,
        project_id: &str,
        workspace_id: &str,
        observed_at: &str,
    ) -> TransactionalStorageResult<()> {
        super::Governance_Errors_Projection_structcore::initialize_empty_governance_snapshot(
            self,
            project_id,
            workspace_id,
            observed_at,
        )
    }

    pub fn replace_canonical_governance_snapshot(
        &self,
        request: &CanonicalGovernanceSnapshotRequest,
    ) -> TransactionalStorageResult<EventWriteOutcome> {
        super::Governance_Errors_Projection_structcore::replace_canonical_governance_snapshot_in_database(
            self, request,
        )
    }

    pub fn render_governance_errors_projection(
        &self,
        project_id: &str,
        workspace_id: &str,
        rendered_at: &str,
    ) -> TransactionalStorageResult<Vec<u8>> {
        super::Governance_Errors_Projection_structcore::render_governance_errors_projection(
            self,
            project_id,
            workspace_id,
            rendered_at,
        )
    }
}
