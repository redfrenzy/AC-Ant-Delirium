use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectSummary {
    pub(crate) repository_path: String,
    pub(crate) agent_count: usize,
    pub(crate) active_agent_count: usize,
    pub(crate) worktree_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceLoginSnapshot {
    pub(crate) agent_id: String,
    pub(crate) branch: String,
    pub(crate) base_revision: String,
    pub(crate) workspace_revision: String,
    pub(crate) repository_revision: String,
    pub(crate) task_id: String,
    pub(crate) task_version: i64,
    pub(crate) task_revision: String,
    pub(crate) task_content_hash: String,
    pub(crate) task_file_path: String,
    pub(crate) repository_path: String,
    pub(crate) workspace_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncMetadata {
    pub(crate) branch: String,
    pub(crate) base_revision: String,
    pub(crate) workspace_revision: String,
    pub(crate) repository_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentSyncState {
    pub(crate) sync_state: String,
    pub(crate) repository_revision: String,
    pub(crate) workspace_revision: String,
    pub(crate) base_revision: String,
    pub(crate) rebase_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentSubmitState {
    pub(crate) submit_state: String,
    pub(crate) has_submit_message: bool,
    pub(crate) has_submit_feedback: bool,
    pub(crate) submit_queue_status: String,
    pub(crate) task_id: String,
    pub(crate) task_version: i64,
    pub(crate) task_revision: String,
    pub(crate) task_file_path: String,
    pub(crate) task_stale: bool,
    pub(crate) stale_reason: String,
    pub(crate) decision_by_agent_id: String,
    pub(crate) decision_note: String,
    pub(crate) decided_at: i64,
    pub(crate) latest_submit_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootOrchestrationFinding {
    pub(crate) code: String,
    pub(crate) message: String,
}

pub(crate) struct RuntimeLockGuard {
    pub(crate) path: PathBuf,
}
