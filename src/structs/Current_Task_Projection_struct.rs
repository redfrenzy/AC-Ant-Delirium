use super::{
    AtomicProjectProjectionWriter, CurrentTaskProjection, ProjectionMetadata,
    ProjectionWriteReceipt, TransactionalStorageResult,
};

pub const CANONICAL_TASK_PROJECTION_PAYLOAD_VERSION: &str = "canonical-task-projection-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectedTaskAction {
    Implement,
    Review,
    PlanReview,
    Wait,
}

impl ProjectedTaskAction {
    pub fn contract_value(self) -> &'static str {
        match self {
            Self::Implement => "implement",
            Self::Review => "review",
            Self::PlanReview => "plan_review",
            Self::Wait => "wait",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectedQueueStatus {
    TaskAvailable,
    AwaitingReview,
    Blocked,
    NoTaskAvailable,
}

impl ProjectedQueueStatus {
    pub fn contract_value(self) -> &'static str {
        match self {
            Self::TaskAvailable => "task_available",
            Self::AwaitingReview => "awaiting_review",
            Self::Blocked => "blocked",
            Self::NoTaskAvailable => "no_task_available",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedCurrentTask {
    pub task_id: String,
    pub task_revision: i64,
    pub objective: String,
    pub allowed_action: ProjectedTaskAction,
    pub acceptance_criteria: Vec<String>,
    pub relevant_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentTaskProjectionDocument {
    pub metadata: ProjectionMetadata,
    pub task: Option<ProjectedCurrentTask>,
    pub queue_status: ProjectedQueueStatus,
    pub submission_instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTaskProjectionRef {
    pub task_id: String,
    pub task_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoProjectionContext {
    pub metadata: ProjectionMetadata,
    pub task_ref: Option<CanonicalTaskProjectionRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTaskProjectionPayload {
    pub contract_version: String,
    pub objective: String,
    pub allowed_action: ProjectedTaskAction,
    pub acceptance_criteria: Vec<String>,
    pub relevant_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentTaskProjectionRefreshRequest {
    pub project_id: String,
    pub workspace_id: String,
    pub rendered_at: String,
}

pub fn build_current_task_projection(
    document: &CurrentTaskProjectionDocument,
) -> TransactionalStorageResult<CurrentTaskProjection> {
    super::Current_Task_Projection_structcore::build_current_task_projection(document)
}

pub fn encode_canonical_task_projection_payload(
    payload: &CanonicalTaskProjectionPayload,
) -> TransactionalStorageResult<Vec<u8>> {
    super::Current_Task_Projection_structcore::encode_canonical_task_projection_payload(payload)
}

impl AtomicProjectProjectionWriter {
    pub fn refresh_current_task_projection(
        &self,
        request: &CurrentTaskProjectionRefreshRequest,
    ) -> TransactionalStorageResult<ProjectionWriteReceipt> {
        super::Current_Task_Projection_structcore::refresh_current_task_projection(self, request)
    }
}
