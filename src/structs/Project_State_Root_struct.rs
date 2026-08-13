use std::path::{Path, PathBuf};

use crate::engine_identity;

/// Resolves every fixed AntDelirium state and projection path from explicit roots.
///
/// The current plugin layout and the future external layout use the same contract:
/// callers choose the physical state root, while this type owns the directory and
/// file names below it. It performs no discovery and does not create files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStateRoot {
    state_root: PathBuf,
    config_path: PathBuf,
    project_root: PathBuf,
    workspace_root: PathBuf,
}

/// Temporary compatibility boundary for the project-local plugin layout.
///
/// This adapter only maps legacy paths into `ProjectStateRoot`. It never
/// discovers, copies, migrates, deletes, or changes the authority of state.
/// External enrollment uses `ProjectStateRoot` directly and can therefore run
/// the same stores and writers without depending on `plugins/<project>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyProjectStateRootAdapter {
    state_root: ProjectStateRoot,
}

impl LegacyProjectStateRootAdapter {
    pub fn new(
        plugin_dir: PathBuf,
        config_path: PathBuf,
        project_root: PathBuf,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            state_root: ProjectStateRoot::new(
                plugin_dir,
                config_path,
                project_root,
                workspace_root,
            ),
        }
    }

    pub fn state_root(&self) -> &ProjectStateRoot {
        &self.state_root
    }

    pub fn into_state_root(self) -> ProjectStateRoot {
        self.state_root
    }
}

