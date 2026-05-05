use super::*;
use ac_ant_delirium::engine_identity;
use ac_ant_delirium::structs::{GovernanceMode, PluginContext};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let dir = env::temp_dir().join(format!("{}_{}", label, nanos));
    fs::create_dir_all(&dir).expect("temporary test directory should exist");
    dir
}

fn write_runtime_plugin_config(plugin_dir: &Path, repository_path: &Path) {
    let config_dir = plugin_dir.join("ant_delirium_config");
    fs::create_dir_all(&config_dir).expect("plugin config directory should exist");
    let hierarchy = format!(
        "[hierarchy]\nrepository = \"{}\"\n[hierarchy.target]\ncore = [\"src\"]\ntests = [\"tests\"]\n",
        repository_path.to_string_lossy().replace('\\', "/")
    );
    fs::write(config_dir.join("default_hierarchy.toml"), hierarchy)
        .expect("default_hierarchy.toml should be written");
    fs::write(config_dir.join("default_rules.toml"), "# test rules\n")
        .expect("default_rules.toml should be written");
}

fn temp_plugin_context(root: &Path, plugin_name: &str) -> PluginContext {
    let repository_path = root.join("repo");
    let plugin_dir = root.join("plugins").join(plugin_name);
    fs::create_dir_all(repository_path.join("src")).expect("repo src should exist");
    write_runtime_plugin_config(&plugin_dir, &repository_path);
    PluginContext {
        plugin_dir: plugin_dir.clone(),
        config_path: plugin_dir.join("ant_delirium_config"),
        repository_path: repository_path.clone(),
        workspace_path: repository_path,
        is_worktree: false,
    }
}

fn canonical_bootstrap_plugin_path(project_name: &str) -> String {
    format!(
        "../plugins/{}/{}",
        project_name,
        engine_identity::DEFAULT_PLUGIN_BINARY_NAME
    )
}

#[test]
fn parser_supports_professional_command_surface() {
    let args = vec![
        "--json".to_string(),
        "config".to_string(),
        "dump".to_string(),
        "--help".to_string(),
    ];

    let parsed = parse_cli_args(&args);
    assert!(parsed.json_requested);
    assert!(parsed.config_dump);
    assert!(parsed.help_requested);
}

#[test]
fn parser_accepts_config_init_subcommand() {
    let args = vec!["config".to_string(), "init".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.config_init);
}

#[test]
fn parser_accepts_hooks_init_subcommand() {
    let args = vec!["config".to_string(), "hooks".to_string(), "init".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.hooks_init);
}

