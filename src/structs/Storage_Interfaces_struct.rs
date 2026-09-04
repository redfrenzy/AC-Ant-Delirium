use std::path::{Path, PathBuf};

use super::{
    CanonicalTaskProjectionRef, EventWriteOutcome, NewAuditEvent, ProjectDatabase,
    ProjectStateRoot, StorageProject, TransactionalStorageResult,
};

pub const CANONICAL_STATE_CONTRACT_VERSION: &str = "canonical-state-v1";
pub const SUBMISSION_CONTRACT_VERSION: &str = "submission-v1";
pub const AUDIT_EVENT_CONTRACT_VERSION: &str = "audit-event-v1";
pub const AGENT_PROJECTION_CONTRACT_VERSION: &str = "agent-projection-v1";
pub const GOVERNANCE_TASK_PACKAGE_CONTRACT_VERSION: &str = "governance-task-package-v1";

pub fn materialize_limited_project_projections(
    project_root: &Path,
    current_task: &[u8],
    governance_errors: &[u8],
) -> TransactionalStorageResult<()> {
    super::Storage_Interfaces_structcore::materialize_limited_project_projections(
        project_root,
        current_task,
        governance_errors,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageWorkspace {
    pub workspace_id: String,
    pub project_id: String,
    pub workspace_key: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTaskState {
    pub task_id: String,
    pub task_revision: i64,
    pub status: String,
    pub review_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalStateSnapshot {
    pub contract_version: String,
    pub project_id: String,
    pub workspace_id: String,
    pub state_revision: i64,
    pub active_plan_id: Option<String>,
    pub current_task_id: Option<String>,
    pub tasks: Vec<CanonicalTaskState>,
    pub last_event_sequence: i64,
    pub projection_generation: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalStateTransition {
    pub project_id: String,
    pub workspace_id: String,
    pub expected_state_revision: i64,
    pub active_plan_id: Option<String>,
    pub current_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTaskPackageTask {
    pub task_id: String,
    pub task_revision: i64,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub relevant_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTaskPackage {
    pub contract_version: String,
    pub project_id: String,
    pub workspace_id: String,
    pub plan_id: String,
    pub expected_state_revision: i64,
    pub tasks: Vec<CanonicalTaskPackageTask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTaskAdvance {
    pub project_id: String,
    pub workspace_id: String,
    pub task_id: String,
    pub review_id: String,
    pub expected_state_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalMutationOutcome {
    pub state: CanonicalStateSnapshot,
    pub audit: EventWriteOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTaskAdvanceOutcome {
    pub state: CanonicalStateSnapshot,
    pub audit: EventWriteOutcome,
    pub completed_task_id: String,
    pub next_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupCheckpointRecord {
    pub transition_kind: String,
    pub transition_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub task_revision: Option<i64>,
    pub state_revision: i64,
    pub commit_id: String,
    pub parent_commit_id: Option<String>,
    pub source_fingerprint: String,
    pub entry_count: i64,
    pub byte_count: i64,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSubmission {
    pub contract_version: String,
    pub submission_id: String,
    pub request_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub task_id: String,
    pub task_revision: i64,
    pub base_revision: Option<String>,
    pub fingerprint: String,
    pub payload: Vec<u8>,
    pub received_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionAssignment {
    pub project_id: String,
    pub workspace_id: String,
    pub task_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignedSubmission {
    pub assignment: SubmissionAssignment,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision {
    Accepted,
    Rejected,
}

impl ReviewDecision {
    pub fn storage_value(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReview {
    pub review_id: String,
    pub project_id: String,
    pub submission_id: String,
    pub reviewer_id: String,
    pub decision: ReviewDecision,
    pub fingerprint: String,
    pub payload: Vec<u8>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmissionWriteOutcome {
    Inserted {
        submission_id: String,
        event_sequence: i64,
    },
    Duplicate {
        submission_id: String,
        event_sequence: i64,
    },
}

impl SubmissionWriteOutcome {
    pub fn submission_id(&self) -> &str {
        match self {
            Self::Inserted { submission_id, .. } | Self::Duplicate { submission_id, .. } => {
                submission_id
            }
        }
    }

    pub fn inserted(&self) -> bool {
        matches!(self, Self::Inserted { .. })
    }

    pub fn event_sequence(&self) -> i64 {
        match self {
            Self::Inserted { event_sequence, .. } | Self::Duplicate { event_sequence, .. } => {
                *event_sequence
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewWriteOutcome {
    Inserted {
        review_id: String,
        event_sequence: i64,
    },
    Duplicate {
        review_id: String,
        event_sequence: i64,
    },
}

impl ReviewWriteOutcome {
    pub fn review_id(&self) -> &str {
        match self {
            Self::Inserted { review_id, .. } | Self::Duplicate { review_id, .. } => review_id,
        }
    }

    pub fn inserted(&self) -> bool {
        matches!(self, Self::Inserted { .. })
    }

    pub fn event_sequence(&self) -> i64 {
        match self {
            Self::Inserted { event_sequence, .. } | Self::Duplicate { event_sequence, .. } => {
                *event_sequence
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionMetadata {
    pub project_id: String,
    pub workspace_id: String,
    pub source_revision: i64,
    pub generation: i64,
    pub fingerprint: String,
    pub rendered_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentTaskProjection {
    pub(crate) contract_version: String,
    pub(crate) metadata: ProjectionMetadata,
    pub(crate) task_ref: Option<CanonicalTaskProjectionRef>,
    pub(crate) serialized_contract: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceErrorsProjection {
    pub(crate) contract_version: String,
    pub(crate) metadata: ProjectionMetadata,
    pub(crate) snapshot_fingerprint: Option<String>,
    pub(crate) serialized_contract: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionArtifactKind {
    CurrentTask,
    GovernanceErrors,
}

impl ProjectionArtifactKind {
    pub fn storage_name(self) -> &'static str {
        match self {
            Self::CurrentTask => "current_task",
            Self::GovernanceErrors => "governance_errors",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionWriteReceipt {
    pub artifact_kind: ProjectionArtifactKind,
    pub generation: i64,
    pub audit: EventWriteOutcome,
}

#[derive(Debug, Clone)]
pub struct AtomicProjectProjectionWriter {
    pub(crate) database: ProjectDatabase,
    pub(crate) state_root: ProjectStateRoot,
}

pub trait CanonicalStateStore {
    fn register_project(&self, project: &StorageProject) -> TransactionalStorageResult<()>;

    fn register_workspace(&self, workspace: &StorageWorkspace) -> TransactionalStorageResult<()>;

    fn load_canonical_state(
        &self,
        project_id: &str,
        workspace_id: &str,
    ) -> TransactionalStorageResult<CanonicalStateSnapshot>;

    fn apply_canonical_transition(
        &self,
        transition: &CanonicalStateTransition,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<CanonicalMutationOutcome>;

    fn install_task_package(
        &self,
        package: &CanonicalTaskPackage,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<CanonicalMutationOutcome>;

    fn advance_accepted_task(
        &self,
        advance: &CanonicalTaskAdvance,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<CanonicalTaskAdvanceOutcome>;

    fn advance_accepted_task_with_checkpoint(
        &self,
        advance: &CanonicalTaskAdvance,
        audit_event: &NewAuditEvent,
        checkpoint: &BackupCheckpointRecord,
    ) -> TransactionalStorageResult<CanonicalTaskAdvanceOutcome>;
}

pub trait SubmissionStore {
    fn record_submission(
        &self,
        submission: &NewSubmission,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<SubmissionWriteOutcome>;

    fn record_submission_with_checkpoint(
        &self,
        submission: &NewSubmission,
        audit_event: &NewAuditEvent,
        checkpoint: &BackupCheckpointRecord,
    ) -> TransactionalStorageResult<SubmissionWriteOutcome>;

    fn submission_id_for_request(
        &self,
        project_id: &str,
        request_id: &str,
        fingerprint: &str,
    ) -> TransactionalStorageResult<Option<String>>;

    fn submission_count(&self, project_id: &str) -> TransactionalStorageResult<i64>;

    fn submission_assignment(
        &self,
        project_id: &str,
        submission_id: &str,
    ) -> TransactionalStorageResult<Option<SubmissionAssignment>>;

    fn read_assigned_submission(
        &self,
        project_id: &str,
        submission_id: &str,
    ) -> TransactionalStorageResult<Option<AssignedSubmission>>;
}

pub trait ReviewStore {
    fn record_review(
        &self,
        review: &NewReview,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<ReviewWriteOutcome>;

    fn record_review_with_checkpoint(
        &self,
        review: &NewReview,
        audit_event: &NewAuditEvent,
        checkpoint: &BackupCheckpointRecord,
    ) -> TransactionalStorageResult<ReviewWriteOutcome>;

    fn review_id_for_submission(
        &self,
        project_id: &str,
        submission_id: &str,
    ) -> TransactionalStorageResult<Option<String>>;

    fn review_assignment(
        &self,
        project_id: &str,
        review_id: &str,
    ) -> TransactionalStorageResult<Option<SubmissionAssignment>>;
}

pub trait BackupCheckpointStore {
    fn record_backup_checkpoint(
        &self,
        checkpoint: &BackupCheckpointRecord,
    ) -> TransactionalStorageResult<bool>;

    fn backup_checkpoint(
        &self,
        project_id: &str,
        transition_kind: &str,
        transition_id: &str,
    ) -> TransactionalStorageResult<Option<BackupCheckpointRecord>>;

    fn backup_checkpoint_recovery_required(
        &self,
        project_id: &str,
        workspace_id: &str,
    ) -> TransactionalStorageResult<bool>;
}

pub trait AuditStore {
    fn append_audit_event(
        &self,
        event: &NewAuditEvent,
    ) -> TransactionalStorageResult<EventWriteOutcome>;

    fn audit_event_count(&self, project_id: &str) -> TransactionalStorageResult<i64>;

    fn last_audit_sequence(&self, project_id: &str) -> TransactionalStorageResult<i64>;
}

pub trait ProjectProjectionWriter {
    fn write_current_task(
        &self,
        projection: &CurrentTaskProjection,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<ProjectionWriteReceipt>;

    fn write_governance_errors(
        &self,
        projection: &GovernanceErrorsProjection,
        audit_event: &NewAuditEvent,
    ) -> TransactionalStorageResult<ProjectionWriteReceipt>;

    fn current_task_path(&self) -> PathBuf;

    fn governance_errors_path(&self) -> PathBuf;
}
