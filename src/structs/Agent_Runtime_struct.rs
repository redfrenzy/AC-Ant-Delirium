use std::path::PathBuf;

use super::{HierarchyConfig, LegacyProjectStateRootAdapter, ProjectStateRoot};

#[derive(Debug, Clone)]
pub struct PluginContext {
    pub plugin_dir: PathBuf,
    pub config_path: PathBuf,
    pub repository_path: PathBuf,
    pub workspace_path: PathBuf,
    pub is_worktree: bool,
}

impl PluginContext {
    pub fn state_root(&self) -> ProjectStateRoot {
        LegacyProjectStateRootAdapter::new(
            self.plugin_dir.clone(),
            self.config_path.clone(),
            self.repository_path.clone(),
            self.workspace_path.clone(),
        )
        .into_state_root()
    }

    pub fn props_dir(&self) -> PathBuf {
        self.state_root().workspace_props_dir()
    }

    pub fn login_dir(&self) -> PathBuf {
        self.state_root().workspace_login_dir()
    }

    pub fn login_path(&self) -> PathBuf {
        self.state_root().workspace_login_path()
    }

    pub fn connected_path(&self) -> PathBuf {
        self.state_root().connected_path()
    }

    pub fn governance_path(&self) -> PathBuf {
        self.state_root().governance_path()
    }

    pub fn governance_history_path(&self) -> PathBuf {
        self.state_root().governance_history_path()
    }

    pub fn context_path(&self) -> PathBuf {
        self.state_root().context_path()
    }

    pub fn messages_path(&self) -> PathBuf {
        self.state_root().messages_path()
    }

    pub fn feedback_path(&self) -> PathBuf {
        self.state_root().feedback_path()
    }

    pub fn manager_tasks_path(&self) -> PathBuf {
        self.state_root().manager_tasks_path()
    }

    pub fn submit_queue_path(&self) -> PathBuf {
        self.state_root().submit_queue_path()
    }

    pub fn task_orchestrator_path(&self) -> PathBuf {
        self.state_root().task_orchestrator_path()
    }

    pub fn global_transition_task_path(&self) -> PathBuf {
        self.state_root().global_transition_task_path()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedAgent {
    pub internal_id: i64,
    pub agent_id: String,
    pub branch: String,
    pub base_revision: String,
    pub workspace_revision: String,
    pub repository_revision: String,
    pub worktree_path: String,
    pub login_path: String,
    pub assigned_mission: String,
    pub last_ping: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceTask {
    pub task_id: String,
    pub task_version: i64,
    pub task_revision: String,
    pub task_content_hash: String,
    pub file_path: String,
    pub violation_count: i64,
    pub owner_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceHistoryEntry {
    pub id: String,
    pub task_id: String,
    pub file_path: String,
    pub change_kind: String,
    pub previous_version: i64,
    pub current_version: i64,
    pub previous_revision: String,
    pub current_revision: String,
    pub previous_content_hash: String,
    pub current_content_hash: String,
    pub previous_owner_id: i64,
    pub current_owner_id: i64,
    pub created_at: i64,
    pub previous_event_hash: String,
    pub event_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagerTask {
    pub id: String,
    pub title: String,
    pub body: String,
    pub file_path: String,
    pub priority: i64,
    pub owner_id: i64,
    pub status: String,
    pub created_at: i64,
    pub mode: Option<String>,
    pub grade: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberedPlanStep {
    pub step_id: String,
    pub task_prompt: String,
    pub task_reviewed: bool,
    pub review_explanation: String,
    pub implemented: bool,
    pub implementation_explanation: String,
    pub implementation_context: String,
    pub review_status: String,
    pub grade_explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberedPlan {
    pub plan_id: String,
    pub title: String,
    pub complete_plan: String,
    pub tasks: Vec<NumberedPlanStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskOrchestration {
    pub active_plan: String,
    pub current_step_id: String,
    pub mode: String,
    pub grade: f64,
    pub task_start_timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionTaskState {
    pub active: bool,
    pub source_plan: String,
    pub source_plan_id: String,
    pub final_step_id: String,
    pub transition_source: String,
    pub current_transition_id: String,
    pub current_projected_step_id: String,
    pub completed_transition_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskOrchestratorPayload {
    pub orchestration: TaskOrchestration,
    pub transition: Option<TransitionTaskState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitRecord {
    pub id: String,
    pub internal_id: i64,
    pub agent_id: String,
    pub task_id: String,
    pub task_version: i64,
    pub task_revision: String,
    pub task_content_hash: String,
    pub task_file_path: String,
    pub title: String,
    pub topic: String,
    pub branch: String,
    pub base_revision: String,
    pub workspace_revision: String,
    pub repository_revision: String,
    pub status: String,
    pub decision_by_internal_id: i64,
    pub decision_by_agent_id: String,
    pub decision_note: String,
    pub decided_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeCommandContext {
    pub plugin: Option<PluginContext>,
    pub hierarchy: Option<HierarchyConfig>,
    pub hierarchy_hint_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonEntry {
    pub fields: Vec<(String, JsonValue)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonValue {
    String(String),
    Number(i64),
}