#[test]
fn parser_accepts_hooks_verify_merge_subcommand() {
    let args = vec![
        "config".to_string(),
        "hooks".to_string(),
        "verify".to_string(),
        "merge".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert!(parsed.hooks_verify_merge);
}

#[test]
fn parser_accepts_hooks_verify_push_subcommand() {
    let args = vec![
        "config".to_string(),
        "hooks".to_string(),
        "verify".to_string(),
        "push".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert!(parsed.hooks_verify_push);
}

#[test]
fn parser_accepts_swarm_status_subcommand() {
    let args = vec!["swarm".to_string(), "status".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.swarm_status);
}

#[test]
fn parser_accepts_getprojects_alias() {
    let args = vec!["--getprojects".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.get_projects);
}

#[test]
fn parser_accepts_whoami_and_show_agent() {
    let args = vec!["--whoami".to_string(), "--show-agent".to_string(), "3".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.whoami);
    assert_eq!(parsed.show_agent_id, Some(3));
}

#[test]
fn parser_accepts_rebase_check() {
    let args = vec!["--rebase-check".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.rebase_check);
}

#[test]
fn parser_accepts_root_checks() {
    let args = vec!["--root-checks".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.root_checks);
}

#[test]
fn parser_accepts_release_stale_agent_claims() {
    let args = vec!["--release-stale-agent-claims".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.release_stale_agent_claims);
}

#[test]
fn parser_accepts_prune_stale_agents() {
    let args = vec!["--prune-stale-agents".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.prune_stale_agents);
}

#[test]
fn parser_accepts_governance_history() {
    let args = vec!["--governance-history".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.governance_history);
}

#[test]
fn parser_accepts_show_agent_id_uid() {
    let args = vec!["--show-agent-id".to_string(), "BMXIX1Z2".to_string()];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.show_agent_uid.as_deref(), Some("BMXIX1Z2"));
}

#[test]
fn parser_accepts_messages_thread_and_topic() {
    let args = vec![
        "--messages".to_string(),
        "--thread".to_string(),
        "range.rs".to_string(),
        "--message".to_string(),
        "check this".to_string(),
        "--topic".to_string(),
        "range.rs".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert!(parsed.messages_feed);
    assert_eq!(parsed.thread_topic.as_deref(), Some("range.rs"));
    assert_eq!(parsed.message_topic.as_deref(), Some("range.rs"));
}

#[test]
fn parser_accepts_feedback_context_and_help() {
    let args = vec![
        "--feedback".to_string(),
        "explain better".to_string(),
        "--context".to_string(),
        "hierarchy.files.max_nesting".to_string(),
        "--help.feedback".to_string(),
    ];

    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.feedback.as_deref(), Some("explain better"));
    assert_eq!(
        parsed.feedback_context.as_deref(),
        Some("hierarchy.files.max_nesting")
    );
    assert_eq!(parsed.plugin_info_query.as_deref(), Some("feedback_help"));
}

#[test]
fn parser_accepts_message_sender_and_recipient_controls() {
    let args = vec![
        "--message".to_string(),
        "sync before push".to_string(),
        "--to-agent".to_string(),
        "3".to_string(),
        "--from-agent".to_string(),
        "1".to_string(),
    ];

    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.message_body.as_deref(), Some("sync before push"));
    assert_eq!(parsed.message_target_agent_id, Some(3));
    assert_eq!(parsed.message_sender_agent_id, Some(1));
}

#[test]
fn parser_accepts_next_action() {
    let args = vec!["--next-action".to_string()];
    let parsed = parse_cli_args(&args);
    assert!(parsed.next_action);
}

#[test]
fn parser_accepts_message_agent_uid_controls() {
    let args = vec![
        "--message".to_string(),
        "sync before push".to_string(),
        "--to-agent-id".to_string(),
        "C55HN2KI".to_string(),
        "--from-agent-id".to_string(),
        "BMXIX1Z2".to_string(),
    ];

    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.message_body.as_deref(), Some("sync before push"));
    assert_eq!(parsed.message_target_agent_uid.as_deref(), Some("C55HN2KI"));
    assert_eq!(parsed.message_sender_agent_uid.as_deref(), Some("BMXIX1Z2"));
}

#[test]
fn parser_accepts_metadata_driven_sync_flags() {
    let args = vec![
        "--message".to_string(),
        "rebase required".to_string(),
        "--message-kind".to_string(),
        "sync".to_string(),
        "--branch".to_string(),
        "ant-agent-1".to_string(),
        "--base-revision".to_string(),
        "abc123".to_string(),
        "--workspace-revision".to_string(),
        "def456".to_string(),
        "--repository-revision".to_string(),
        "789xyz".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.message_kind.as_deref(), Some("sync"));
    assert_eq!(parsed.branch.as_deref(), Some("ant-agent-1"));
    assert_eq!(parsed.base_revision.as_deref(), Some("abc123"));
    assert_eq!(parsed.workspace_revision.as_deref(), Some("def456"));
    assert_eq!(parsed.repository_revision.as_deref(), Some("789xyz"));
}

#[test]
fn parser_accepts_submit_surface() {
    let args = vec![
        "--submit".to_string(),
        "ready to integrate".to_string(),
        "--topic".to_string(),
        "submit.range".to_string(),
        "--context".to_string(),
        "hierarchy.submit".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.submit_body.as_deref(), Some("ready to integrate"));
    assert_eq!(parsed.message_topic.as_deref(), Some("submit.range"));
    assert_eq!(parsed.feedback_context.as_deref(), Some("hierarchy.submit"));
}

