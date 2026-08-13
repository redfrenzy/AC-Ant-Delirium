use super::{CurrentTaskProjection, ProjectionMetadata, TransactionalStorageResult};

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

pub fn build_current_task_projection(
    document: &CurrentTaskProjectionDocument,
) -> TransactionalStorageResult<CurrentTaskProjection> {
    super::Current_Task_Projection_structcore::build_current_task_projection(document)
}
