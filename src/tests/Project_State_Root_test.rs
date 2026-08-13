#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::structs::{LegacyProjectStateRootAdapter, PluginContext, ProjectStateRoot};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after Unix epoch")
            .as_nanos();
        env::temp_dir().join(format!("{label}_{}_{}", std::process::id(), nonce))
    }

    #[test]
    fn resolves_state_files_from_one_physical_root() {
        let state = PathBuf::from("/operator/state/project-key");
        let project = PathBuf::from("/work/project");
        let workspace = PathBuf::from("/work/project-feature");
        let root = ProjectStateRoot::with_default_config(
            state.clone(),
            project.clone(),
            workspace.clone(),
        );

        assert_eq!(root.root(), state.as_path());
        assert_eq!(
            root.config_path(),
            state.join("ant_delirium_config").as_path()
        );
        assert_eq!(root.plan_dir(), state.join("plan"));
        assert_eq!(root.problems_dir(), state.join("problems"));
        assert_eq!(root.project_state_dir(), state.join("project_state"));
        assert_eq!(root.connected_path(), state.join("connected.json"));
        assert_eq!(
            root.task_orchestrator_path(),
            state.join("task_orchestrator.json")
        );
        assert_eq!(root.task_index_path(), state.join("task_index.txt"));
        assert_eq!(
            root.runtime_lock_path(),
            state.join(".ac_ant_delirium_runtime.lock")
        );
    }

    #[test]
    fn keeps_external_state_separate_from_project_and_workspace_projections() {
        let state = PathBuf::from("/operator/state/project-key");
        let project = PathBuf::from("/work/project");
        let workspace = PathBuf::from("/work/project-feature");
        let root = ProjectStateRoot::with_default_config(state.clone(), project.clone(), workspace);

        assert_eq!(
            root.todo_projection_path(),
            project.join("guard/to-do_list.json")
        );
        assert_eq!(
            root.governance_error_projection_path(),
            project.join("guard/governance_error.json")
        );
        assert!(root.plan_dir().starts_with(&state));
        assert!(!root.plan_dir().starts_with(&project));
    }

    #[test]
    fn plugin_context_preserves_the_current_local_layout() {
        let plugin = PluginContext {
            plugin_dir: PathBuf::from("/project/plugins/example"),
            config_path: PathBuf::from("/project/plugins/example/custom-config"),
            repository_path: PathBuf::from("/project"),
            workspace_path: PathBuf::from("/project-worktree"),
            is_worktree: true,
        };
        let root = plugin.state_root();

        assert_eq!(root.root(), plugin.plugin_dir.as_path());
        assert_eq!(root.config_path(), plugin.config_path.as_path());
        assert_eq!(plugin.connected_path(), root.connected_path());
        assert_eq!(plugin.login_path(), root.workspace_login_path());
        assert_eq!(root.plan_dir(), plugin.plugin_dir.join("plan"));
    }

    #[test]
    fn transition_reader_prefers_current_layout_and_supports_legacy_layout() {
        let temp = unique_temp_dir("project_state_root_transition");
        let root = ProjectStateRoot::with_default_config(
            temp.clone(),
            temp.join("project"),
            temp.join("workspace"),
        );
        let legacy_path = root.legacy_global_dir().join("transition_task.json");
        fs::create_dir_all(root.legacy_global_dir()).expect("legacy global dir should be created");
        fs::write(&legacy_path, "{}\n").expect("legacy transition should be written");
        assert_eq!(root.global_transition_task_path(), legacy_path);

        let current_path = root.current_global_transition_task_path();
        fs::create_dir_all(root.global_dir()).expect("current global dir should be created");
        fs::write(&current_path, "{}\n").expect("current transition should be written");
        assert_eq!(root.global_transition_task_path(), current_path);

        fs::remove_dir_all(&temp).expect("temporary transition tree should be removed");
    }

    #[test]
    fn legacy_adapter_only_maps_paths_without_touching_the_filesystem() {
        let temp = unique_temp_dir("legacy_state_root_adapter");
        let plugin = temp.join("project/plugins/example");
        let config = plugin.join("ant_delirium_config");
        let project = temp.join("project");
        let workspace = temp.join("worktree");

        let adapter = LegacyProjectStateRootAdapter::new(
            plugin.clone(),
            config.clone(),
            project.clone(),
            workspace.clone(),
        );
        let state_root = adapter.state_root();

        assert_eq!(state_root.root(), plugin);
        assert_eq!(state_root.config_path(), config);
        assert_eq!(state_root.project_root(), project);
        assert_eq!(state_root.workspace_root(), workspace);
        assert!(
            !temp.exists(),
            "adapting the legacy layout must not create, copy, or migrate anything"
        );
    }
}