#[test]
fn parser_accepts_submit_queue_actions() {
    let args = vec![
        "--submits".to_string(),
        "--submits-active".to_string(),
        "--submits-history".to_string(),
        "--submit-show".to_string(),
        "SUBMIT000".to_string(),
        "--submit-note".to_string(),
        "looks good".to_string(),
        "--submit-accept".to_string(),
        "SUBMIT001".to_string(),
        "--submit-close".to_string(),
        "SUBMIT002".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert!(parsed.submits);
    assert!(parsed.submits_active);
    assert!(parsed.submits_history);
    assert_eq!(parsed.submit_show.as_deref(), Some("SUBMIT000"));
    assert_eq!(parsed.submit_note.as_deref(), Some("looks good"));
    assert_eq!(parsed.submit_accept.as_deref(), Some("SUBMIT001"));
    assert_eq!(parsed.submit_close.as_deref(), Some("SUBMIT002"));
}

#[test]
fn parser_accepts_manager_task_surface() {
    let args = vec![
        "--manager-task-add".to_string(),
        "Implement creative mode".to_string(),
        "--manager-task-body".to_string(),
        "Allow temporary formatting drift while feature work is active".to_string(),
        "--manager-task-file".to_string(),
        "src/creative.rs".to_string(),
        "--manager-task-priority".to_string(),
        "250".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(
        parsed.manager_task_add.as_deref(),
        Some("Implement creative mode")
    );
    assert_eq!(
        parsed.manager_task_body.as_deref(),
        Some("Allow temporary formatting drift while feature work is active")
    );
    assert_eq!(parsed.manager_task_file.as_deref(), Some("src/creative.rs"));
    assert_eq!(parsed.manager_task_priority, Some(250));
}

#[test]
fn parser_accepts_broadcast_alias() {
    let args = vec!["--broadcast".to_string(), "pull before merge".to_string()];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.message_body.as_deref(), Some("pull before merge"));
    assert_eq!(parsed.message_target_agent_id, Some(-1));
}

#[test]
fn detect_project_root_ignores_invalid_cli_root_values() {
    let current = env::current_dir().expect("cwd should exist");
    let detected = detect_project_root(Some("1"));
    assert_eq!(detected, current);
}

#[test]
fn refresh_runtime_governance_state_rewrites_stale_governance_before_root_checks() {
    let root = unique_temp_dir("ac_ant_delirium_main_logic_refreshes_stale_governance");
    let plugin = temp_plugin_context(&root, "Bootstrap-Rayon-Lab");
    agent_runtime::sync_agent_registration(&plugin)
        .expect("root registration should succeed")
        .expect("root agent should exist");
    fs::write(plugin.repository_path.join("src/lib.rs"), "pub fn ok() {}\n")
        .expect("source file should exist");
    fs::write(
        plugin.governance_path(),
        format!(
            "{{\n  \"tasks\": [\n    {{\n      \"task_id\": \"STALE000\",\n      \"task_version\": 1,\n      \"task_revision\": \"\",\n      \"task_content_hash\": \"\",\n      \"file_path\": \"{}\",\n      \"violation_count\": 2,\n      \"owner_id\": -1\n    }}\n  ]\n}}\n",
            canonical_bootstrap_plugin_path("Bootstrap-Rayon-Lab")
        ),
    )
    .expect("stale governance should be seeded");

    let mut hierarchy = load_hierarchy_from_path(&plugin.config_path);
    hierarchy.attach_runtime_project_root(&plugin.repository_path.to_string_lossy());
    let config = AuditConfig {
        hierarchy,
        runtime: ac_ant_delirium::structs::RuntimeConfig {
            project_root: plugin.repository_path.to_string_lossy().to_string(),
            mode: GovernanceMode::Strict,
        },
        ..Default::default()
    };

    refresh_runtime_governance_state(&plugin, &config)
        .expect("refresh should rewrite stale governance");

    let report = agent_runtime::render_root_orchestration_violations(&plugin, false)
        .expect("root checks should render");
    assert!(report.is_none());

    let governance =
        fs::read_to_string(plugin.governance_path()).expect("governance should be readable");
    assert!(!governance.contains(&canonical_bootstrap_plugin_path("Bootstrap-Rayon-Lab")));
}
