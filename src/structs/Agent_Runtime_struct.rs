use std::path::PathBuf;

use super::HierarchyConfig;

#[derive(Debug, Clone)]
pub struct PluginContext {
    pub plugin_dir: PathBuf,
    pub config_path: PathBuf,
    pub repository_path: PathBuf,
    pub workspace_path: PathBuf,
    pub is_worktree: bool,
}

impl PluginContext {
    pub fn props_dir(&self) -> PathBuf {
        self.workspace_path.join("AC_Ant-Delirium_props")
    }

    pub fn login_dir(&self) -> PathBuf {
        self.props_dir().join("login")
    }

    pub fn login_path(&self) -> PathBuf {
        self.login_dir().join("login.json")
    }

    pub fn connected_path(&self) -> PathBuf {
        self.plugin_dir.join("connected.json")
    }

    pub fn governance_path(&self) -> PathBuf {
        self.plugin_dir.join("governance.json")
    }

    pub fn governance_history_path(&self) -> PathBuf {
        self.plugin_dir.join("governance_history.json")
    }

    pub fn context_path(&self) -> PathBuf {
        self.plugin_dir.join("context.json")
    }

    pub fn messages_path(&self) -> PathBuf {
        self.plugin_dir.join("messages.json")
    }

    pub fn feedback_path(&self) -> PathBuf {
        self.plugin_dir.join("feedback.json")
    }

    pub fn manager_tasks_path(&self) -> PathBuf {
        self.plugin_dir.join("manager_tasks.json")
    }

    pub fn submit_queue_path(&self) -> PathBuf {
        self.plugin_dir.join("submit_queue.json")
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerTask {
    pub id: String,
    pub title: String,
    pub body: String,
    pub file_path: String,
    pub priority: i64,
    pub owner_id: i64,
    pub status: String,
    pub created_at: i64,
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