impl ProjectStateRoot {
    pub fn new(
        state_root: PathBuf,
        config_path: PathBuf,
        project_root: PathBuf,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            state_root,
            config_path,
            project_root,
            workspace_root,
        }
    }

    pub fn with_default_config(
        state_root: PathBuf,
        project_root: PathBuf,
        workspace_root: PathBuf,
    ) -> Self {
        let config_path = Self::default_config_path_for(&state_root);
        Self::new(state_root, config_path, project_root, workspace_root)
    }

    pub fn default_config_path_for(state_root: &Path) -> PathBuf {
        state_root.join(engine_identity::DEFAULT_CONFIG_DIR_NAME)
    }

    pub fn legacy_config_path_for(state_root: &Path) -> PathBuf {
        state_root.join("delirium.toml")
    }

    pub fn root(&self) -> &Path {
        &self.state_root
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn bin_dir(&self) -> PathBuf {
        Self::bin_dir_for(&self.state_root)
    }

    pub fn plugin_binary_path(&self) -> PathBuf {
        engine_identity::default_plugin_binary_path(&self.state_root)
    }

    pub fn guard_binary_path(&self) -> PathBuf {
        self.bin_dir()
            .join(engine_identity::DEFAULT_GUARD_BINARY_NAME)
    }

    pub fn bin_dir_for(state_root: &Path) -> PathBuf {
        state_root.join("bin")
    }

    pub fn default_hierarchy_path(&self) -> PathBuf {
        self.config_path.join("default_hierarchy.toml")
    }

    pub fn default_rules_path(&self) -> PathBuf {
        self.config_path.join("default_rules.toml")
    }

    pub fn legacy_config_path(&self) -> PathBuf {
        Self::legacy_config_path_for(&self.state_root)
    }

    pub fn plan_dir(&self) -> PathBuf {
        self.state_root.join("plan")
    }

    pub fn sample_plan_path(&self) -> PathBuf {
        self.plan_dir().join("[0.0]_Sample_Plan.json")
    }

    pub fn problems_dir(&self) -> PathBuf {
        self.state_root.join("problems")
    }

    pub fn global_dir(&self) -> PathBuf {
        self.state_root.join("global")
    }

    pub fn legacy_global_dir(&self) -> PathBuf {
        self.state_root.join("Global")
    }

    pub fn current_global_transition_task_path(&self) -> PathBuf {
        self.global_dir().join("transition_task.json")
    }

    pub fn global_transition_task_path(&self) -> PathBuf {
        let current = self.current_global_transition_task_path();
        if current.exists() {
            return current;
        }

        let legacy = self.legacy_global_dir().join("transition_task.json");
        if legacy.exists() {
            return legacy;
        }

        current
    }

    pub fn project_state_dir(&self) -> PathBuf {
        self.state_root.join("project_state")
    }

    pub fn project_state_gitignore_path(&self) -> PathBuf {
        self.project_state_dir().join(".gitignore")
    }

    pub fn workspace_states_dir(&self) -> PathBuf {
        self.project_state_dir().join("workspaces")
    }

    pub fn schemas_dir(&self) -> PathBuf {
        self.project_state_dir().join("schemas")
    }

    pub fn fixtures_dir(&self) -> PathBuf {
        self.project_state_dir().join("fixtures")
    }

    pub fn references_dir(&self) -> PathBuf {
        self.project_state_dir().join("references")
    }

    pub fn temporary_dir(&self) -> PathBuf {
        self.project_state_dir().join("tmp")
    }

    pub fn scripts_dir(&self) -> PathBuf {
        self.state_root.join("scripts")
    }

    pub fn scripts_readme_path(&self) -> PathBuf {
        self.scripts_dir().join("README.md")
    }

    pub fn unlock_template_path(&self) -> PathBuf {
        self.scripts_dir()
            .join("unlock_governance_files.template.sh")
    }

    pub fn lock_template_path(&self) -> PathBuf {
        self.scripts_dir().join("lock_governance_files.template.sh")
    }

    pub fn update_binary_template_path(&self) -> PathBuf {
        self.scripts_dir()
            .join("update_plugin_binary_and_lock.template.sh")
    }

    pub fn trash_dir(&self) -> PathBuf {
        self.state_root.join("trash")
    }

    pub fn artifacts_dir(&self) -> PathBuf {
        self.state_root.join("artifacts")
    }

    pub fn transactional_state_dir(&self) -> PathBuf {
        self.state_root.join("state")
    }

    pub fn state_lock_path(&self) -> PathBuf {
        self.transactional_state_dir().join("lock")
    }

    pub fn database_path(&self) -> PathBuf {
        self.transactional_state_dir().join("state.db")
    }

    pub fn events_path(&self) -> PathBuf {
        self.transactional_state_dir().join("events.log")
    }

    pub fn snapshot_path(&self) -> PathBuf {
        self.transactional_state_dir().join("snapshot.json")
    }

    pub fn project_metadata_path(&self) -> PathBuf {
        self.state_root.join("project.json")
    }

    pub fn connected_path(&self) -> PathBuf {
        self.state_root.join("connected.json")
    }

    pub fn governance_path(&self) -> PathBuf {
        self.state_root.join("governance.json")
    }

    pub fn governance_history_path(&self) -> PathBuf {
        self.state_root.join("governance_history.json")
    }

    pub fn context_path(&self) -> PathBuf {
        self.state_root.join("context.json")
    }

    pub fn messages_path(&self) -> PathBuf {
        self.state_root.join("messages.json")
    }

    pub fn feedback_path(&self) -> PathBuf {
        self.state_root.join("feedback.json")
    }

    pub fn manager_tasks_path(&self) -> PathBuf {
        self.state_root.join("manager_tasks.json")
    }

    pub fn submit_queue_path(&self) -> PathBuf {
        self.state_root.join("submit_queue.json")
    }

    pub fn task_orchestrator_path(&self) -> PathBuf {
        self.state_root.join("task_orchestrator.json")
    }

    pub fn task_index_path(&self) -> PathBuf {
        self.state_root.join("task_index.txt")
    }

    pub fn runtime_lock_path(&self) -> PathBuf {
        engine_identity::default_runtime_lock_path(&self.state_root)
    }

    pub fn workspace_props_dir(&self) -> PathBuf {
        Self::props_dir_for(&self.workspace_root)
    }

    pub fn workspace_login_dir(&self) -> PathBuf {
        Self::login_dir_for(&self.workspace_root)
    }

    pub fn workspace_login_path(&self) -> PathBuf {
        Self::login_path_for(&self.workspace_root)
    }

    pub fn props_dir_for(root: &Path) -> PathBuf {
        root.join("AC_Ant-Delirium_props")
    }

    pub fn login_dir_for(workspace_root: &Path) -> PathBuf {
        Self::props_dir_for(workspace_root).join("login")
    }

    pub fn login_path_for(workspace_root: &Path) -> PathBuf {
        Self::login_dir_for(workspace_root).join("login.json")
    }

    pub fn project_props_dir(&self) -> PathBuf {
        Self::props_dir_for(&self.project_root)
    }

    pub fn build_guard_path(&self) -> PathBuf {
        Self::build_guard_path_for(&self.project_root)
    }

    pub fn hooks_dir(&self) -> PathBuf {
        Self::hooks_dir_for(&self.project_root)
    }

    pub fn orchestration_dir(&self) -> PathBuf {
        Self::orchestration_dir_for(&self.project_root)
    }

    pub fn build_guard_path_for(project_root: &Path) -> PathBuf {
        Self::props_dir_for(project_root).join("ant_delirium_build_guard.rs")
    }

    pub fn hooks_dir_for(project_root: &Path) -> PathBuf {
        Self::props_dir_for(project_root).join("githooks")
    }

    pub fn orchestration_dir_for(project_root: &Path) -> PathBuf {
        Self::props_dir_for(project_root).join("orchestration")
    }

    pub fn project_guard_dir(&self) -> PathBuf {
        self.project_root.join("guard")
    }

    pub fn todo_projection_path(&self) -> PathBuf {
        self.project_guard_dir().join("to-do_list.json")
    }

    pub fn governance_error_projection_path(&self) -> PathBuf {
        self.project_guard_dir().join("governance_error.json")
    }
}
