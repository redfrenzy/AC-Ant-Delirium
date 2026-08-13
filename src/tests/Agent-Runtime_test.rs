use super::test_bridge::*;
use super::*;
use ac_ant_delirium::engine::Swarm_Engine_unit::SwarmEngine;
use ac_ant_delirium::engine_identity;
use ac_ant_delirium::structs::{
    AtomicProjectProjectionWriter, AuditConfig, AuditResult, AuditViolation, CanonicalStateStore,
    CanonicalTaskProjectionRef, GovernanceMode, NewAuditEvent, NumberedPlan, NumberedPlanStep,
    ProjectDatabase, ProjectionMetadata, RuntimeConfig, StorageProject, StorageWorkspace,
    TaskOrchestration, TaskOrchestratorPayload, TodoProjectionContext, TransitionTaskState,
    AUDIT_EVENT_CONTRACT_VERSION,
};

const BOOTSTRAP_TEMPLATES_V1: &str = include_str!("snapshots/v1/bootstrap_templates.tsv");
const JSON_SCHEMA_CONTRACT_V1: &str = include_str!("snapshots/v1/json_schema_contract.tsv");
const AUTOBROWSER_BOOTSTRAP_LAYOUT_V1: &str =
    include_str!("snapshots/v1/autobrowser_bootstrap_layout.tsv");

fn fnv1a_64(content: &str) -> u64 {
    content
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}

fn bootstrap_template_entries() -> Vec<(&'static str, &'static str)> {
    vec![
        ("default_hierarchy.toml", DEFAULT_NEW_PROJECT_HIERARCHY),
        ("connected.json", DEFAULT_CONNECTED_JSON),
        ("context.json", DEFAULT_CONTEXT_JSON),
        ("feedback.json", DEFAULT_FEEDBACK_JSON),
        ("governance_history.json", DEFAULT_GOVERNANCE_HISTORY_JSON),
        ("governance.json", DEFAULT_GOVERNANCE_JSON),
        ("manager_tasks.json", DEFAULT_MANAGER_TASKS_JSON),
        ("messages.json", DEFAULT_MESSAGES_JSON),
        ("submit_queue.json", DEFAULT_SUBMIT_QUEUE_JSON),
        ("task_orchestrator.json", DEFAULT_TASK_ORCHESTRATOR_JSON),
        ("sample_plan.json", DEFAULT_SAMPLE_PLAN_JSON),
        ("transition_task.json", DEFAULT_TRANSITION_TASK_JSON),
        ("project_state.gitignore", DEFAULT_PROJECT_STATE_GITIGNORE),
        ("task_index.txt", DEFAULT_TASK_INDEX_TXT),
        ("scripts_readme.md", DEFAULT_SCRIPTS_README),
        (
            "unlock_governance.template.sh",
            DEFAULT_UNLOCK_GOVERNANCE_TEMPLATE_SH,
        ),
        (
            "lock_governance.template.sh",
            DEFAULT_LOCK_GOVERNANCE_TEMPLATE_SH,
        ),
        (
            "update_plugin_binary.template.sh",
            DEFAULT_UPDATE_PLUGIN_BINARY_TEMPLATE_SH,
        ),
        ("default_rules.toml", DEFAULT_NEW_PROJECT_RULES),
        (
            "ant_delirium_build_guard.rs",
            DEFAULT_NEW_PROJECT_BUILD_GUARD_TEMPLATE,
        ),
        ("githooks_readme.md", DEFAULT_GITHOOKS_README),
        ("merge_request.json", DEFAULT_MERGE_REQUEST_TEMPLATE),
        ("push_request.json", DEFAULT_PUSH_REQUEST_TEMPLATE),
    ]
}

fn render_template_snapshot() -> String {
    let mut actual = String::from("snapshot_version=1\n");
    for (name, content) in bootstrap_template_entries() {
        actual.push_str(&format!(
            "{}\t{}\t{:016x}\n",
            name,
            content.len(),
            fnv1a_64(content)
        ));
    }
    actual
}

fn assert_required_json_fields(name: &str, content: &str, fields: &str) {
    for field in fields.split(',') {
        assert!(
            content.contains(&format!("\"{}\"", field)),
            "{} lost required JSON field '{}'",
            name,
            field
        );
    }
}

fn collect_snapshot_layout(root: &Path, scope: &str, excluded: &[&str]) -> Vec<String> {
    fn visit(root: &Path, current: &Path, scope: &str, excluded: &[&str], out: &mut Vec<String>) {
        let mut entries = fs::read_dir(current)
            .expect("bootstrap snapshot directory should be readable")
            .map(|entry| entry.expect("bootstrap snapshot entry should be readable"))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("snapshot entry should remain under its root")
                .to_string_lossy()
                .replace('\\', "/");
            if excluded.contains(&relative.as_str()) {
                continue;
            }
            let file_type = entry
                .file_type()
                .expect("bootstrap snapshot file type should be readable");
            out.push(format!(
                "{}\t{}\t{}",
                scope,
                if file_type.is_dir() { "D" } else { "F" },
                relative
            ));
            if file_type.is_dir() {
                visit(root, &path, scope, excluded, out);
            }
        }
    }

    let mut entries = Vec::new();
    visit(root, root, scope, excluded, &mut entries);
    entries
}

fn canonical_runtime_plugin_command() -> String {
    format!(
        "./plugins/AC_Rayon-Genetics/{}",
        engine_identity::DEFAULT_PLUGIN_BINARY_NAME
    )
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let dir = env::temp_dir().join(format!("{}_{}", label, nanos));
    fs::create_dir_all(&dir).expect("temporary test directory should be created");
    dir
}

fn write_plugin_config(plugin_dir: &Path, repository_path: &Path) {
    let config_dir = plugin_dir.join("ant_delirium_config");
    fs::create_dir_all(&config_dir).expect("plugin config directory should exist");
    let hierarchy = format!(
        "[hierarchy]\nrepository = \"{}\"\n",
        repository_path.to_string_lossy().replace('\\', "/")
    );
    fs::write(config_dir.join("default_hierarchy.toml"), hierarchy)
        .expect("test hierarchy config should be written");
    fs::write(config_dir.join("default_rules.toml"), "# test rules\n")
        .expect("test rules config should be written");
}

fn temp_plugin_context(
    root: &Path,
    plugin_name: &str,
    repository_rel: &str,
    workspace_rel: &str,
) -> PluginContext {
    let plugin_dir = root.join("plugins").join(plugin_name);
    let repository_path = root.join(repository_rel);
    let workspace_path = root.join(workspace_rel);

    fs::create_dir_all(&repository_path).expect("repository directory should exist");
    fs::create_dir_all(&workspace_path).expect("workspace directory should exist");
    write_plugin_config(&plugin_dir, &repository_path);

    PluginContext {
        plugin_dir: plugin_dir.clone(),
        config_path: plugin_dir.join("ant_delirium_config"),
        repository_path: repository_path.clone(),
        workspace_path: workspace_path.clone(),
        is_worktree: normalize_path(&workspace_path) != normalize_path(&repository_path),
    }
}

fn sample_violation(file_path: &str, rule_id: &str) -> AuditViolation {
    AuditViolation {
        file_path: file_path.to_string(),
        line: 1,
        rule_id: rule_id.to_string(),
        message: format!("violation for {}", file_path),
        snippet: None,
        quick_fix: None,
        detected_dna: String::new(),
        taxonomy_path: String::new(),
    }
}

#[test]
fn bootstrap_templates_match_versioned_snapshot() {
    let actual = render_template_snapshot();
    if actual != BOOTSTRAP_TEMPLATES_V1 {
        println!("BOOTSTRAP_TEMPLATE_SNAPSHOT_BEGIN\n{actual}BOOTSTRAP_TEMPLATE_SNAPSHOT_END");
    }
    assert_eq!(actual, BOOTSTRAP_TEMPLATES_V1);
}

#[test]
fn bootstrap_json_templates_match_versioned_schema_contract() {
    let templates = bootstrap_template_entries();

    for line in JSON_SCHEMA_CONTRACT_V1.lines().skip(1) {
        let (name, fields) = line
            .split_once('\t')
            .expect("schema snapshot rows should contain a tab");
        if name == "materialized_context.json" {
            continue;
        }
        let content = templates
            .iter()
            .find_map(|(template_name, content)| (*template_name == name).then_some(*content))
            .unwrap_or_else(|| panic!("schema contract references unknown template: {name}"));
        assert_required_json_fields(name, content, fields);
    }
}

#[test]
fn autobrowser_bootstrap_layout_matches_versioned_snapshot() {
    let root = unique_temp_dir("ac_ant_delirium_autobrowser_bootstrap_snapshot");
    let project_root = root.join("AutoBrowser");
    let plugin_dir = project_root.join("plugins").join("AutoBrowser");
    fs::create_dir_all(&project_root).expect("AutoBrowser fixture root should be created");
    let plugin = PluginContext {
        plugin_dir: plugin_dir.clone(),
        config_path: plugin_dir.join("ant_delirium_config"),
        repository_path: project_root.clone(),
        workspace_path: project_root.clone(),
        is_worktree: false,
    };

    materialize_project_local_plugin_package(
        &project_root,
        None,
        None,
        Some(&plugin),
        "antdelirium",
    )
    .expect("AutoBrowser bootstrap package should materialize");

    let mut actual = String::from("snapshot_version=1\n");
    for entry in collect_snapshot_layout(&project_root, "project", &["plugins"]) {
        actual.push_str(&entry);
        actual.push('\n');
    }
    for entry in collect_snapshot_layout(&plugin_dir, "plugin", &["bin/guard"]) {
        actual.push_str(&entry);
        actual.push('\n');
    }
    assert_eq!(actual, AUTOBROWSER_BOOTSTRAP_LAYOUT_V1);

    let context = fs::read_to_string(plugin.context_path())
        .expect("materialized context snapshot should be readable");
    let context_contract = JSON_SCHEMA_CONTRACT_V1
        .lines()
        .find_map(|line| line.strip_prefix("materialized_context.json\t"))
        .expect("materialized context schema should be versioned");
    assert_required_json_fields("materialized_context.json", &context, context_contract);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for template in [
            "unlock_governance_files.template.sh",
            "lock_governance_files.template.sh",
            "update_plugin_binary_and_lock.template.sh",
        ] {
            let mode = fs::metadata(plugin_dir.join("scripts").join(template))
                .expect("script template metadata should be readable")
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o111,
                0,
                "generated script templates must remain non-executable"
            );
        }
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn assigns_highest_unowned_governance_task() {
    let mut tasks = vec![
        GovernanceTask {
            task_id: "TASKA001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "a.rs".to_string(),
            violation_count: 10,
            owner_id: -1,
        },
        GovernanceTask {
            task_id: "TASKB001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "b.rs".to_string(),
            violation_count: 50,
            owner_id: -1,
        },
    ];

    let assigned = assign_governance_task(&mut tasks, &[], 7);
    assert_eq!(assigned.as_deref(), Some("b.rs"));
    assert_eq!(tasks[1].owner_id, 7);
}

#[test]
fn governance_refresh_assigns_stable_task_id_and_increments_version_on_change() {
    let root = unique_temp_dir("ac_ant_delirium_governance_versioning");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    sync_agent_registration(&plugin)
        .expect("root should register")
        .expect("root should exist");

    let first = refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![
                sample_violation("src/range.rs", "rule.a"),
                sample_violation("src/range.rs", "rule.b"),
            ],
            logs: Vec::new(),
        },
    )
    .expect("first refresh should work");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].task_version, 1);
    let task_id = first[0].task_id.clone();

    let second = refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("second refresh should work");
    assert_eq!(second[0].task_id, task_id);
    assert_eq!(second[0].task_version, 2);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn governance_refresh_skips_super_block_violations_from_distributed_backlog() {
    let root = unique_temp_dir("ac_ant_delirium_governance_skips_super_block");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    sync_agent_registration(&plugin)
        .expect("root should register")
        .expect("root should exist");

    let tasks = refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![
                sample_violation("build.rs", "hierarchy.super_block.enforced.build"),
                sample_violation("src/lib.rs", "hierarchy.rule.max_nesting"),
            ],
            logs: Vec::new(),
        },
    )
    .expect("refresh should work");

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].file_path, "src/lib.rs");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn governance_refresh_increments_version_when_task_content_hash_changes() {
    let root = unique_temp_dir("ac_ant_delirium_governance_content_hash");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    sync_agent_registration(&plugin)
        .expect("root should register")
        .expect("root should exist");

    let source_path = plugin.repository_path.join("src/range.rs");
    fs::create_dir_all(
        source_path
            .parent()
            .expect("source file parent should exist"),
    )
    .expect("source directory should be created");
    fs::write(&source_path, "fn range() { let a = 1; }\n").expect("initial source should write");

    let first = refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("first refresh should work");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].task_version, 1);
    assert!(!first[0].task_content_hash.is_empty());

    fs::write(&source_path, "fn range() { let a = 2; }\n").expect("updated source should write");

    let second = refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("second refresh should work");
    assert_eq!(second[0].task_id, first[0].task_id);
    assert_eq!(second[0].violation_count, first[0].violation_count);
    assert_ne!(second[0].task_content_hash, first[0].task_content_hash);
    assert_eq!(second[0].task_version, 2);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn governance_refresh_releases_stale_claim_when_task_changes() {
    let root = unique_temp_dir("ac_ant_delirium_governance_releases_stale_claim");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    let agent = sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");

    let source_path = plugin.repository_path.join("src/range.rs");
    fs::create_dir_all(
        source_path
            .parent()
            .expect("source file parent should exist"),
    )
    .expect("source directory should be created");
    fs::write(&source_path, "fn range() { let a = 1; }\n").expect("initial source should write");

    let first = refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("first refresh should work");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            owner_id: agent.internal_id,
            ..first[0].clone()
        }],
    )
    .expect("claim should persist");
    let claimed = assign_task_to_current_agent_if_missing(&plugin)
        .expect("assignment should work")
        .expect("mission should exist");
    assert_eq!(claimed, "src/range.rs");

    fs::write(&source_path, "fn range() { let a = 2; }\n").expect("updated source should write");
    let second = refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("second refresh should work");
    assert_eq!(second[0].owner_id, -1);
    assert_eq!(second[0].task_version, 2);

    let agents = read_connected_agents(&plugin.connected_path()).expect("agents should reload");
    let refreshed_agent = agents
        .iter()
        .find(|entry| entry.internal_id == agent.internal_id)
        .expect("worktree agent should still be present");
    assert!(
        refreshed_agent.assigned_mission.is_empty(),
        "stale governance change should clear the prior claim"
    );
    let raw_login =
        fs::read_to_string(plugin.login_path()).expect("login should update after refresh");
    assert!(raw_login.contains("\"assigned_mission\": \"\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn governance_refresh_preserves_claim_when_task_is_unchanged() {
    let root = unique_temp_dir("ac_ant_delirium_governance_preserves_claim");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    let agent = sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");

    let source_path = plugin.repository_path.join("src/range.rs");
    fs::create_dir_all(
        source_path
            .parent()
            .expect("source file parent should exist"),
    )
    .expect("source directory should be created");
    fs::write(&source_path, "fn range() { let a = 1; }\n").expect("initial source should write");

    let first = refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("first refresh should work");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            owner_id: agent.internal_id,
            ..first[0].clone()
        }],
    )
    .expect("claim should persist");
    let claimed = assign_task_to_current_agent_if_missing(&plugin)
        .expect("assignment should work")
        .expect("mission should exist");
    assert_eq!(claimed, "src/range.rs");

    let second = refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("second refresh should work");
    assert_eq!(second[0].owner_id, agent.internal_id);
    assert_eq!(second[0].task_version, 1);

    let agents = read_connected_agents(&plugin.connected_path()).expect("agents should reload");
    let refreshed_agent = agents
        .iter()
        .find(|entry| entry.internal_id == agent.internal_id)
        .expect("worktree agent should still be present");
    assert_eq!(refreshed_agent.assigned_mission, "src/range.rs");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn governance_refresh_marks_open_submit_record_stale_when_task_changes() {
    let root = unique_temp_dir("ac_ant_delirium_governance_marks_submit_stale");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");

    let source_path = plugin.repository_path.join("src/range.rs");
    fs::create_dir_all(
        source_path
            .parent()
            .expect("source file parent should exist"),
    )
    .expect("source directory should be created");
    fs::write(&source_path, "fn range() { let a = 1; }\n").expect("initial source should write");

    refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("first refresh should work");
    let mission = assign_task_to_current_agent_if_missing(&plugin)
        .expect("assignment should work")
        .expect("mission should exist");
    assert_eq!(mission, "src/range.rs");

    append_submit(
        &plugin,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("WORK1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    fs::write(&source_path, "fn range() { let a = 2; }\n").expect("updated source should write");
    refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("second refresh should work");

    let queue = read_submit_records(&plugin.submit_queue_path()).expect("submit queue should load");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].status, "stale");

    let queue_report =
        render_submit_queue(&plugin_root, false, None, None).expect("queue should render");
    assert!(queue_report.contains("Stale queue records: 1"));
    assert!(queue_report.contains("task_stale=yes"));

    let agent_action = render_next_action(&plugin, false).expect("agent next action should render");
    assert!(agent_action.contains("refresh_governance_task"));

    let root_action =
        render_next_action(&plugin_root, false).expect("root next action should render");
    assert!(root_action.contains("review_stale_submit_tasks"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn governance_history_records_created_updated_and_removed_events() {
    let root = unique_temp_dir("ac_ant_delirium_governance_history");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    sync_agent_registration(&plugin)
        .expect("root should register")
        .expect("root should exist");

    let source_path = plugin.repository_path.join("src/range.rs");
    fs::create_dir_all(
        source_path
            .parent()
            .expect("source file parent should exist"),
    )
    .expect("source directory should be created");
    fs::write(&source_path, "fn range() { let a = 1; }\n").expect("initial source should write");

    refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("first refresh should work");

    fs::write(&source_path, "fn range() { let a = 2; }\n").expect("updated source should write");
    refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: vec![sample_violation("src/range.rs", "rule.a")],
            logs: Vec::new(),
        },
    )
    .expect("second refresh should work");

    refresh_governance_from_audit(
        &plugin,
        &AuditResult {
            violations: Vec::new(),
            logs: Vec::new(),
        },
    )
    .expect("third refresh should work");

    let history =
        read_governance_history(&plugin.governance_history_path()).expect("history should load");
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].change_kind, "created");
    assert_eq!(history[1].change_kind, "updated");
    assert_eq!(history[2].change_kind, "removed");
    assert_eq!(history[1].previous_event_hash, history[0].event_hash);
    assert_eq!(history[2].previous_event_hash, history[1].event_hash);

    let report = render_governance_history(&plugin, false).expect("history report should render");
    assert!(report.contains("Count: 3"));
    assert!(report.contains("kind=created"));
    assert!(report.contains("kind=updated"));
    assert!(report.contains("kind=removed"));

    let swarm_status = render_agents_report(&plugin, false).expect("swarm status should render");
    assert!(swarm_status.contains("Governance history events: 3"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn claims_highest_priority_manager_task_before_common_work() {
    let mut tasks = vec![
        ManagerTask {
            id: "a".to_string(),
            title: "Low".to_string(),
            body: String::new(),
            file_path: "src/a.rs".to_string(),
            priority: 10,
            owner_id: -1,
            status: "open".to_string(),
            created_at: 1,
            mode: None,
            grade: None,
        },
        ManagerTask {
            id: "b".to_string(),
            title: "High".to_string(),
            body: String::new(),
            file_path: "src/b.rs".to_string(),
            priority: 90,
            owner_id: -1,
            status: "open".to_string(),
            created_at: 2,
            mode: None,
            grade: None,
        },
    ];

    let claimed = claim_manager_task_for_agent(&mut tasks, 2).expect("task should be claimed");
    assert_eq!(claimed.id, "b");
    assert_eq!(tasks[1].owner_id, 2);
    assert_eq!(tasks[1].status, "claimed");
}

#[test]
fn swarm_status_truncates_large_text_backlog_preview() {
    let root = unique_temp_dir("ac_ant_delirium_swarm_status_backlog_preview");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );

    sync_agent_registration(&plugin)
        .expect("root registration should succeed")
        .expect("root should exist");

    let mut tasks = Vec::new();
    for idx in 0..15 {
        tasks.push(GovernanceTask {
            task_id: format!("TASK{:04}", idx),
            task_version: 1,
            task_revision: String::new(),
            task_content_hash: String::new(),
            file_path: format!("src/file_{}.rs", idx),
            violation_count: 10 + idx as i64,
            owner_id: -1,
        });
    }
    write_governance_tasks(&plugin.governance_path(), &tasks).expect("governance should write");

    let rendered = render_agents_report(&plugin, false).expect("status should render");
    assert!(rendered.contains("governance_backlog:"));
    assert!(rendered.contains("file=src/file_0.rs"));
    assert!(rendered.contains("file=src/file_11.rs"));
    assert!(!rendered.contains("file=src/file_12.rs"));
    assert!(rendered.contains("... 3 more file(s) hidden"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn done_manager_tasks_are_not_active() {
    assert!(is_manager_task_active(&ManagerTask {
        id: "a".to_string(),
        title: "Open".to_string(),
        body: String::new(),
        file_path: String::new(),
        priority: 1,
        owner_id: -1,
        status: "open".to_string(),
        created_at: 0,
        mode: None,
        grade: None,
    }));
    assert!(!is_manager_task_active(&ManagerTask {
        id: "b".to_string(),
        title: "Done".to_string(),
        body: String::new(),
        file_path: String::new(),
        priority: 1,
        owner_id: -1,
        status: "done".to_string(),
        created_at: 0,
        mode: None,
        grade: None,
    }));
}

#[test]
fn manager_task_commands_create_and_close_priority_work() {
    let root = unique_temp_dir("ac_ant_delirium_manager_task_commands");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );

    let stored = append_manager_task(
        &plugin,
        "Implement creative mode",
        Some("Allow temporary formatting drift while feature work is active"),
        Some("src/creative.rs"),
        250,
    )
    .expect("manager task should be stored");
    assert!(stored.contains("manager task stored:"));

    let rendered = render_manager_tasks(&plugin, false).expect("manager tasks should render");
    assert!(rendered.contains("Implement creative mode"));
    assert!(rendered.contains("priority=250"));

    let tasks = read_manager_tasks(&plugin.manager_tasks_path()).expect("tasks should load");
    assert_eq!(tasks.len(), 1);
    let task_id = tasks[0].id.clone();

    let closed = close_manager_task(&plugin, &task_id).expect("task should close");
    assert!(closed.contains("manager task marked done"));
    let tasks = read_manager_tasks(&plugin.manager_tasks_path()).expect("tasks should reload");
    assert_eq!(tasks[0].status, "done");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_command_materializes_submit_queue_record() {
    let root = unique_temp_dir("ac_ant_delirium_submit_queue_record");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    append_submit(
        &plugin,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    let queue = read_submit_records(&plugin.submit_queue_path()).expect("queue should load");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].status, "open");
    assert_eq!(queue[0].topic, "submit.range");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn newer_submit_supersedes_previous_open_submit_for_same_agent() {
    let root = unique_temp_dir("ac_ant_delirium_submit_queue_supersede");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    append_submit(
        &plugin,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("first submit should persist");
    append_submit(
        &plugin,
        "range.rs v2 ready for integration",
        Some("submit.range.v2"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT2"),
        Some("ROOT2"),
        Some("ROOT2"),
        &canonical_runtime_plugin_command(),
    )
    .expect("second submit should persist");

    let queue = read_submit_records(&plugin.submit_queue_path()).expect("queue should load");
    assert_eq!(queue.len(), 2);
    assert_eq!(
        queue.iter().filter(|entry| entry.status == "open").count(),
        1
    );
    assert_eq!(
        queue
            .iter()
            .filter(|entry| entry.status == "superseded")
            .count(),
        1
    );

    let report =
        render_submit_queue(&plugin, false, None, None).expect("queue report should render");
    assert!(report.contains("Open: 1"));
    assert!(report.contains("Superseded: 1"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_record_captures_active_governance_task_version() {
    let root = unique_temp_dir("ac_ant_delirium_submit_task_version");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 3,
            task_revision: "ROOT3".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: 1,
        }],
    )
    .expect("governance should write");

    append_submit(
        &plugin,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT3"),
        Some("ROOT3"),
        Some("ROOT3"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    let queue = read_submit_records(&plugin.submit_queue_path()).expect("queue should load");
    assert_eq!(queue[0].task_id, "RANGE001");
    assert_eq!(queue[0].task_version, 3);
    assert_eq!(queue[0].task_revision, "ROOT3");
    assert_eq!(queue[0].task_file_path, "src/range.rs");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_record_backfills_legacy_governance_task_metadata() {
    let root = unique_temp_dir("ac_ant_delirium_submit_legacy_task_metadata");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    let agent = sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    let source_path = plugin.repository_path.join("src/range.rs");
    fs::create_dir_all(source_path.parent().expect("source parent should exist"))
        .expect("source parent should be created");
    fs::write(&source_path, "fn range_fixture() {}\n").expect("source file should be written");
    fs::write(
            plugin.governance_path(),
            format!(
                "{{\n  \"tasks\": [\n    {{\n      \"file_path\": \"src/range.rs\",\n      \"violation_count\": 12,\n      \"owner_id\": {}\n    }}\n  ]\n}}\n",
                agent.internal_id
            ),
        )
        .expect("legacy governance should write");

    append_submit(
        &plugin,
        "legacy governance task metadata should be hydrated",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should hydrate legacy metadata");

    let queue = read_submit_records(&plugin.submit_queue_path()).expect("queue should load");
    assert_eq!(queue.len(), 1);
    assert!(!queue[0].task_id.trim().is_empty());
    assert_eq!(queue[0].task_version, 1);
    assert!(!queue[0].task_revision.trim().is_empty());
    assert!(!queue[0].task_content_hash.trim().is_empty());
    assert_eq!(queue[0].task_file_path, "src/range.rs");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn root_can_accept_and_close_submit_queue_record() {
    let root = unique_temp_dir("ac_ant_delirium_submit_queue_status");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");
    append_submit(
        &plugin_a,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    let queue = read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
    let submit_id = queue[0].id.clone();
    update_submit_status(
        &plugin_root,
        &submit_id,
        "accepted",
        Some("looks good"),
        None,
    )
    .expect("root should accept");

    let accepted_status =
        render_agents_report(&plugin_root, true).expect("accepted status should render");
    assert!(accepted_status.contains("\"consumed_submits\": 1"));
    assert!(accepted_status.contains("\"accepted_submit_records\": 1"));
    assert!(accepted_status.contains("\"submit_state\": \"consumed\""));
    assert!(accepted_status.contains("\"submit_queue_status\": \"accepted\""));
    assert!(accepted_status.contains("\"decision_by_agent_id\":"));
    assert!(accepted_status.contains("\"decision_note\": \"looks good\""));

    update_submit_status(&plugin_root, &submit_id, "closed", Some("integrated"), None)
        .expect("root should close");

    let queue = read_submit_records(&plugin_root.submit_queue_path()).expect("queue should reload");
    assert_eq!(queue[0].status, "closed");

    let closed_status =
        render_agents_report(&plugin_root, true).expect("closed status should render");
    assert!(closed_status.contains("\"consumed_submits\": 1"));
    assert!(closed_status.contains("\"closed_submit_records\": 1"));
    assert!(closed_status.contains("\"submit_state\": \"consumed\""));
    assert!(closed_status.contains("\"submit_queue_status\": \"closed\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn accepted_submit_releases_agent_claim_and_suppresses_same_task_until_governance_changes() {
    let root = unique_temp_dir("ac_ant_delirium_submit_release_and_suppress");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    let agent = sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should write");
    let claimed = assign_task_to_current_agent_if_missing(&plugin_a)
        .expect("agent should see the governed task")
        .expect("mission should exist before submit");
    assert_eq!(claimed, "src/range.rs");

    append_submit(
        &plugin_a,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    let queue = read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
    let submit_id = queue[0].id.clone();
    update_submit_status(
        &plugin_root,
        &submit_id,
        "accepted",
        Some("review passed"),
        None,
    )
    .expect("root should accept");

    let tasks =
        read_governance_tasks(&plugin_root.governance_path()).expect("governance should reload");
    assert_eq!(tasks[0].owner_id, -1);

    let agents =
        read_connected_agents(&plugin_root.connected_path()).expect("agents should reload");
    let accepted_agent = agents
        .iter()
        .find(|candidate| candidate.agent_id == agent.agent_id)
        .expect("accepted agent should still exist");
    assert!(accepted_agent.assigned_mission.is_empty());

    let raw_login = fs::read_to_string(plugin_a.login_path()).expect("agent login should update");
    assert!(raw_login.contains("\"assigned_mission\": \"\""));

    let mission = assign_task_to_current_agent_if_missing(&plugin_a)
        .expect("assignment should succeed while task is suppressed");
    assert!(
        mission.is_none(),
        "the same governance task should stay suppressed until governance advances"
    );

    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 2,
            task_revision: "ROOT2".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 8,
            owner_id: -1,
        }],
    )
    .expect("governance should advance");

    let mission = assign_task_to_current_agent_if_missing(&plugin_a)
        .expect("assignment should succeed after governance changes")
        .expect("task should become assignable again");
    assert_eq!(mission, "src/range.rs");
    let next_action = render_next_action(&plugin_a, true).expect("next action should render");
    assert!(next_action.contains("\"submit_queue_status\": \"accepted\""));
    assert!(next_action.contains("\"action_code\": \"continue_assigned_mission\""));
    assert!(next_action.contains("\"active_task_id\": \"RANGE001\""));
    let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
    assert!(whoami.contains("Submit queue status: accepted"));
    assert!(whoami.contains("Next action: continue_assigned_mission"));
    assert!(whoami.contains("State summary: active governed task is `src/range.rs` at version 2"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejected_submit_preserves_agent_claim() {
    let root = unique_temp_dir("ac_ant_delirium_submit_reject_preserves_claim");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    let agent = sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should write");
    let claimed = assign_task_to_current_agent_if_missing(&plugin_a)
        .expect("agent should see the governed task")
        .expect("mission should exist before submit");
    assert_eq!(claimed, "src/range.rs");

    append_submit(
        &plugin_a,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    let queue = read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
    let submit_id = queue[0].id.clone();
    update_submit_status(
        &plugin_root,
        &submit_id,
        "rejected",
        Some("needs more work"),
        None,
    )
    .expect("root should reject");

    let tasks =
        read_governance_tasks(&plugin_root.governance_path()).expect("governance should reload");
    assert_eq!(tasks[0].owner_id, agent.internal_id);

    let agents =
        read_connected_agents(&plugin_root.connected_path()).expect("agents should reload");
    let rejected_agent = agents
        .iter()
        .find(|candidate| candidate.agent_id == agent.agent_id)
        .expect("rejected agent should still exist");
    assert_eq!(rejected_agent.assigned_mission, "src/range.rs");
    let whoami = render_current_agent_identity(&plugin_a, true).expect("whoami should render");
    assert!(whoami.contains("\"submit_queue_status\": \"rejected\""));
    assert!(whoami.contains("\"action_code\": \"revise_rejected_submit\""));
    assert!(whoami.contains("root rejected the latest submit"));
    let whoami_text =
        render_current_agent_identity(&plugin_a, false).expect("text whoami should render");
    assert!(whoami_text.contains(
        "State summary: root rejected the latest submit but the claim is still active, so this workspace should revise the mission and resubmit"
    ));
    let next_action = render_next_action(&plugin_a, true).expect("next action should render");
    assert!(next_action.contains("\"action_code\": \"revise_rejected_submit\""));
    assert!(next_action.contains("root rejected the latest submit"));
    assert!(next_action.contains("\"active_task_id\": \"RANGE001\""));
    assert!(next_action.contains("\"decision_note\": \"needs more work\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_queue_report_summarizes_lifecycle_statuses() {
    let root = unique_temp_dir("ac_ant_delirium_submit_queue_report");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );
    let plugin_b = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-2_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent a should register")
        .expect("agent a should exist");
    sync_agent_registration(&plugin_b)
        .expect("agent b should register")
        .expect("agent b should exist");

    append_submit(
        &plugin_a,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit a should persist");
    append_submit(
        &plugin_b,
        "scope.rs ready for integration",
        Some("submit.scope"),
        Some("hierarchy.submit"),
        Some("ant-agent-2"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit b should persist");

    let queue = read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
    let submit_a = queue[0].id.clone();
    let submit_b = queue[1].id.clone();

    update_submit_status(
        &plugin_root,
        &submit_a,
        "accepted",
        Some("review passed"),
        None,
    )
    .expect("root should accept");
    update_submit_status(
        &plugin_root,
        &submit_b,
        "closed",
        Some("already merged"),
        None,
    )
    .expect("root should close");

    let report =
        render_submit_queue(&plugin_root, false, None, None).expect("queue report should render");
    assert!(report.contains("Count: 2"));
    assert!(report.contains("Open: 0"));
    assert!(report.contains("Accepted: 1"));
    assert!(report.contains("Closed: 1"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_queue_views_support_active_history_and_single_submit() {
    let root = unique_temp_dir("ac_ant_delirium_submit_queue_views");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );
    let plugin_b = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-2_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent a should register")
        .expect("agent a should exist");
    sync_agent_registration(&plugin_b)
        .expect("agent b should register")
        .expect("agent b should exist");

    append_submit(
        &plugin_a,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit a should persist");
    append_submit(
        &plugin_b,
        "scope.rs ready for integration",
        Some("submit.scope"),
        Some("hierarchy.submit"),
        Some("ant-agent-2"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit b should persist");

    let queue = read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
    let submit_a = queue[0].id.clone();
    let submit_b = queue[1].id.clone();
    update_submit_status(
        &plugin_root,
        &submit_a,
        "accepted",
        Some("review passed"),
        None,
    )
    .expect("root should accept");

    let active = render_submit_queue(&plugin_root, false, Some("active"), None)
        .expect("active queue should render");
    assert!(active.contains("View: active"));
    assert!(active.contains("Count: 1"));
    assert!(active.contains(&submit_b));
    assert!(!active.contains(&submit_a));

    let history = render_submit_queue(&plugin_root, false, Some("history"), None)
        .expect("history queue should render");
    assert!(history.contains("View: history"));
    assert!(history.contains("Count: 1"));
    assert!(history.contains(&submit_a));
    assert!(!history.contains(&submit_b));

    let single = render_submit_queue(&plugin_root, false, None, Some(&submit_b))
        .expect("single submit should render");
    assert!(single.contains("Submit ID: "));
    assert!(single.contains(&submit_b));
    assert!(!single.contains(&submit_a));

    let single_history = render_submit_queue(&plugin_root, false, None, Some(&submit_a))
        .expect("single history submit should render");
    assert!(single_history.contains("review passed"));
    assert!(single_history.contains("decided_by="));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_queue_marks_stale_task_versions_in_views() {
    let root = unique_temp_dir("ac_ant_delirium_submit_queue_task_stale_view");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    let agent = sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should write");
    assign_task_to_current_agent_if_missing(&plugin_a)
        .expect("assignment should succeed")
        .expect("mission should exist");

    append_submit(
        &plugin_a,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 2,
            task_revision: "ROOT2".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 8,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should advance");

    let queue = read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
    let submit_id = queue[0].id.clone();

    let single = render_submit_queue(&plugin_root, false, None, Some(&submit_id))
        .expect("single submit should render");
    assert!(single.contains("task_stale=yes"));
    assert!(single.contains("stale_reason=version_changed:1->2"));
    assert!(single.contains("Stale task submits: 1"));

    let single_json = render_submit_queue(&plugin_root, true, None, Some(&submit_id))
        .expect("single submit json should render");
    assert!(single_json.contains("\"task_stale\": true"));
    assert!(single_json.contains("\"stale_reason\": \"version_changed:1->2"));

    let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
    assert!(whoami.contains("Task stale: yes"));
    assert!(whoami.contains("Stale reason: version_changed:1->2"));

    let whoami_json =
        render_current_agent_identity(&plugin_a, true).expect("whoami json should render");
    assert!(whoami_json.contains("\"task_stale\": true"));
    assert!(whoami_json.contains("\"stale_reason\": \"version_changed:1->2"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn closed_manager_task_releases_agent_back_to_governance_work() {
    let root = unique_temp_dir("ac_ant_delirium_manager_task_release");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    let registered = sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 50,
            owner_id: -1,
        }],
    )
    .expect("governance should be written");
    write_manager_tasks(
        &plugin.manager_tasks_path(),
        &[ManagerTask {
            id: "CREATIVE1".to_string(),
            title: "Implement creative mode".to_string(),
            body: "temporary creative backlog".to_string(),
            file_path: "src/creative.rs".to_string(),
            priority: 250,
            owner_id: -1,
            status: "open".to_string(),
            created_at: unix_timestamp(),
            mode: None,
            grade: None,
        }],
    )
    .expect("manager task should be written");

    let mission = assign_task_to_current_agent_if_missing(&plugin)
        .expect("assignment should work")
        .expect("manager mission should exist");
    assert!(mission.contains("manager:Implement creative mode"));

    close_manager_task(&plugin, "CREATIVE1").expect("manager task should close");
    let mission = assign_task_to_current_agent_if_missing(&plugin)
        .expect("assignment should work again")
        .expect("governance mission should exist");
    assert_eq!(mission, "src/range.rs");

    let tasks = read_governance_tasks(&plugin.governance_path()).expect("governance should reload");
    assert_eq!(tasks[0].owner_id, registered.internal_id);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swarm_status_and_identity_highlight_manager_override() {
    let root = unique_temp_dir("ac_ant_delirium_manager_override_status");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 50,
            owner_id: 1,
        }],
    )
    .expect("governance should be written");
    write_manager_tasks(
        &plugin_root.manager_tasks_path(),
        &[ManagerTask {
            id: "CREATIVE1".to_string(),
            title: "Implement creative mode".to_string(),
            body: "temporary creative backlog".to_string(),
            file_path: "src/creative.rs".to_string(),
            priority: 250,
            owner_id: 1,
            status: "claimed".to_string(),
            created_at: unix_timestamp(),
            mode: None,
            grade: None,
        }],
    )
    .expect("manager tasks should be written");

    let status = render_agents_report(&plugin_root, false).expect("status should render");
    assert!(status.contains("manager_override=yes"));
    assert!(status.contains("priority_backlog:"));
    assert!(status.contains("governance_backlog:"));

    let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
    assert!(whoami.contains("Manager override: yes"));
    assert!(whoami.contains("Active manager tasks: 1"));
    assert!(whoami.contains("Queued violation files: 1"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn parses_connected_agents_from_existing_json() {
    let raw = r#"{
  "agents": [
    {
      "internal_id": 2,
      "worktree_path": "/tmp/ws",
      "assigned_mission": "src/lib.rs",
      "last_ping": 10
    }
  ]
}"#;

    let path = env::temp_dir().join("ac_ant_delirium_connected_test.json");
    fs::write(&path, raw).expect("connected test fixture should be written");
    let agents = read_connected_agents(&path).expect("agents should parse");
    assert_eq!(
        agents,
        vec![ConnectedAgent {
            internal_id: 2,
            agent_id: agent_id_for_workspace_path(Path::new("/tmp/ws")),
            branch: String::new(),
            base_revision: String::new(),
            workspace_revision: String::new(),
            repository_revision: String::new(),
            worktree_path: "/tmp/ws".to_string(),
            login_path: "/tmp/ws/AC_Ant-Delirium_props/login/login.json".to_string(),
            assigned_mission: "src/lib.rs".to_string(),
            last_ping: 10,
        }]
    );
    let _ = fs::remove_file(path);
}

#[test]
fn plugin_context_matches_same_project_name_as_worktree() {
    let project_root = Path::new("/tmp/AC_Rayon-Genetics");
    let plugin_dir = Path::new("/tmp/plugins/AC_Rayon-Genetics");
    let repository = Path::new("/srv/main/AC_Rayon-Genetics");
    assert!(match_plugin_context(project_root, plugin_dir, repository) >= 90);
}

#[test]
fn directed_messages_are_shared_between_registered_agents() {
    let root = unique_temp_dir("ac_ant_delirium_agent_messages");
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );
    let plugin_b = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-2_RAYON",
    );

    let agent_a = sync_agent_registration(&plugin_a)
        .expect("agent a should register")
        .expect("worktree registration should create agent a");
    let agent_b = sync_agent_registration(&plugin_b)
        .expect("agent b should register")
        .expect("worktree registration should create agent b");
    assert_ne!(agent_a.internal_id, agent_b.internal_id);

    append_message(
        &plugin_a,
        Some(agent_b.internal_id),
        None,
        None,
        None,
        Some("range.rs"),
        None,
        None,
        None,
        None,
        None,
        "sync ownership for range.rs",
    )
    .expect("directed message should be stored");

    let inbox_b = render_current_inbox(&plugin_b).expect("agent b inbox should render");
    assert!(inbox_b.contains("sync ownership for range.rs"));
    assert!(inbox_b.contains(plugin_a.workspace_path.to_string_lossy().as_ref()));
    assert!(inbox_b.contains(&format!("from_agent={}", agent_a.agent_id)));
    assert!(inbox_b.contains(&format!("to_agent={}", agent_b.agent_id)));

    let inbox_a = render_current_inbox(&plugin_a).expect("agent a inbox should render");
    assert!(
        !inbox_a.contains("sync ownership for range.rs"),
        "directed message should not leak back to the sender inbox"
    );

    let raw_messages =
        fs::read_to_string(plugin_a.messages_path()).expect("messages should persist to disk");
    assert!(raw_messages.contains(&format!("\"sender_internal_id\": {}", agent_a.internal_id)));
    assert!(raw_messages.contains(&format!("\"sender_agent_id\": \"{}\"", agent_a.agent_id)));
    assert!(raw_messages.contains("\"login_path\":"));
    assert!(raw_messages.contains("\"repository_path\":"));
    assert!(raw_messages.contains(plugin_a.login_path().to_string_lossy().as_ref()));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn canonical_submit_records_public_message_and_feedback() {
    let root = unique_temp_dir("ac_ant_delirium_submit_records_evidence");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    let agent = sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 4,
            task_revision: "ROOT4".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should write");

    append_submit(
        &plugin,
        "range.rs fix is ready for review",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("BASE1"),
        Some("WORK2"),
        Some("ROOT3"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should store both artifacts");

    let raw_messages = fs::read_to_string(plugin.messages_path()).expect("messages should exist");
    let raw_feedback = fs::read_to_string(plugin.feedback_path()).expect("feedback should exist");
    assert!(raw_messages.contains("\"operation_kind\": \"submit\""));
    assert!(raw_messages.contains("\"target_agent_id\": \"broadcast\""));
    assert!(raw_messages.contains(&format!("\"sender_agent_id\": \"{}\"", agent.agent_id)));
    assert!(raw_messages.contains("\"task_id\": \"RANGE001\""));
    assert!(raw_messages.contains("\"task_version\": 4"));
    assert!(raw_messages.contains("\"task_revision\": \"ROOT4\""));
    assert!(raw_feedback.contains("\"operation_kind\": \"submit\""));
    assert!(raw_feedback.contains("\"taxonomy_path\": \"hierarchy.submit\""));
    assert!(raw_feedback.contains(&format!("\"sender_agent_id\": \"{}\"", agent.agent_id)));
    assert!(raw_feedback.contains("\"task_id\": \"RANGE001\""));
    assert!(raw_feedback.contains("\"task_version\": 4"));
    assert!(raw_feedback.contains("\"task_revision\": \"ROOT4\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn next_action_guides_agent_toward_assigned_mission() {
    let root = unique_temp_dir("ac_ant_delirium_next_action_agent");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: 1,
        }],
    )
    .expect("governance should write");
    let report = render_next_action(&plugin, false).expect("next action should render");
    assert!(report.contains("resume_owned_governance"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn next_action_guides_root_toward_submit_triage() {
    let root = unique_temp_dir("ac_ant_delirium_next_action_root");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");
    append_submit(
        &plugin_a,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    let report = render_next_action(&plugin_root, false).expect("next action should render");
    assert!(report.contains("triage_active_submits"));
    assert!(report.contains("--submits-active"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn next_action_guides_root_toward_stale_task_submit_review() {
    let root = unique_temp_dir("ac_ant_delirium_next_action_root_task_stale");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    let agent = sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should write");

    append_submit(
        &plugin_a,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 2,
            task_revision: "ROOT2".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 8,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should advance");

    let report = render_next_action(&plugin_root, false).expect("next action should render");
    assert!(report.contains("review_stale_submit_tasks"));

    let status = render_agents_report(&plugin_root, false).expect("status should render");
    assert!(status.contains("Blocked task-version submits: 1"));

    let status_json = render_agents_report(&plugin_root, true).expect("json status should render");
    assert!(status_json.contains("\"blocked_task_version_submits\": 1"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn identity_reports_operational_and_pedagogical_feedback_counts() {
    let root = unique_temp_dir("ac_ant_delirium_feedback_telemetry");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    append_feedback(
        &plugin,
        "explain branch meaning",
        Some("hierarchy.sync.branch"),
        None,
        None,
        None,
        None,
        None,
        &canonical_runtime_plugin_command(),
    )
    .expect("pedagogical feedback should persist");
    append_feedback(
        &plugin,
        "sync complete",
        Some("hierarchy.sync"),
        Some("sync"),
        Some("ant-agent-1"),
        Some("BASE1"),
        Some("WORK1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("operational feedback should persist");

    let whoami = render_current_agent_identity(&plugin, false).expect("whoami should render");
    assert!(whoami.contains("Operational feedback: 1"));
    assert!(whoami.contains("Pedagogical feedback: 1"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_root_registers_as_agent_zero_and_sees_global_scope() {
    let root = unique_temp_dir("ac_ant_delirium_root_agent_zero");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );

    let registered = sync_agent_registration(&plugin)
        .expect("root registration should succeed")
        .expect("root should be represented as an agent");
    assert_eq!(registered.internal_id, 0);
    assert_eq!(registered.assigned_mission, "global");
    assert!(
        plugin.login_path().is_file(),
        "root login.json should be generated"
    );
    let raw_login = fs::read_to_string(plugin.login_path()).expect("root login should be readable");
    assert!(raw_login.contains("\"internal_id\": 0"));
    assert!(raw_login.contains(plugin.repository_path.to_string_lossy().as_ref()));

    let agents = read_connected_agents(&plugin.connected_path()).expect("agents should load");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].internal_id, 0);
    assert_eq!(agents[0].login_path, plugin.login_path().to_string_lossy());

    let audit_result = AuditResult {
        violations: vec![
            sample_violation("src/range.rs", "hierarchy.identity.dna_suffix"),
            sample_violation("src/string.rs", "hierarchy.structural.logical_density"),
        ],
        logs: vec!["global audit".to_string()],
    };

    refresh_governance_from_audit(&plugin, &audit_result).expect("governance should refresh");
    let whoami = render_current_agent_identity(&plugin, false).expect("whoami should render");
    assert!(whoami.contains(
        "State summary: root is coordinating 2 queued governance file(s) and waiting for agent claims"
    ));

    let mission = assign_task_to_current_agent_if_missing(&plugin)
        .expect("root assignment should succeed")
        .expect("root should receive a global mission");
    assert_eq!(mission, "global");

    let isolated =
        isolate_audit_to_current_agent(&plugin, &audit_result).expect("root keeps full audit");
    assert_eq!(isolated.violations.len(), 2);
    assert!(isolated
        .logs
        .iter()
        .any(|entry| entry.contains("full audit surface")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn root_orchestration_checks_report_invalid_governance_owner() {
    let root = unique_temp_dir("ac_ant_delirium_root_checks_governance_owner");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );

    sync_agent_registration(&plugin)
        .expect("root registration should succeed")
        .expect("root should exist");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "LIB00001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/lib.rs".to_string(),
            violation_count: 3,
            owner_id: 99,
        }],
    )
    .expect("governance fixture should be written");

    let report = render_root_orchestration_violations(&plugin, false)
        .expect("root orchestration report should render")
        .expect("root orchestration report should fail");
    assert!(report.contains("invalid_governance_owner"));
    assert!(report.contains("unknown agent 99"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn root_orchestration_checks_report_invalid_sync_message_metadata() {
    let root = unique_temp_dir("ac_ant_delirium_root_checks_invalid_sync_message");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    let agent_a = sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");

    write_entry_collection(
        &plugin_root.messages_path(),
        "messages",
        &[JsonEntry {
            fields: vec![
                ("target_internal_id".to_string(), JsonValue::Number(-1)),
                (
                    "sender_internal_id".to_string(),
                    JsonValue::Number(agent_a.internal_id),
                ),
                (
                    "target_agent_id".to_string(),
                    JsonValue::String("broadcast".to_string()),
                ),
                (
                    "sender_agent_id".to_string(),
                    JsonValue::String(agent_a.agent_id.clone()),
                ),
                (
                    "body".to_string(),
                    JsonValue::String("sync entry missing metadata".to_string()),
                ),
                (
                    "visibility".to_string(),
                    JsonValue::String("public".to_string()),
                ),
                (
                    "workspace_path".to_string(),
                    JsonValue::String(plugin_a.workspace_path.to_string_lossy().to_string()),
                ),
                (
                    "login_path".to_string(),
                    JsonValue::String(plugin_a.login_path().to_string_lossy().to_string()),
                ),
                (
                    "repository_path".to_string(),
                    JsonValue::String(plugin_a.repository_path.to_string_lossy().to_string()),
                ),
                (
                    "execution_context".to_string(),
                    JsonValue::String("git worktree".to_string()),
                ),
                (
                    "assigned_mission".to_string(),
                    JsonValue::String(agent_a.assigned_mission.clone()),
                ),
                (
                    "operation_kind".to_string(),
                    JsonValue::String("sync".to_string()),
                ),
                (
                    "created_at".to_string(),
                    JsonValue::Number(unix_timestamp()),
                ),
            ],
        }],
    )
    .expect("messages fixture should be written");

    let report = render_root_orchestration_violations(&plugin_root, false)
        .expect("root orchestration report should render")
        .expect("root orchestration report should fail");
    assert!(report.contains("invalid_message_entry"));
    assert!(report.contains("incomplete sync metadata"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn root_orchestration_checks_report_missing_submit_message() {
    let root = unique_temp_dir("ac_ant_delirium_root_checks_missing_submit_message");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");

    append_feedback(
        &plugin_a,
        "submit without public message",
        Some("hierarchy.submit"),
        Some("submit"),
        Some("ant-agent-1"),
        Some("BASE1"),
        Some("WORK2"),
        Some("ROOT3"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit feedback should be stored");

    let report = render_root_orchestration_violations(&plugin_root, false)
        .expect("root orchestration report should render")
        .expect("root orchestration report should fail");
    assert!(report.contains("missing_submit_message"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn root_orchestration_accepts_canonical_submit_pair() {
    let root = unique_temp_dir("ac_ant_delirium_root_checks_canonical_submit");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");

    append_submit(
        &plugin_a,
        "range.rs fix is ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("canonical submit should store a consistent pair");

    let report = render_root_orchestration_violations(&plugin_root, false)
        .expect("root orchestration should evaluate");
    assert!(
        report.is_none(),
        "canonical submit should not leave missing submit evidence findings"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swarm_status_and_identity_render_ready_submit_state() {
    let root = unique_temp_dir("ac_ant_delirium_submit_state_ready");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");

    append_submit(
        &plugin_a,
        "range.rs is ready",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    let status = render_agents_report(&plugin_root, false).expect("status should render");
    assert!(status.contains("Ready submits: 1"));
    assert!(status.contains("Open submit records: 1"));
    assert!(status.contains("submit_state=ready_for_root"));
    assert!(status.contains("submit_queue_status=open"));
    let root_whoami =
        render_current_agent_identity(&plugin_root, false).expect("root whoami should render");
    assert!(root_whoami.contains(
        "State summary: root has open submit evidence waiting for triage and should decide accept/reject/close"
    ));

    let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
    assert!(whoami.contains(
        "State summary: latest submit is open and this workspace must wait for root triage instead of resending it"
    ));
    assert!(whoami.contains("Submit state: ready_for_root"));
    assert!(whoami.contains("Submit queue status: open"));
    assert!(whoami.contains("Next action: wait_root_triage"));
    assert!(whoami.contains("do not resend it yet"));
    assert!(whoami.contains("Submit message: yes"));
    assert!(whoami.contains("Submit feedback: yes"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swarm_status_and_root_checks_render_blocked_submit_state() {
    let root = unique_temp_dir("ac_ant_delirium_submit_state_blocked");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");

    append_feedback(
        &plugin_root,
        "root metadata updated",
        Some("hierarchy.sync"),
        Some("sync"),
        Some("main"),
        Some("ROOT2"),
        Some("ROOT2"),
        Some("ROOT2"),
        &canonical_runtime_plugin_command(),
    )
    .expect("root sync should persist");
    append_submit(
        &plugin_a,
        "range.rs asked for integration on stale base",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("WORK1"),
        Some("ROOT2"),
        &canonical_runtime_plugin_command(),
    )
    .expect("stale submit should still persist");

    let status_json = render_agents_report(&plugin_root, true).expect("json status should render");
    assert!(status_json.contains("\"blocked_submits\": 1"));
    assert!(status_json.contains("\"open_submit_records\": 1"));
    assert!(status_json.contains("\"submit_state\": \"blocked_rebase\""));
    assert!(status_json.contains("\"submit_queue_status\": \"open\""));

    let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
    assert!(whoami.contains(
        "State summary: sync metadata says this workspace must refresh branch/revision context before submit"
    ));
    assert!(whoami.contains("Submit state: blocked_rebase"));
    assert!(whoami.contains("Submit queue status: open"));

    let report = render_root_orchestration_violations(&plugin_root, false)
        .expect("root report should render")
        .expect("blocked submit should fail root checks");
    assert!(report.contains("submit_requires_rebase"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn root_checks_ignore_consumed_submit_history() {
    let root = unique_temp_dir("ac_ant_delirium_submit_consumed_root_checks");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");

    append_feedback(
        &plugin_root,
        "root metadata updated",
        Some("hierarchy.sync"),
        Some("sync"),
        Some("main"),
        Some("ROOT2"),
        Some("ROOT2"),
        Some("ROOT2"),
        &canonical_runtime_plugin_command(),
    )
    .expect("root sync should persist");
    append_submit(
        &plugin_a,
        "range.rs asked for integration on stale base",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("WORK1"),
        Some("ROOT2"),
        &canonical_runtime_plugin_command(),
    )
    .expect("stale submit should persist");

    let queue = read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
    let submit_id = queue[0].id.clone();
    update_submit_status(
        &plugin_root,
        &submit_id,
        "accepted",
        Some("accepted despite old state"),
        None,
    )
    .expect("root should accept");

    let report = render_root_orchestration_violations(&plugin_root, false)
        .expect("root report should render");
    assert!(
        report.is_none(),
        "accepted submit should be treated as consumed, not as an active rebase conflict"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn root_checks_report_submit_task_stale_when_governance_version_advances() {
    let root = unique_temp_dir("ac_ant_delirium_submit_task_stale");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: 1,
        }],
    )
    .expect("governance should write");

    append_submit(
        &plugin_a,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("ROOT1"),
        Some("ROOT1"),
        &canonical_runtime_plugin_command(),
    )
    .expect("submit should persist");

    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 2,
            task_revision: "ROOT2".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 8,
            owner_id: 1,
        }],
    )
    .expect("governance should advance");

    let report = render_root_orchestration_violations(&plugin_root, false)
        .expect("root report should render")
        .expect("stale task submit should fail root checks");
    assert!(report.contains("submit_task_stale"));

    let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
    assert!(whoami.contains(
        "State summary: latest submit metadata is stale and this workspace must refresh governance state before resubmitting"
    ));
    assert!(whoami.contains("Submit state: blocked_task_version"));
    assert!(whoami.contains("Task stale: yes"));
    assert!(whoami.contains("Next action: refresh_governance_task"));
    assert!(whoami.contains("confirm the new task metadata"));

    let next = render_next_action(&plugin_a, false).expect("next action should render");
    assert!(next.contains("refresh_governance_task"));
    assert!(next.contains("confirm the new task metadata"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_rejects_stale_task_metadata_before_recording() {
    let root = unique_temp_dir("ac_ant_delirium_submit_rejects_stale_task_metadata");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    let agent = sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should write");
    assign_task_to_current_agent_if_missing(&plugin)
        .expect("assignment should succeed")
        .expect("mission should exist");

    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 2,
            task_revision: "ROOT2".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 8,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should advance");

    let err = append_submit(
        &plugin,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        Some("ant-agent-1"),
        Some("ROOT2"),
        Some("ROOT2"),
        Some("ROOT2"),
        &canonical_runtime_plugin_command(),
    )
    .expect_err("submit should reject stale task metadata");
    assert!(err.to_string().contains("submit task metadata is stale"));
    assert!(err.to_string().contains("refresh the governance task"));

    let queue = read_submit_records(&plugin.submit_queue_path()).expect("queue should load");
    assert!(queue.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_feedback_rejects_stale_task_metadata_before_recording() {
    let root = unique_temp_dir("ac_ant_delirium_submit_feedback_rejects_stale_task_metadata");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    let agent = sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should write");
    assign_task_to_current_agent_if_missing(&plugin)
        .expect("assignment should succeed")
        .expect("mission should exist");

    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 2,
            task_revision: "ROOT2".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 8,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should advance");

    let err = append_feedback(
        &plugin,
        "range.rs submit feedback should fail",
        Some("hierarchy.submit"),
        Some("submit"),
        Some("ant-agent-1"),
        Some("ROOT2"),
        Some("ROOT2"),
        Some("ROOT2"),
        &canonical_runtime_plugin_command(),
    )
    .expect_err("submit feedback should reject stale task metadata");
    assert!(err
        .to_string()
        .contains("feedback submit task metadata is stale"));

    let raw_feedback =
        fs::read_to_string(plugin.feedback_path()).expect("feedback file should still exist");
    assert!(!raw_feedback.contains("range.rs submit feedback should fail"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn root_orchestration_checks_report_stale_agent_with_active_mission() {
    let root = unique_temp_dir("ac_ant_delirium_root_checks_stale_active_agent");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let stale_workspace = root.join("Ant-Delirium-Agent-1_RAYON");
    let stale_login = stale_workspace.join("AC_Ant-Delirium_props/login/login.json");
    let root_login = expected_login_path_from_workspace_path(plugin.repository_path.as_path());
    fs::create_dir_all(stale_workspace.join("AC_Ant-Delirium_props/login"))
        .expect("stale login dir should exist");
    fs::create_dir_all(root_login.parent().expect("root login parent should exist"))
        .expect("root login dir should exist");
    fs::write(
        &root_login,
        format!(
            "{{\"agent_id\":\"ROOT0001\",\"repository_path\":\"{}\",\"workspace_path\":\"{}\"}}",
            plugin.repository_path.to_string_lossy(),
            plugin.repository_path.to_string_lossy()
        ),
    )
    .expect("root login should be written");
    fs::write(
        &stale_login,
        format!(
            "{{\"agent_id\":\"STALE123\",\"repository_path\":\"{}\",\"workspace_path\":\"{}\"}}",
            plugin.repository_path.to_string_lossy(),
            stale_workspace.to_string_lossy()
        ),
    )
    .expect("stale login should be written");

    write_connected_agents(
        &plugin.connected_path(),
        &[
            ConnectedAgent {
                internal_id: 0,
                agent_id: "ROOT0001".to_string(),
                branch: "main".to_string(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: plugin.repository_path.to_string_lossy().to_string(),
                login_path: expected_login_path_from_workspace_path(
                    plugin.repository_path.as_path(),
                )
                .to_string_lossy()
                .to_string(),
                assigned_mission: "global".to_string(),
                last_ping: unix_timestamp(),
            },
            ConnectedAgent {
                internal_id: 1,
                agent_id: "STALE123".to_string(),
                branch: "ant-agent-1".to_string(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: stale_workspace.to_string_lossy().to_string(),
                login_path: stale_login.to_string_lossy().to_string(),
                assigned_mission: "src/lib.rs".to_string(),
                last_ping: unix_timestamp() - 7200,
            },
        ],
    )
    .expect("agents fixture should be written");

    let report = render_root_orchestration_violations(&plugin, false)
        .expect("root orchestration report should render")
        .expect("root orchestration report should fail");
    assert!(report.contains("stale_agent_with_active_mission"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn release_stale_agent_claims_returns_owned_work_to_backlog() {
    let root = unique_temp_dir("ac_ant_delirium_release_stale_claims");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let stale_workspace = root.join("Ant-Delirium-Agent-1_RAYON");
    let stale_login = stale_workspace.join("AC_Ant-Delirium_props/login/login.json");
    let root_login = expected_login_path_from_workspace_path(plugin.repository_path.as_path());
    fs::create_dir_all(stale_workspace.join("AC_Ant-Delirium_props/login"))
        .expect("stale login dir should exist");
    fs::create_dir_all(root_login.parent().expect("root login parent should exist"))
        .expect("root login dir should exist");
    fs::write(
        &root_login,
        format!(
            "{{\"agent_id\":\"ROOT0001\",\"repository_path\":\"{}\",\"workspace_path\":\"{}\"}}",
            plugin.repository_path.to_string_lossy(),
            plugin.repository_path.to_string_lossy()
        ),
    )
    .expect("root login should be written");
    fs::write(
            &stale_login,
            format!(
                "{{\"agent_id\":\"STALE123\",\"repository_path\":\"{}\",\"workspace_path\":\"{}\",\"assigned_mission\":\"src/lib.rs\"}}",
                plugin.repository_path.to_string_lossy(),
                stale_workspace.to_string_lossy()
            ),
        )
        .expect("stale login should be written");

    write_connected_agents(
        &plugin.connected_path(),
        &[
            ConnectedAgent {
                internal_id: 0,
                agent_id: "ROOT0001".to_string(),
                branch: "main".to_string(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: plugin.repository_path.to_string_lossy().to_string(),
                login_path: root_login.to_string_lossy().to_string(),
                assigned_mission: "global".to_string(),
                last_ping: unix_timestamp(),
            },
            ConnectedAgent {
                internal_id: 1,
                agent_id: "STALE123".to_string(),
                branch: "ant-agent-1".to_string(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: stale_workspace.to_string_lossy().to_string(),
                login_path: stale_login.to_string_lossy().to_string(),
                assigned_mission: "src/lib.rs".to_string(),
                last_ping: unix_timestamp() - 7200,
            },
        ],
    )
    .expect("agents fixture should be written");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "LIB00001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: "HASH1".to_string(),
            file_path: "src/lib.rs".to_string(),
            violation_count: 3,
            owner_id: 1,
        }],
    )
    .expect("governance fixture should be written");
    write_manager_tasks(
        &plugin.manager_tasks_path(),
        &[ManagerTask {
            id: "MANAGER1".to_string(),
            title: "Hotfix".to_string(),
            body: "urgent".to_string(),
            file_path: "src/lib.rs".to_string(),
            priority: 100,
            owner_id: 1,
            status: "claimed".to_string(),
            created_at: unix_timestamp(),
            mode: None,
            grade: None,
        }],
    )
    .expect("manager fixture should be written");

    let report = release_stale_agent_claims(&plugin, false).expect("stale claims should release");
    assert!(report.contains("Released agents: 1"));
    assert!(report.contains("Released governance tasks: 1"));
    assert!(report.contains("Released manager tasks: 1"));

    let agents = read_connected_agents(&plugin.connected_path()).expect("agents should load");
    let stale_agent = agents
        .iter()
        .find(|agent| agent.agent_id == "STALE123")
        .expect("stale agent should remain registered");
    assert!(stale_agent.assigned_mission.is_empty());

    let tasks = read_governance_tasks(&plugin.governance_path()).expect("tasks should load");
    assert_eq!(tasks[0].owner_id, -1);
    let manager_tasks =
        read_manager_tasks(&plugin.manager_tasks_path()).expect("manager tasks should load");
    assert_eq!(manager_tasks[0].owner_id, -1);

    let raw_login = fs::read_to_string(&stale_login).expect("stale login should exist");
    assert!(raw_login.contains("\"assigned_mission\": \"\""));

    let checks =
        render_root_orchestration_violations(&plugin, true).expect("root checks should render");
    assert!(checks.is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn prune_stale_agents_removes_only_idle_stale_entries() {
    let root = unique_temp_dir("ac_ant_delirium_prune_stale_agents");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let active_workspace = root.join("Ant-Delirium-Agent-1_RAYON");
    let idle_workspace = root.join(engine_identity::DEFAULT_PLUGIN_BINARY_NAME);
    let active_login = active_workspace.join("AC_Ant-Delirium_props/login/login.json");
    let idle_login = idle_workspace.join("AC_Ant-Delirium_props/login/login.json");
    let root_login = expected_login_path_from_workspace_path(plugin.repository_path.as_path());
    fs::create_dir_all(
        active_login
            .parent()
            .expect("active login parent should exist"),
    )
    .expect("active login dir should exist");
    fs::create_dir_all(idle_login.parent().expect("idle login parent should exist"))
        .expect("idle login dir should exist");
    fs::create_dir_all(root_login.parent().expect("root login parent should exist"))
        .expect("root login dir should exist");
    for (path, workspace, agent_id) in [
        (&root_login, plugin.repository_path.as_path(), "ROOT0001"),
        (&active_login, active_workspace.as_path(), "ACTIVE01"),
        (&idle_login, idle_workspace.as_path(), "IDLE0001"),
    ] {
        fs::write(
            path,
            format!(
                "{{\"agent_id\":\"{}\",\"repository_path\":\"{}\",\"workspace_path\":\"{}\"}}",
                agent_id,
                plugin.repository_path.to_string_lossy(),
                workspace.to_string_lossy()
            ),
        )
        .expect("login fixture should be written");
    }

    write_connected_agents(
        &plugin.connected_path(),
        &[
            ConnectedAgent {
                internal_id: 0,
                agent_id: "ROOT0001".to_string(),
                branch: "main".to_string(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: plugin.repository_path.to_string_lossy().to_string(),
                login_path: root_login.to_string_lossy().to_string(),
                assigned_mission: "global".to_string(),
                last_ping: unix_timestamp(),
            },
            ConnectedAgent {
                internal_id: 1,
                agent_id: "ACTIVE01".to_string(),
                branch: "ant-agent-1".to_string(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: active_workspace.to_string_lossy().to_string(),
                login_path: active_login.to_string_lossy().to_string(),
                assigned_mission: "src/lib.rs".to_string(),
                last_ping: unix_timestamp() - 7200,
            },
            ConnectedAgent {
                internal_id: 2,
                agent_id: "IDLE0001".to_string(),
                branch: String::new(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: idle_workspace.to_string_lossy().to_string(),
                login_path: idle_login.to_string_lossy().to_string(),
                assigned_mission: String::new(),
                last_ping: unix_timestamp() - 7200,
            },
        ],
    )
    .expect("agents fixture should be written");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "LIB00001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: "HASH1".to_string(),
            file_path: "src/lib.rs".to_string(),
            violation_count: 3,
            owner_id: 1,
        }],
    )
    .expect("governance fixture should be written");

    let report = prune_stale_agents(&plugin, true).expect("stale agents should prune");
    assert!(report.contains("\"pruned_agents\": 1"));
    assert!(report.contains("IDLE0001"));

    let agents = read_connected_agents(&plugin.connected_path()).expect("agents should load");
    assert!(agents.iter().any(|agent| agent.agent_id == "ACTIVE01"));
    assert!(!agents.iter().any(|agent| agent.agent_id == "IDLE0001"));
    assert!(idle_login.is_file(), "prune should not delete login files");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn topology_normalization_deduplicates_same_workspace_entries() {
    let root = unique_temp_dir("ac_ant_delirium_dedup_agents");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    write_connected_agents(
        &plugin.connected_path(),
        &[
            ConnectedAgent {
                internal_id: 0,
                agent_id: agent_id_for_workspace_path(plugin.workspace_path.as_path()),
                branch: String::new(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: plugin.workspace_path.to_string_lossy().to_string(),
                login_path: plugin.login_path().to_string_lossy().to_string(),
                assigned_mission: "src/range.rs".to_string(),
                last_ping: 1,
            },
            ConnectedAgent {
                internal_id: 7,
                agent_id: agent_id_for_workspace_path(plugin.workspace_path.as_path()),
                branch: String::new(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: plugin.workspace_path.to_string_lossy().to_string(),
                login_path: plugin.login_path().to_string_lossy().to_string(),
                assigned_mission: "src/string.rs".to_string(),
                last_ping: 2,
            },
        ],
    )
    .expect("seeded duplicate agents should be written");

    let registered = sync_agent_registration(&plugin)
        .expect("registration should succeed")
        .expect("worktree should remain registered");
    assert_eq!(registered.internal_id, 1);

    let agents = read_connected_agents(&plugin.connected_path()).expect("agents should load");
    assert_eq!(agents.len(), 2, "root + one deduped worktree");
    assert!(agents.iter().any(|agent| agent.internal_id == 0));
    assert_eq!(
        agents
            .iter()
            .filter(|agent| normalize_path_str(&agent.worktree_path)
                == normalize_path(plugin.workspace_path.as_path()))
            .count(),
        1
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn topology_normalization_discards_invalid_workspace_entries() {
    let root = unique_temp_dir("ac_ant_delirium_discards_invalid_agents");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    write_connected_agents(
        &plugin.connected_path(),
        &[
            ConnectedAgent {
                internal_id: 0,
                agent_id: agent_id_for_workspace_path(plugin.repository_path.as_path()),
                branch: String::new(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: plugin.repository_path.to_string_lossy().to_string(),
                login_path: expected_login_path_from_workspace_path(
                    plugin.repository_path.as_path(),
                )
                .to_string_lossy()
                .to_string(),
                assigned_mission: "global".to_string(),
                last_ping: 1,
            },
            ConnectedAgent {
                internal_id: 4,
                agent_id: "BAD00004".to_string(),
                branch: String::new(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: "2".to_string(),
                login_path: "2".to_string(),
                assigned_mission: "src/ghost.rs".to_string(),
                last_ping: 2,
            },
            ConnectedAgent {
                internal_id: 5,
                agent_id: "BAD00005".to_string(),
                branch: String::new(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: "root updated swarm policy".to_string(),
                login_path: "root updated swarm policy".to_string(),
                assigned_mission: "src/ghost2.rs".to_string(),
                last_ping: 3,
            },
        ],
    )
    .expect("seeded invalid agents should be written");

    let registered = sync_agent_registration(&plugin)
        .expect("registration should succeed")
        .expect("worktree should register");
    assert_eq!(registered.internal_id, 1);

    let agents = read_connected_agents(&plugin.connected_path()).expect("agents should load");
    assert_eq!(agents.len(), 2, "root + current worktree only");
    assert!(agents
        .iter()
        .all(|agent| is_valid_registered_workspace_path(&agent.worktree_path)));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn projects_report_groups_agents_by_repository_path() {
    let root = unique_temp_dir("ac_ant_delirium_projects_report");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );
    let plugin_b = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-2_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root agent should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent a should register")
        .expect("agent a should exist");
    sync_agent_registration(&plugin_b)
        .expect("agent b should register")
        .expect("agent b should exist");

    let rendered = render_projects_report(&plugin_root, false).expect("projects should render");
    assert!(rendered.contains("Project count: 1"));
    assert!(rendered.contains(plugin_root.repository_path.to_string_lossy().as_ref()));
    assert!(rendered.contains("agents=3"));
    assert!(rendered.contains("worktrees=2"));

    let rendered_json =
        render_projects_report(&plugin_root, true).expect("projects json should render");
    assert!(rendered_json.contains("\"project_count\": 1"));
    assert!(rendered_json.contains("\"agent_count\": 3"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn current_and_direct_agent_identity_reports_render() {
    let root = unique_temp_dir("ac_ant_delirium_agent_identity_reports");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    let agent_a = sync_agent_registration(&plugin_a)
        .expect("agent a should register")
        .expect("agent a should exist");
    write_governance_tasks(
        &plugin_root.governance_path(),
        &[GovernanceTask {
            task_id: "TASK0001".to_string(),
            task_version: 3,
            task_revision: "ROOT123".to_string(),
            task_content_hash: "HASH123".to_string(),
            file_path: "src/lib.rs".to_string(),
            violation_count: 4,
            owner_id: agent_a.internal_id,
        }],
    )
    .expect("governance fixture should be written");

    let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
    assert!(whoami.contains("Agent 1"));
    assert!(whoami.contains("State summary: active governed task is `src/lib.rs` at version 3"));
    assert!(whoami.contains(&agent_a.agent_id));
    assert!(whoami.contains(plugin_a.login_path().to_string_lossy().as_ref()));
    assert!(whoami.contains("Submit task ID: -"));
    assert!(whoami.contains("Submit task file: -"));
    assert!(whoami.contains("Active task ID: TASK0001"));
    assert!(whoami.contains("Active task version: 3"));
    assert!(whoami.contains("Active task file: src/lib.rs"));

    let by_id = render_agent_identity(&plugin_root, agent_a.internal_id, true)
        .expect("agent should render");
    assert!(by_id.contains("\"internal_id\": 1"));
    assert!(by_id.contains(&format!("\"agent_id\": \"{}\"", agent_a.agent_id)));
    assert!(by_id.contains("\"login_path\":"));
    assert!(by_id.contains("\"active_task_id\": \"TASK0001\""));
    assert!(by_id.contains("\"active_task_version\": 3"));
    assert!(by_id.contains("\"active_task_file_path\": \"src/lib.rs\""));

    let root_identity =
        render_agent_identity(&plugin_root, 0, true).expect("root agent should render");
    assert!(root_identity.contains("\"root_scope\": true"));
    assert!(root_identity.contains("\"active_task_id\": \"\""));
    assert!(root_identity.contains("\"active_task_version\": 0"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn public_messages_feed_supports_topic_filtering() {
    let root = unique_temp_dir("ac_ant_delirium_public_messages_feed");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent a should register")
        .expect("agent a should exist");

    append_message(
        &plugin_root,
        Some(-1),
        None,
        None,
        None,
        Some("sync-policy"),
        None,
        None,
        None,
        None,
        None,
        "broadcast policy",
    )
    .expect("broadcast should store");
    append_message(
        &plugin_a,
        Some(0),
        None,
        None,
        None,
        Some("range.rs"),
        None,
        None,
        None,
        None,
        None,
        "range question",
    )
    .expect("directed message should store");

    let feed = render_messages_feed(&plugin_root, None, false).expect("feed should render");
    assert!(feed.contains("broadcast policy"));
    assert!(feed.contains("range question"));
    assert!(feed.contains("visibility=public"));

    let thread =
        render_messages_feed(&plugin_root, Some("range.rs"), false).expect("thread should render");
    assert!(thread.contains("range question"));
    assert!(!thread.contains("broadcast policy"));

    let thread_json = render_messages_feed(&plugin_root, Some("sync-policy"), true)
        .expect("thread json should render");
    assert!(thread_json.contains("\"topic\": \"sync-policy\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn render_agents_report_reconciles_missing_login_entries() {
    let root = unique_temp_dir("ac_ant_delirium_reconcile_missing_login");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );

    fs::create_dir_all(plugin.repository_path.join("AC_Ant-Delirium_props/login"))
        .expect("root login dir should exist");
    fs::write(
        plugin
            .repository_path
            .join("AC_Ant-Delirium_props/login/login.json"),
        format!(
            "{{\"repository_path\":\"{}\",\"workspace_path\":\"{}\"}}",
            plugin.repository_path.to_string_lossy(),
            plugin.repository_path.to_string_lossy()
        ),
    )
    .expect("root login should be written");

    let ghost_workspace = root.join("Ant-Delirium-Agent-9_RAYON");
    fs::create_dir_all(&ghost_workspace).expect("ghost workspace dir should exist");

    write_connected_agents(
        &plugin.connected_path(),
        &[
            ConnectedAgent {
                internal_id: 0,
                agent_id: agent_id_for_workspace_path(plugin.repository_path.as_path()),
                branch: String::new(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: plugin.repository_path.to_string_lossy().to_string(),
                login_path: plugin
                    .repository_path
                    .join("AC_Ant-Delirium_props/login/login.json")
                    .to_string_lossy()
                    .to_string(),
                assigned_mission: "global".to_string(),
                last_ping: 1,
            },
            ConnectedAgent {
                internal_id: 9,
                agent_id: agent_id_for_workspace_path(ghost_workspace.as_path()),
                branch: String::new(),
                base_revision: String::new(),
                workspace_revision: String::new(),
                repository_revision: String::new(),
                worktree_path: ghost_workspace.to_string_lossy().to_string(),
                login_path: ghost_workspace
                    .join("AC_Ant-Delirium_props/login/login.json")
                    .to_string_lossy()
                    .to_string(),
                assigned_mission: "ghost".to_string(),
                last_ping: 2,
            },
        ],
    )
    .expect("seed agents should be written");

    let rendered = render_agents_report(&plugin, false).expect("status should render");
    assert!(rendered.contains("agent 0"));
    assert!(!rendered.contains("agent 9"));

    let agents = read_connected_agents(&plugin.connected_path()).expect("agents should load");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].internal_id, 0);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn feedback_entries_store_taxonomy_context_and_commands() {
    let root = unique_temp_dir("ac_ant_delirium_feedback_context");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    sync_agent_registration(&plugin)
        .expect("root registration should succeed")
        .expect("root should be represented as agent zero");

    append_feedback(
        &plugin,
        "Explain better the sovereign limit for this case",
        Some("hierarchy.files.max_nesting"),
        None,
        None,
        None,
        None,
        None,
        &canonical_runtime_plugin_command(),
    )
    .expect("feedback should be stored");

    let raw = fs::read_to_string(plugin.feedback_path()).expect("feedback file should exist");
    assert!(raw.contains("\"internal_id\": 0"));
    assert!(raw.contains("\"agent_id\": \""));
    assert!(raw.contains("\"execution_context\": \"repository root\""));
    assert!(raw.contains("\"taxonomy_path\": \"hierarchy.files.max_nesting\""));
    assert!(raw.contains(&format!(
        "\"api_taxonomy_command\": \"{} --api hierarchy.files.max_nesting\"",
        canonical_runtime_plugin_command()
    )));
    assert!(raw.contains(&format!(
        "\"feedback_help_command\": \"{} --help.feedback\"",
        canonical_runtime_plugin_command()
    )));
    assert!(raw.contains("\"login_path\":"));
    assert!(raw.contains("\"repository_path\":"));
    assert!(raw.contains(plugin.login_path().to_string_lossy().as_ref()));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn feedback_requires_reconciled_agent_identity() {
    let root = unique_temp_dir("ac_ant_delirium_feedback_requires_login");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    let err = append_feedback(
        &plugin,
        "this should fail without login",
        Some("hierarchy.files.max_nesting"),
        None,
        None,
        None,
        None,
        None,
        &canonical_runtime_plugin_command(),
    )
    .expect_err("feedback should require a reconciled agent");
    assert!(err.to_string().contains("reconciled agent"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn sync_message_requires_explicit_metadata() {
    let root = unique_temp_dir("ac_ant_delirium_sync_message_requires_metadata");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    sync_agent_registration(&plugin)
        .expect("root registration should succeed")
        .expect("root should exist");

    let err = append_message(
        &plugin,
        Some(-1),
        None,
        None,
        None,
        Some("sync"),
        Some("sync"),
        None,
        None,
        None,
        None,
        "missing metadata should fail",
    )
    .expect_err("sync message should require explicit metadata");
    assert!(err.to_string().contains(
        "provide --branch, --base-revision, --workspace-revision, --repository-revision"
    ));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_requires_explicit_metadata_before_recording() {
    let root = unique_temp_dir("ac_ant_delirium_submit_requires_metadata");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    let agent = sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");
    write_governance_tasks(
        &plugin.governance_path(),
        &[GovernanceTask {
            task_id: "RANGE001".to_string(),
            task_version: 1,
            task_revision: "ROOT1".to_string(),
            task_content_hash: String::new(),
            file_path: "src/range.rs".to_string(),
            violation_count: 12,
            owner_id: agent.internal_id,
        }],
    )
    .expect("governance should write");
    assign_task_to_current_agent_if_missing(&plugin)
        .expect("assignment should succeed")
        .expect("mission should exist");

    let err = append_submit(
        &plugin,
        "range.rs ready for integration",
        Some("submit.range"),
        Some("hierarchy.submit"),
        None,
        None,
        None,
        None,
        &canonical_runtime_plugin_command(),
    )
    .expect_err("submit should require explicit metadata");
    let err = err.to_string();
    assert!(err.contains("submit metadata missing for submit operation"));
    assert!(err.contains("--branch"));
    assert!(err.contains("--base-revision"));
    assert!(err.contains("--workspace-revision"));
    assert!(err.contains("--repository-revision"));
    assert!(err.contains("The motor does not run Git commands"));
    assert!(err.contains("--branch is the current work branch name"));
    assert!(err.contains("--repository-revision is the current root/repository revision"));

    let queue = read_submit_records(&plugin.submit_queue_path()).expect("queue should load");
    assert!(queue.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn submit_feedback_requires_explicit_metadata_guidance() {
    let root = unique_temp_dir("ac_ant_delirium_submit_feedback_requires_metadata");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin)
        .expect("agent should register")
        .expect("agent should exist");

    let err = append_feedback(
        &plugin,
        "submit feedback missing metadata should fail",
        Some("hierarchy.submit"),
        Some("submit"),
        None,
        None,
        None,
        None,
        &canonical_runtime_plugin_command(),
    )
    .expect_err("submit feedback should require explicit metadata");
    let err = err.to_string();
    assert!(err.contains("feedback metadata missing for submit operation"));
    assert!(err.contains("Example: --branch <branch>"));
    assert!(err.contains("The motor does not run Git commands"));
    assert!(err.contains("--workspace-revision is the current workspace revision"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn feedback_append_does_not_copy_message_entries_into_feedback_file() {
    let root = unique_temp_dir("ac_ant_delirium_feedback_isolated_file");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );

    sync_agent_registration(&plugin)
        .expect("root registration should succeed")
        .expect("root should be represented as agent zero");
    append_message(
        &plugin,
        Some(0),
        None,
        None,
        None,
        Some("feedback-isolation"),
        None,
        None,
        None,
        None,
        None,
        "message should stay in messages.json",
    )
    .expect("message should be stored");
    append_feedback(
        &plugin,
        "feedback should stay isolated",
        Some("hierarchy.files.max_nesting"),
        None,
        None,
        None,
        None,
        None,
        &canonical_runtime_plugin_command(),
    )
    .expect("feedback should be stored");

    let feedback_raw =
        fs::read_to_string(plugin.feedback_path()).expect("feedback file should exist");
    assert!(feedback_raw.contains("feedback should stay isolated"));
    assert!(!feedback_raw.contains("message should stay in messages.json"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn rebase_check_detects_stale_state_from_persisted_metadata() {
    let root = unique_temp_dir("ac_ant_delirium_rebase_check_stale");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root registration should succeed")
        .expect("root should exist");
    sync_agent_registration(&plugin)
        .expect("registration should succeed")
        .expect("agent should exist");

    append_feedback(
        &plugin_root,
        "root metadata updated",
        Some("hierarchy.sync"),
        Some("sync"),
        Some("main"),
        Some("ROOT2"),
        Some("ROOT2"),
        Some("ROOT2"),
        &canonical_runtime_plugin_command(),
    )
    .expect("root feedback should persist sync metadata");

    append_feedback(
        &plugin,
        "worktree still based on older root",
        Some("hierarchy.sync"),
        Some("submit"),
        Some("ant-agent-1"),
        Some("ROOT1"),
        Some("WORK1"),
        Some("ROOT2"),
        &canonical_runtime_plugin_command(),
    )
    .expect("worktree feedback should persist sync metadata");

    let rendered = render_rebase_check(&plugin, false).expect("rebase check should render");
    assert!(rendered.contains("Sync state: stale"));
    assert!(rendered.contains("Rebase required: yes"));

    let rendered_json =
        render_rebase_check(&plugin, true).expect("json rebase check should render");
    assert!(rendered_json.contains("\"rebase_required\": true"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn inbox_unavailable_after_login_file_is_removed() {
    let root = unique_temp_dir("ac_ant_delirium_inbox_missing_login");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin)
        .expect("registration should succeed")
        .expect("agent should exist");
    fs::remove_file(plugin.login_path()).expect("login file should be removable");

    let inbox = render_current_inbox(&plugin).expect("inbox should render");
    assert!(inbox.contains("not a registered agent"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn plugin_help_keeps_installation_canonical() {
    let rendered = render_plugin_help(None, &canonical_runtime_plugin_command());
    assert!(rendered.contains("--installation"));
    assert!(!rendered.contains("--instalation"));
    assert!(!rendered.contains("--installtion"));
    assert!(rendered.contains("config init"));
    assert!(rendered.contains("config hooks init"));
    assert!(rendered.contains("config hooks verify merge"));
    assert!(rendered.contains("config hooks verify push"));
    assert!(rendered.contains("config dump"));
    assert!(rendered.contains("swarm status"));
    assert!(rendered.contains("--help.feedback"));
    assert!(rendered.contains("stale_reason"));
    assert!(rendered.contains("--whoami"));
    assert!(rendered.contains("--root-checks"));
    assert!(rendered.contains("--release-stale-agent-claims"));
    assert!(rendered.contains("--prune-stale-agents"));
    assert!(rendered.contains("Canonical submit lifecycle"));
    assert!(rendered.contains("Canonical multi-agent bootstrap"));
    assert!(rendered.contains("--submits-active"));
    assert!(rendered.contains("--next-action"));
    assert!(rendered.contains("revises a rejected submit"));
    assert!(rendered.contains("continues active task work"));
    assert!(rendered.contains("cargo check"));
    assert!(rendered.contains("AC_Ant-Delirium_props/login"));
}

#[test]
fn config_init_materializes_default_tomls_and_build_guard_for_new_project() {
    let root = unique_temp_dir("ac_ant_delirium_config_init");
    let project_root = root.join("Mail_Project");
    fs::create_dir_all(&project_root).expect("project root should exist");

    let fake_bin = root.join("delirium_cli");
    fs::write(&fake_bin, "fake-binary").expect("fake binary should exist");

    let plugin_dir = project_root.join("plugins").join("Stalwart");
    fs::create_dir_all(&plugin_dir).expect("plugin dir should exist");

    let copied_binary = copy_binary_if_needed(
        &fake_bin,
        &plugin_dir
            .join("bin")
            .join(engine_identity::DEFAULT_PLUGIN_BINARY_NAME),
    )
    .expect("binary copy should succeed");
    assert!(copied_binary);

    let hierarchy_written = write_default_template_if_missing(
        &plugin_dir.join("ant_delirium_config/default_hierarchy.toml"),
        &render_new_project_hierarchy(&project_root, "Stalwart"),
    )
    .expect("hierarchy should be written");
    let rules_written = write_default_template_if_missing(
        &plugin_dir.join("ant_delirium_config/default_rules.toml"),
        DEFAULT_NEW_PROJECT_RULES,
    )
    .expect("rules should be written");
    let guard_written = write_default_template_if_missing(
        &project_root.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"),
        &render_new_project_build_guard("Stalwart"),
    )
    .expect("guard should be written");

    assert!(hierarchy_written);
    assert!(rules_written);
    assert!(guard_written);
    assert!(plugin_dir
        .join("ant_delirium_config/default_hierarchy.toml")
        .is_file());
    assert!(plugin_dir
        .join("ant_delirium_config/default_rules.toml")
        .is_file());
    assert!(project_root
        .join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs")
        .is_file());
    assert!(!plugin_dir.join("ant_delirium_config/type_1").exists());
    assert!(!plugin_dir.join("ant_delirium_config/type_2").exists());
    assert!(!plugin_dir.join("ant_delirium_config/type_3").exists());

    let hierarchy =
        fs::read_to_string(plugin_dir.join("ant_delirium_config/default_hierarchy.toml"))
            .expect("hierarchy should be readable");
    assert!(hierarchy.contains(&project_root.to_string_lossy().replace('\\', "/")));
    assert!(hierarchy.contains("../plugins/Stalwart/bin/ac_ant_delirium"));
    assert!(hierarchy.contains("../plugins/Stalwart/ant_delirium_config/default_hierarchy.toml"));
    assert!(!hierarchy.contains("../plugins/Mail_Project/"));
    let orchestrator = render_new_project_task_orchestrator("Stalwart");
    assert!(orchestrator.contains("../plugins/Stalwart/plan/[0.0]_Sample_Plan.json"));
    assert!(!orchestrator.contains("Aesthetic-Red"));
    let guard =
        fs::read_to_string(project_root.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"))
            .expect("guard should be readable");
    assert!(guard.contains("pub(crate) fn run"));
    assert!(guard.contains("hierarchy.repository"));
    assert!(guard.contains("candidate_plugin_roots"));
    assert!(!guard.contains("Stalwart"));
    assert!(!guard.contains("Mail_Project"));
    assert!(!guard.contains("Aesthetic-Red"));
    assert!(!guard.contains("__ANT_DELIRIUM_PLUGIN_PACKAGE__"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn config_init_detects_existing_plans_in_nested_directories() {
    let root = unique_temp_dir("ac_ant_delirium_nested_plan_detection");
    let plan_dir = root.join("plan/foundation");
    fs::create_dir_all(&plan_dir).expect("nested plan directory should exist");
    fs::write(plan_dir.join("[0.0]_Real_Plan.json"), "{}\n").expect("real plan should be written");

    assert!(directory_contains_json(&root.join("plan")).expect("plan tree should be readable"));
    assert!(!root.join("plan/[0.0]_Sample_Plan.json").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn plugin_context_prefers_lowercase_global_transition_task_path() {
    let root = unique_temp_dir("ac_ant_delirium_lowercase_global_transition");
    let plugin = temp_plugin_context(&root, "Stalwart", "Mail_Project", "Mail_Project");
    let lower = plugin
        .plugin_dir
        .join("global")
        .join("transition_task.json");
    let legacy = plugin
        .plugin_dir
        .join("Global")
        .join("transition_task.json");
    fs::create_dir_all(lower.parent().unwrap()).expect("lower global dir should exist");
    fs::create_dir_all(legacy.parent().unwrap()).expect("legacy Global dir should exist");
    fs::write(&lower, r#"{ "transition_tasks": [] }"#).expect("lower transition should write");
    fs::write(&legacy, r#"{ "transition_tasks": [] }"#).expect("legacy transition should write");

    assert_eq!(plugin.global_transition_task_path(), lower);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn plugin_context_falls_back_to_legacy_global_transition_task_path() {
    let root = unique_temp_dir("ac_ant_delirium_legacy_global_transition");
    let plugin = temp_plugin_context(&root, "Aesthetic-Red", "Aesthetic-Red", "Aesthetic-Red");
    let legacy = plugin
        .plugin_dir
        .join("Global")
        .join("transition_task.json");
    fs::create_dir_all(legacy.parent().unwrap()).expect("legacy Global dir should exist");
    fs::write(&legacy, r#"{ "transition_tasks": [] }"#).expect("legacy transition should write");

    assert_eq!(plugin.global_transition_task_path(), legacy);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn hooks_init_materializes_managed_git_hooks() {
    let root = unique_temp_dir("ac_ant_delirium_hooks_init");
    let project_root = root.join("AC_Rayon-Genetics");
    fs::create_dir_all(&project_root).expect("project root should exist");

    let fake_bin = root.join("delirium_cli");
    fs::write(&fake_bin, "fake-binary").expect("fake binary should exist");

    let report = materialize_managed_git_hooks(
        &project_root,
        Some(project_root.to_string_lossy().as_ref()),
        None,
        fake_bin.to_string_lossy().as_ref(),
    )
    .expect("hooks init should succeed");

    let hooks_dir = project_root.join("AC_Ant-Delirium_props/githooks");
    let orchestration_dir = project_root.join("AC_Ant-Delirium_props/orchestration");
    assert!(hooks_dir.join("README.md").is_file());
    assert!(hooks_dir.join("pre-commit").is_file());
    assert!(hooks_dir.join("commit-msg").is_file());
    assert!(hooks_dir.join("pre-merge-commit").is_file());
    assert!(hooks_dir.join("pre-push").is_file());
    assert!(orchestration_dir
        .join("merge_request.template.json")
        .is_file());
    assert!(orchestration_dir
        .join("push_request.template.json")
        .is_file());

    let pre_commit =
        fs::read_to_string(hooks_dir.join("pre-commit")).expect("pre-commit should be readable");
    assert!(pre_commit.contains("current workspace is not a registered agent"));

    let commit_msg =
        fs::read_to_string(hooks_dir.join("commit-msg")).expect("commit-msg should be readable");
    assert!(commit_msg.contains("missing stable agent identity"));
    assert!(commit_msg.contains("missing explicit submit_id"));
    assert!(commit_msg.contains("submit_id=<ID>"));

    let pre_merge_commit = fs::read_to_string(hooks_dir.join("pre-merge-commit"))
        .expect("pre-merge-commit should be readable");
    assert!(pre_merge_commit.contains("missing orchestration/merge_request.json"));
    assert!(pre_merge_commit.contains("The motor does not discover Git state"));

    let pre_push =
        fs::read_to_string(hooks_dir.join("pre-push")).expect("pre-push should be readable");
    assert!(pre_push.contains("must not push directly to main"));
    assert!(pre_push.contains("missing orchestration/push_request.json"));
    assert!(pre_push.contains("The motor does not discover Git state"));

    let merge_template = fs::read_to_string(orchestration_dir.join("merge_request.template.json"))
        .expect("merge template should be readable");
    assert!(merge_template.contains("\"branch\": \"ant-agent-1\""));
    assert!(merge_template.contains("\"approved_by_agent_id\": \"83BAV05E\""));

    let push_template = fs::read_to_string(orchestration_dir.join("push_request.template.json"))
        .expect("push template should be readable");
    assert!(push_template.contains("\"push_target\": \"origin/ant-agent-1\""));

    assert!(report.contains("Managed Git hooks ensured."));
    assert!(report.contains("activate AC_Ant-Delirium_props/githooks in your hook system manually"));
    assert!(report.contains("fill agent_id, workspace_path, branch, base_revision, workspace_revision and repository_revision explicitly"));
    assert!(report.contains("config hooks verify merge"));
    assert!(report.contains("config hooks verify push"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn hooks_verify_merge_accepts_matching_explicit_metadata() {
    let root = unique_temp_dir("ac_ant_delirium_hooks_verify_merge_ok");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    let agent = sync_agent_registration(&plugin)
        .expect("registration should succeed")
        .expect("agent should exist");
    materialize_managed_git_hooks(
        &plugin.repository_path,
        Some(plugin.repository_path.to_string_lossy().as_ref()),
        Some(&plugin),
        &canonical_runtime_plugin_command(),
    )
    .expect("hooks should materialize");

    fs::write(
        plugin
            .repository_path
            .join("AC_Ant-Delirium_props/orchestration/merge_request.json"),
        format!(
            "{{\n  \"agent_id\": \"{}\",\n  \"workspace_path\": \"{}\",\n  \"branch\": \"ant-agent-1\",\n  \"base_revision\": \"BASE1234\",\n  \"workspace_revision\": \"WORK5678\",\n  \"repository_revision\": \"ROOT9999\",\n  \"submit_id\": \"SUBMIT123\",\n  \"approved_by_agent_id\": \"83BAV05E\",\n  \"approval_note\": \"ok\"\n}}\n",
            agent.agent_id,
            plugin.workspace_path.to_string_lossy()
        ),
    )
    .expect("merge request should write");

    let report = validate_managed_orchestration_request(
        &plugin.repository_path,
        Some(plugin.repository_path.to_string_lossy().as_ref()),
        Some(&plugin),
        "merge",
        false,
    )
    .expect("merge request should validate");
    assert!(report.contains("Managed orchestration request is valid."));
    assert!(report.contains("Kind: merge"));
    assert!(report.contains("Submit ID: SUBMIT123"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn hooks_verify_push_rejects_missing_or_mismatched_metadata() {
    let root = unique_temp_dir("ac_ant_delirium_hooks_verify_push_invalid");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin)
        .expect("registration should succeed")
        .expect("agent should exist");
    materialize_managed_git_hooks(
        &plugin.repository_path,
        Some(plugin.repository_path.to_string_lossy().as_ref()),
        Some(&plugin),
        &canonical_runtime_plugin_command(),
    )
    .expect("hooks should materialize");

    fs::write(
        plugin
            .repository_path
            .join("AC_Ant-Delirium_props/orchestration/push_request.json"),
        format!(
            "{{\n  \"agent_id\": \"WRONG123\",\n  \"workspace_path\": \"{}\",\n  \"branch\": \"ant-agent-1\",\n  \"base_revision\": \"BASE1234\",\n  \"workspace_revision\": \"WORK5678\",\n  \"repository_revision\": \"ROOT9999\",\n  \"submit_id\": \"SUBMIT123\",\n  \"push_target\": \"\",\n  \"approved_by_agent_id\": \"83BAV05E\"\n}}\n",
            plugin.repository_path.to_string_lossy()
        ),
    )
    .expect("push request should write");

    let err = validate_managed_orchestration_request(
        &plugin.repository_path,
        Some(plugin.repository_path.to_string_lossy().as_ref()),
        Some(&plugin),
        "push",
        false,
    )
    .expect_err("push request should fail");
    let err = err.to_string();
    assert!(err.contains("push_request.json"));
    assert!(err.contains("push_target"));
    assert!(err.contains("explicitly"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn hooks_init_commit_msg_requires_submit_id_contract() {
    let root = unique_temp_dir("ac_ant_delirium_commit_msg_submit_id");
    let project_root = root.join("AC_Rayon-Genetics");
    fs::create_dir_all(&project_root).expect("project root should exist");

    let fake_bin = root.join("delirium_cli");
    fs::write(&fake_bin, "fake-binary").expect("fake binary should exist");

    materialize_managed_git_hooks(
        &project_root,
        Some(project_root.to_string_lossy().as_ref()),
        None,
        fake_bin.to_string_lossy().as_ref(),
    )
    .expect("hooks init should succeed");

    let commit_msg =
        fs::read_to_string(project_root.join("AC_Ant-Delirium_props/githooks/commit-msg"))
            .expect("commit-msg should be readable");
    assert!(commit_msg.contains("submit_id=$(sed -n"));
    assert!(commit_msg.contains("missing explicit submit_id"));
    assert!(commit_msg.contains("submit_id=<ID>"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn materialize_project_local_plugin_package_creates_guard_in_repository_root() {
    let root = unique_temp_dir("ac_ant_delirium_materialize_guard");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let guard_path = plugin
        .repository_path
        .join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs");
    fs::create_dir_all(guard_path.parent().expect("guard parent should exist"))
        .expect("guard parent should be created");
    fs::write(&guard_path, "stale manually edited guard\n").expect("stale guard should be seeded");

    let report = materialize_project_local_plugin_package(
        &plugin.repository_path,
        None,
        None,
        Some(&plugin),
        &canonical_runtime_plugin_command(),
    )
    .expect("materialization should succeed");

    assert!(guard_path.is_file());
    let guard = fs::read_to_string(&guard_path).expect("guard should be readable");
    assert!(!guard.contains("stale manually edited guard"));
    assert!(guard.contains("hierarchy.repository"));
    assert!(!guard.contains("AC_Rayon-Genetics"));
    assert!(report.contains("Guard path:"));
    assert!(report.contains("ant_delirium_build_guard.rs"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn bootstrap_context_prefers_plugin_dir_when_running_inside_plugins_folder() {
    let root = unique_temp_dir("ac_ant_delirium_bootstrap_context");
    let project_root = root.join("AC_Rayon-Genetics");
    let plugin_dir = root.join("plugins").join("AC_Rayon-Genetics");
    fs::create_dir_all(&project_root).expect("project root should exist");
    fs::create_dir_all(&plugin_dir).expect("plugin dir should exist");

    let fake_exe = plugin_dir.join(engine_identity::DEFAULT_PLUGIN_BINARY_NAME);
    let (effective_root, effective_plugin_dir) =
        infer_bootstrap_context(&plugin_dir, None, &fake_exe);

    assert_eq!(effective_root, project_root);
    assert_eq!(effective_plugin_dir, plugin_dir);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn bootstrap_context_resolves_project_local_bin_layout_from_repository_root() {
    let root = unique_temp_dir("ac_ant_delirium_project_local_bin_root");
    let project_root = root.join("Mail_Project");
    let plugin_dir = project_root.join("plugins").join("Stalwart");
    fs::create_dir_all(plugin_dir.join("bin")).expect("plugin bin dir should exist");

    let fake_exe = plugin_dir
        .join("bin")
        .join(engine_identity::DEFAULT_PLUGIN_BINARY_NAME);
    let (effective_root, effective_plugin_dir) =
        infer_bootstrap_context(&project_root, None, &fake_exe);

    assert_eq!(effective_root, project_root);
    assert_eq!(effective_plugin_dir, plugin_dir);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn bootstrap_context_resolves_project_local_bin_layout_from_plugin_cwd() {
    let root = unique_temp_dir("ac_ant_delirium_project_local_bin_plugin_cwd");
    let project_root = root.join("Mail_Project");
    let plugin_dir = project_root.join("plugins").join("Stalwart");
    fs::create_dir_all(plugin_dir.join("bin")).expect("plugin bin dir should exist");

    let fake_exe = plugin_dir
        .join("bin")
        .join(engine_identity::DEFAULT_PLUGIN_BINARY_NAME);
    let (effective_root, effective_plugin_dir) =
        infer_bootstrap_context(&plugin_dir, None, &fake_exe);

    assert_eq!(effective_root, project_root);
    assert_eq!(effective_plugin_dir, plugin_dir);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn explicit_config_target_resolves_project_local_plugin_dir_before_file_exists() {
    let root = unique_temp_dir("ac_ant_delirium_explicit_config_target");
    let project_root = root.join("Mail_Project");
    let config_dir = project_root
        .join("plugins")
        .join("Stalwart")
        .join("ant_delirium_config");

    assert_eq!(
        plugin_dir_from_config_target(&config_dir),
        Some(project_root.join("plugins").join("Stalwart"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn copy_binary_if_needed_skips_when_paths_point_to_same_existing_file() {
    let root = unique_temp_dir("ac_ant_delirium_copy_binary_same_file");
    let bin_path = root
        .join("plugins")
        .join("AC_Rayon-Genetics")
        .join(engine_identity::DEFAULT_PLUGIN_BINARY_NAME);
    if let Some(parent) = bin_path.parent() {
        fs::create_dir_all(parent).expect("parent should exist");
    }
    fs::write(&bin_path, "fake-binary").expect("binary should exist");

    let canonical = fs::canonicalize(&bin_path).expect("canonical path should resolve");
    let copied = copy_binary_if_needed(&canonical, &bin_path).expect("copy check should succeed");

    assert!(!copied, "same file should not be recopied");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn swarm_status_can_render_json() {
    let root = unique_temp_dir("ac_ant_delirium_swarm_status_json");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin)
        .expect("registration should succeed")
        .expect("worktree registration should return an agent");

    let rendered = render_agents_report(&plugin, true).expect("json status should render");
    assert!(rendered.contains("\"command\": \"swarm_status\""));
    assert!(rendered.contains("\"agents\": ["));
    assert!(rendered.contains("\"execution_context\": \"git worktree\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn manager_task_suppresses_normal_audit_until_it_is_done() {
    let root = unique_temp_dir("ac_ant_delirium_manager_task");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    write_manager_tasks(
        &plugin.manager_tasks_path(),
        &[ManagerTask {
            id: "manager-1".to_string(),
            title: "Hotfix".to_string(),
            body: "Handle the urgent coordination task".to_string(),
            file_path: "src/range.rs".to_string(),
            priority: 100,
            owner_id: -1,
            status: "open".to_string(),
            created_at: 1,
            mode: None,
            grade: None,
        }],
    )
    .expect("manager task fixture should be written");

    let registered = sync_agent_registration(&plugin)
        .expect("registration should succeed")
        .expect("worktree registration should return an agent");
    assert_eq!(registered.assigned_mission, "manager:Hotfix [src/range.rs]");

    let claimed_manager_tasks =
        read_manager_tasks(&plugin.manager_tasks_path()).expect("manager tasks should load");
    assert_eq!(claimed_manager_tasks.len(), 1);
    assert_eq!(claimed_manager_tasks[0].owner_id, registered.internal_id);
    assert_eq!(claimed_manager_tasks[0].status, "claimed");

    let audit_result = AuditResult {
        violations: vec![
            sample_violation("src/range.rs", "hierarchy.identity.dna_suffix"),
            sample_violation("src/string.rs", "hierarchy.identity.dna_suffix"),
        ],
        logs: vec!["full audit".to_string()],
    };

    let isolated =
        isolate_audit_to_current_agent(&plugin, &audit_result).expect("audit should isolate");
    assert!(
        isolated.violations.is_empty(),
        "manager task should suppress normal agent violations until completion"
    );
    assert!(
        isolated
            .logs
            .iter()
            .any(|entry| entry.contains("normal audit suppressed")),
        "suppressed audit should explain why no file violations were delivered"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn governance_tasks_are_assigned_and_filtered_per_agent() {
    let root = unique_temp_dir("ac_ant_delirium_governance_distribution");
    let plugin_root = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plugin_a = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );
    let plugin_b = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-2_RAYON",
    );

    sync_agent_registration(&plugin_root)
        .expect("root should register")
        .expect("root should exist");
    sync_agent_registration(&plugin_a)
        .expect("agent a should register")
        .expect("agent a should exist");
    sync_agent_registration(&plugin_b)
        .expect("agent b should register")
        .expect("agent b should exist");

    let audit_result = AuditResult {
        violations: vec![
            sample_violation("src/range.rs", "hierarchy.identity.dna_suffix"),
            sample_violation("src/range.rs", "hierarchy.structural.logical_density"),
            sample_violation("src/string.rs", "hierarchy.identity.dna_suffix"),
        ],
        logs: vec!["coordinated audit".to_string()],
    };

    refresh_governance_from_audit(&plugin_a, &audit_result)
        .expect("governance tasks should refresh");

    let mission_a = assign_task_to_current_agent_if_missing(&plugin_a)
        .expect("agent a assignment should succeed")
        .expect("agent a should receive a mission");
    let mission_b = assign_task_to_current_agent_if_missing(&plugin_b)
        .expect("agent b assignment should succeed")
        .expect("agent b should receive a mission");
    let mission_root = assign_task_to_current_agent_if_missing(&plugin_root)
        .expect("root assignment should succeed")
        .expect("root should receive a global mission");

    assert_eq!(mission_root, "global");
    assert_eq!(mission_a, "src/range.rs");
    assert_eq!(mission_b, "src/string.rs");

    let filtered_root =
        isolate_audit_to_current_agent(&plugin_root, &audit_result).expect("root filter");
    assert_eq!(filtered_root.violations.len(), 3);

    let filtered_a =
        isolate_audit_to_current_agent(&plugin_a, &audit_result).expect("agent a filter");
    assert_eq!(filtered_a.violations.len(), 2);
    assert!(filtered_a
        .violations
        .iter()
        .all(|violation| violation.file_path == "src/range.rs"));

    let filtered_b =
        isolate_audit_to_current_agent(&plugin_b, &audit_result).expect("agent b filter");
    assert_eq!(filtered_b.violations.len(), 1);
    assert!(filtered_b
        .violations
        .iter()
        .all(|violation| violation.file_path == "src/string.rs"));

    let tasks = read_governance_tasks(&plugin_a.governance_path())
        .expect("shared governance tasks should be readable");
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().all(|task| task.owner_id >= 0));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn rayon_project_audit_state_is_coordinated_across_worktrees() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should have a parent workspace")
        .to_path_buf();
    let repository_path = workspace_root.join("AC_Rayon-Genetics");
    let real_plugin_config = workspace_root
        .join("plugins")
        .join("AC_Rayon-Genetics")
        .join("ant_delirium_config");
    let worktree_paths = vec![
        workspace_root.join("Ant-Delirium-Agent-1_RAYON"),
        workspace_root.join("Ant-Delirium-Agent-2_RAYON"),
        workspace_root.join("Ant-Delirium-Agent-3_RAYON"),
    ];

    if !repository_path.is_dir()
        || !real_plugin_config.is_dir()
        || worktree_paths.iter().filter(|path| path.is_dir()).count() < 2
    {
        eprintln!("skipping rayon multi-agent coordination test: workspace fixtures not found");
        return;
    }

    let plugin_root = unique_temp_dir("ac_ant_delirium_real_rayon");
    let plugin_dir = plugin_root.join("plugins").join("AC_Rayon-Genetics");
    fs::create_dir_all(&plugin_dir).expect("temporary plugin directory should exist");
    let copied_config_dir = plugin_dir.join("ant_delirium_config");
    fs::create_dir_all(&copied_config_dir).expect("temporary config directory should exist");
    fs::copy(
        real_plugin_config.join("default_hierarchy.toml"),
        copied_config_dir.join("default_hierarchy.toml"),
    )
    .expect("real Rayon hierarchy config should be copied into the temp plugin directory");
    fs::copy(
        real_plugin_config.join("default_rules.toml"),
        copied_config_dir.join("default_rules.toml"),
    )
    .expect("real Rayon rules config should be copied into the temp plugin directory");

    let hierarchy = load_hierarchy_from_path(&copied_config_dir);
    let config = AuditConfig {
        hierarchy,
        runtime: RuntimeConfig {
            project_root: repository_path.to_string_lossy().to_string(),
            mode: GovernanceMode::Strict,
        },
        ..AuditConfig::default()
    };

    let audit_result = SwarmEngine::audit(&config);
    assert!(
        audit_result
            .logs
            .iter()
            .any(|entry| !entry.trim().is_empty()),
        "real Rayon audit should emit orchestration logs even when no violations are found"
    );

    let mut plugins = Vec::new();
    for path in worktree_paths.into_iter().filter(|path| path.is_dir()) {
        plugins.push(PluginContext {
            plugin_dir: plugin_dir.clone(),
            config_path: copied_config_dir.clone(),
            repository_path: repository_path.clone(),
            workspace_path: path.clone(),
            is_worktree: true,
        });
    }

    refresh_governance_from_audit(&plugins[0], &audit_result)
        .expect("real Rayon audit should populate governance tasks");

    for plugin in &plugins {
        sync_agent_registration(plugin)
            .expect("real Rayon worktree should register")
            .expect("worktree should create or refresh a registered agent");
    }

    let tasks = read_governance_tasks(&plugins[0].governance_path())
        .expect("shared Rayon governance state should be readable");

    if audit_result.violations.is_empty() {
        assert!(
            tasks.is_empty(),
            "when the real Rayon audit is clean, no governance tasks should be generated"
        );

        for plugin in &plugins {
            let mission = assign_task_to_current_agent_if_missing(plugin)
                .expect("clean Rayon assignment attempt should succeed");
            assert!(
                mission.is_none(),
                "clean Rayon audit should not assign missions to worktrees"
            );

            let isolated = isolate_audit_to_current_agent(plugin, &audit_result)
                .expect("audit should isolate");
            assert!(
                isolated.violations.is_empty(),
                "clean Rayon audit should remain empty after per-agent isolation"
            );
        }

        let _ = fs::remove_dir_all(plugin_root);
        return;
    }

    assert!(
        audit_result
            .violations
            .iter()
            .any(|violation| violation.file_path.starts_with("src/")
                || violation.file_path.starts_with("rayon-core/src/")),
        "Rayon audit should report files from the repository roots tracked by the plugin"
    );

    let expected_assignments = tasks.len().min(plugins.len());
    let mut missions = Vec::new();
    for plugin in plugins.iter().take(expected_assignments) {
        let mission = assign_task_to_current_agent_if_missing(plugin)
            .expect("task assignment should succeed")
            .expect("each Rayon worktree should receive a mission while tasks remain");
        missions.push((plugin.workspace_path.clone(), mission));
    }
    for plugin in plugins.iter().skip(expected_assignments) {
        let mission = assign_task_to_current_agent_if_missing(plugin)
            .expect("empty assignment should still succeed");
        assert!(
            mission.is_none(),
            "worktrees beyond the available Rayon tasks should not receive synthetic missions"
        );
    }

    assert!(
        tasks.iter().any(|task| task.owner_id >= 0),
        "at least one real Rayon violation file should be owned after assignment"
    );

    for (_, mission) in &missions {
        assert!(
            repository_path.join(mission).exists(),
            "assigned Rayon mission should point to a real repository file: {}",
            mission
        );
    }

    for (plugin, (_, mission)) in plugins.iter().zip(missions.iter()) {
        let isolated =
            isolate_audit_to_current_agent(plugin, &audit_result).expect("audit should isolate");
        assert!(
            !isolated.violations.is_empty(),
            "assigned Rayon worktree should receive at least one coordinated violation"
        );
        assert!(
            isolated
                .violations
                .iter()
                .all(|violation| violation.file_path == *mission),
            "each Rayon worktree should only receive violations for its assigned mission"
        );
    }

    let _ = fs::remove_dir_all(plugin_root);
}

#[test]
fn update_submit_status_approves_current_task_when_grade_is_passing() {
    let root = unique_temp_dir("ac_ant_delirium_orchestrator_grade");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
    );
    let plan_path = plugin.workspace_path.join("[1.0]_test_plan.json");
    let orchestrator_path = plugin.task_orchestrator_path();

    fs::write(&plan_path, r#"{
        "plan_id": "1.0",
        "title": "Test Plan",
        "complete_plan": "Test",
        "tasks": [{"step_id": "1.0.1", "task_prompt": "Test step", "implemented": true, "reviewed": false}]
    }"#).unwrap();

    fs::write(
        &orchestrator_path,
        r#"{
        "orchestration": {
            "active_plan": "[1.0]_test_plan.json",
            "current_step_id": "1.0.1",
            "mode": "review",
            "grade": 0.0
        }
    }"#,
    )
    .unwrap();

    sync_agent_registration(&plugin).unwrap().unwrap();

    let submit_id = "test-submit-1";
    fs::write(&plugin.submit_queue_path(), format!(
        "{{\n  \"entries\": [\n    {{\n      \"id\": \"{}\",\n      \"task_id\": \"\",\n      \"task_revision\": \"\",\n      \"task_content_hash\": \"\",\n      \"task_file_path\": \"\",\n      \"repository_path\": \"\",\n      \"workspace_path\": \"\",\n      \"title\": \"\",\n      \"topic\": \"\",\n      \"branch\": \"\",\n      \"base_revision\": \"\",\n      \"workspace_revision\": \"\",\n      \"repository_revision\": \"\",\n      \"evidence_content_hash\": \"\",\n      \"agent_id\": \"Test\",\n      \"status\": \"pending\",\n      \"created_at\": 0,\n      \"decision_by_internal_id\": -1,\n      \"decision_by_agent_id\": \"\",\n      \"decision_note\": \"\",\n      \"decided_at\": 0,\n      \"updated_at\": 0\n    }}\n  ]\n}}",
        submit_id
    )).unwrap();

    update_submit_status(&plugin, submit_id, "accepted", None, Some(8.5)).unwrap();

    let plan_content = fs::read_to_string(&plan_path).unwrap();
    assert!(plan_content.contains("\"review_status\": \"approved\""));
    assert!(!plan_content.contains("\"reviewed\":"));

    let orchestrator_content = fs::read_to_string(&orchestrator_path).unwrap();
    assert!(orchestrator_content.contains("\"grade\": 8.5"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn task_orchestrator_reads_legacy_state_without_transition() {
    let root = unique_temp_dir("ac_ant_delirium_legacy_orchestrator_transition");
    let path = root.join("task_orchestrator.json");
    fs::write(
        &path,
        r#"{
        "orchestration": {
            "active_plan": "[1.0]_test_plan.json",
            "current_step_id": "1.0.1",
            "mode": "implementation",
            "grade": 0.0,
            "task_start_timestamp": 11
        }
    }"#,
    )
    .expect("legacy orchestrator should write");

    let payload = read_task_orchestrator(&path)
        .expect("legacy orchestrator should read")
        .expect("legacy orchestrator should parse");

    assert_eq!(payload.orchestration.active_plan, "[1.0]_test_plan.json");
    assert_eq!(payload.orchestration.current_step_id, "1.0.1");
    assert_eq!(payload.transition, None);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn task_orchestrator_round_trips_transition_state() {
    let root = unique_temp_dir("ac_ant_delirium_orchestrator_transition_round_trip");
    let path = root.join("task_orchestrator.json");
    let payload = TaskOrchestratorPayload {
        orchestration: TaskOrchestration {
            active_plan:
                "../plugins/Aesthetic-Red/plan/widgets/[1.36]_Plan___TextBlock_Widget.json"
                    .to_string(),
            current_step_id: "1.36.10".to_string(),
            mode: "implementation".to_string(),
            grade: 0.0,
            task_start_timestamp: 42,
        },
        transition: Some(TransitionTaskState {
            active: true,
            source_plan:
                "../plugins/Aesthetic-Red/plan/widgets/[1.36]_Plan___TextBlock_Widget.json"
                    .to_string(),
            source_plan_id: "1.36".to_string(),
            final_step_id: "1.36.9".to_string(),
            transition_source: "../plugins/Aesthetic-Red/Global/transition_task.json".to_string(),
            current_transition_id: "1.0".to_string(),
            current_projected_step_id: "1.36.10".to_string(),
            completed_transition_ids: vec!["1.0".to_string(), "1.1".to_string()],
        }),
    };

    write_task_orchestrator(&path, &payload).expect("orchestrator should write");
    let parsed = read_task_orchestrator(&path)
        .expect("orchestrator should read")
        .expect("orchestrator should parse");

    assert_eq!(parsed, payload);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn final_plan_step_id_uses_numeric_segment_order() {
    let plan = NumberedPlan {
        plan_id: "1.0".to_string(),
        title: "Numeric Step Plan".to_string(),
        complete_plan: String::new(),
        tasks: vec![
            NumberedPlanStep {
                step_id: "1.9".to_string(),
                task_prompt: String::new(),
                task_reviewed: true,
                review_explanation: String::new(),
                implemented: true,
                implementation_explanation: String::new(),
                implementation_context: String::new(),
                review_status: "none".to_string(),
                grade_explanation: String::new(),
            },
            NumberedPlanStep {
                step_id: "1.10".to_string(),
                task_prompt: String::new(),
                task_reviewed: true,
                review_explanation: String::new(),
                implemented: true,
                implementation_explanation: String::new(),
                implementation_context: String::new(),
                review_status: "none".to_string(),
                grade_explanation: String::new(),
            },
            NumberedPlanStep {
                step_id: "1.2".to_string(),
                task_prompt: String::new(),
                task_reviewed: true,
                review_explanation: String::new(),
                implemented: true,
                implementation_explanation: String::new(),
                implementation_context: String::new(),
                review_status: "none".to_string(),
                grade_explanation: String::new(),
            },
        ],
    };

    assert_eq!(
        final_plan_step_id(&plan)
            .expect("final step should parse")
            .as_deref(),
        Some("1.10")
    );
}

#[test]
fn numbered_plan_reads_legacy_keys_but_writes_only_canonical_keys() {
    let root = unique_temp_dir("ac_ant_delirium_numbered_plan_schema");
    let path = root.join("[1.0]_legacy_plan.json");
    fs::write(
        &path,
        r#"{
        "plan_id": "1.0",
        "title": "Legacy Plan",
        "objective": "Complete legacy plan context",
        "tasks": [
            { "step_id": "1.0", "description": "Legacy task prompt", "implemented": false }
        ]
    }"#,
    )
    .expect("legacy plan fixture should write");

    let plan = read_numbered_plan(&path).expect("legacy plan keys should remain readable");
    assert_eq!(plan.complete_plan, "Complete legacy plan context");
    assert_eq!(plan.tasks[0].task_prompt, "Legacy task prompt");

    super::unitcore::write_numbered_plan(&path, &plan)
        .expect("plan should write with canonical keys");
    let written = fs::read_to_string(&path).expect("canonical plan should read");
    assert!(written.contains("\"complete_plan\""));
    assert!(written.contains("\"task_prompt\""));
    assert!(!written.contains("\"objective\""));
    assert!(!written.contains("\"description\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn transition_step_projection_increments_last_numeric_segment() {
    assert_eq!(
        project_transition_step_id("1.10", 0).expect("projection should work"),
        "1.11"
    );
    assert_eq!(
        project_transition_step_id("1.15", 0).expect("projection should work"),
        "1.16"
    );
    assert_eq!(
        project_transition_step_id("1.2.9", 0).expect("projection should work"),
        "1.2.10"
    );
    assert_eq!(
        project_transition_step_id("1.10", 2).expect("projection should work"),
        "1.13"
    );
}

#[test]
fn step_id_parser_rejects_malformed_transition_boundaries() {
    assert!(parse_step_id_segments("").is_err());
    assert!(parse_step_id_segments("1.x").is_err());
    assert!(parse_step_id_segments("1..2").is_err());
    assert!(parse_step_id_segments("1.2a").is_err());
}

#[test]
fn transition_task_fixture_loads_stable_source_ids() {
    let root = unique_temp_dir("ac_ant_delirium_transition_fixture_loads");
    let path = root.join("transition_task.json");
    fs::write(
        &path,
        r#"{
        "transition_tasks": [
            {
                "transition_task_id": "1.0",
                "task_prompt": "Inspect active task-owned background work.",
                "task_reviewed": true,
                "implemented": false,
                "implementation_context": "Only inspect registered task-owned sessions."
            },
            {
                "transition_task_id": "1.1",
                "description": "Close task-owned sessions that are still active.",
                "task_reviewed": true,
                "implemented": false,
                "implementation_context": "Do not close unrelated sessions."
            }
        ]
    }"#,
    )
    .expect("transition fixture should write");

    let tasks = read_transition_task_definitions(&path).expect("transition fixture should parse");

    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].step_id, "1.0");
    assert_eq!(tasks[1].step_id, "1.1");
    assert_eq!(
        tasks[0].task_prompt,
        "Inspect active task-owned background work."
    );
    assert_eq!(
        tasks[1].task_prompt,
        "Close task-owned sessions that are still active."
    );
    assert!(tasks[0].task_reviewed);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn default_transition_tasks_are_self_contained_and_support_continuous_cleanup() {
    let root = unique_temp_dir("ac_ant_delirium_default_transition_prompts");
    let path = root.join("transition_task.json");
    fs::write(&path, super::unitcore::DEFAULT_TRANSITION_TASK_JSON)
        .expect("default transition task fixture should write");

    let raw = fs::read_to_string(&path).expect("default transition task fixture should read");
    let tasks = read_transition_task_definitions(&path)
        .expect("default transition task fixture should parse");

    assert_eq!(tasks.len(), 4);
    assert_eq!(tasks[0].step_id, "1.0");
    assert_eq!(tasks[3].step_id, "1.3");
    assert!(!raw.contains("\"description\""));
    assert!(tasks.iter().all(|task| task.task_prompt.contains(
        "Stay in the project directory /home/dracker/AntigravityProjects/playground/AutoBrowser/"
    )));
    assert!(tasks[0]
        .task_prompt
        .contains("Do not scan the entire operating system"));
    assert!(tasks[0]
        .task_prompt
        .contains("no-op result and continue immediately"));
    assert!(tasks[1]
        .task_prompt
        .contains("Gracefully stop task-owned background commands"));
    assert!(tasks[1].task_prompt.contains("Never use sudo"));
    assert!(tasks[2].task_prompt.contains("Do not delete source code"));
    assert!(tasks[3]
        .task_prompt
        .contains("must not pause continuous task execution"));
    assert!(tasks
        .iter()
        .all(|task| !task.task_prompt.contains("plugins/")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn transition_task_fixture_rejects_duplicate_source_ids() {
    let root = unique_temp_dir("ac_ant_delirium_transition_fixture_duplicates");
    let path = root.join("transition_task.json");
    fs::write(
        &path,
        r#"{
        "transition_tasks": [
            { "transition_task_id": "1.0", "task_prompt": "First" },
            { "transition_task_id": "1.0", "task_prompt": "Second" }
        ]
    }"#,
    )
    .expect("transition fixture should write");

    let err = read_transition_task_definitions(&path)
        .expect_err("duplicate source ids should be rejected");

    assert!(
        err.to_string().contains("duplicate transition task id"),
        "unexpected error: {}",
        err
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn transition_task_fixture_rejects_empty_task_list() {
    let root = unique_temp_dir("ac_ant_delirium_transition_fixture_empty");
    let path = root.join("transition_task.json");
    fs::write(&path, r#"{ "transition_tasks": [] }"#).expect("transition fixture should write");

    let err = read_transition_task_definitions(&path)
        .expect_err("empty transition fixture should be rejected");

    assert!(
        err.to_string().contains("contains no tasks"),
        "unexpected error: {}",
        err
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn projected_transition_task_uses_visible_continuation_id() {
    let root = unique_temp_dir("ac_ant_delirium_projected_transition_task");
    let plugin = temp_plugin_context(&root, "Aesthetic-Red", "Aesthetic-Red", "Aesthetic-Red");
    let transition_dir = plugin.plugin_dir.join("Global");
    fs::create_dir_all(&transition_dir).expect("transition dir should exist");
    fs::write(
        transition_dir.join("transition_task.json"),
        r#"{
        "transition_tasks": [
            {
                "transition_task_id": "1.0",
                "task_prompt": "Inspect active background sessions.",
                "implemented": true,
                "review_status": "approved"
            }
        ]
    }"#,
    )
    .expect("transition task file should write");

    let state = TransitionTaskState {
        active: true,
        source_plan: "../plugins/Aesthetic-Red/plan/widgets/[1.29]_Plan___Content_Widget.json"
            .to_string(),
        source_plan_id: "1.29".to_string(),
        final_step_id: "1.29.8".to_string(),
        transition_source: "../plugins/Aesthetic-Red/Global/transition_task.json".to_string(),
        current_transition_id: "1.0".to_string(),
        current_projected_step_id: "1.29.9".to_string(),
        completed_transition_ids: Vec::new(),
    };

    let task = projected_transition_task(&plugin, &state)
        .expect("projected transition task should read")
        .expect("projected transition task should exist");

    assert_eq!(task.step_id, "1.29.9");
    assert_eq!(task.task_prompt, "Inspect active background sessions.");
    assert!(!task.implemented);
    assert_eq!(task.review_status, "none");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn transition_activation_delivers_projected_task_through_normal_api_and_manifest() {
    let root = unique_temp_dir("ac_ant_delirium_transition_activation_api");
    let plugin = temp_plugin_context(&root, "Aesthetic-Red", "Aesthetic-Red", "Aesthetic-Red");
    let plan_path = plugin.workspace_path.join("[1.0]_test_plan.json");
    let transition_dir = plugin.plugin_dir.join("Global");
    let output_dir = root.join("manifest");
    fs::create_dir_all(&transition_dir).expect("transition dir should exist");
    fs::create_dir_all(&output_dir).expect("manifest dir should exist");

    fs::write(&plan_path, r#"{
        "plan_id": "1.0",
        "title": "Test Plan",
        "complete_plan": "Test",
        "tasks": [
            { "step_id": "1.1", "task_prompt": "First real task", "task_reviewed": true, "implemented": true, "review_status": "none" },
            { "step_id": "1.2", "task_prompt": "Last real task", "task_reviewed": true, "implemented": true, "review_status": "none" }
        ]
    }"#).expect("plan should write");
    fs::write(
        transition_dir.join("transition_task.json"),
        super::unitcore::DEFAULT_TRANSITION_TASK_JSON,
    )
    .expect("transition task file should write");

    let payload = TaskOrchestratorPayload {
        orchestration: TaskOrchestration {
            active_plan: "[1.0]_test_plan.json".to_string(),
            current_step_id: "1.2".to_string(),
            mode: "implementation".to_string(),
            grade: 0.0,
            task_start_timestamp: 0,
        },
        transition: None,
    };
    write_task_orchestrator(&plugin.task_orchestrator_path(), &payload)
        .expect("orchestrator should write");

    let advance = handle_task_api(&plugin, "hierarchy.task.[1.0].needs_more_tasks=false")
        .expect("task api should advance");
    assert!(
        advance.contains("Advanced to next task [1.3] within current plan."),
        "unexpected advance output: {}",
        advance
    );

    let orchestrator = read_task_orchestrator(&plugin.task_orchestrator_path())
        .expect("orchestrator should read")
        .expect("orchestrator should parse");
    let transition = orchestrator
        .transition
        .as_ref()
        .expect("transition should be active");
    assert!(transition.active);
    assert_eq!(transition.current_transition_id, "1.0");
    assert_eq!(transition.current_projected_step_id, "1.3");
    assert_eq!(orchestrator.orchestration.current_step_id, "1.3");

    let task_list =
        handle_task_api(&plugin, "hierarchy.task.[1.0]").expect("task list should render");
    assert!(task_list.contains("[1.3] Stay in the project directory"));
    assert!(task_list.contains("no-op result and continue immediately"));
    assert!(task_list.contains("Do not scan the entire operating system"));
    assert!(task_list.contains("Implemented: false, Review Status: none"));
    assert!(!task_list.contains("[1.0] Stay in the project directory"));
    assert!(!task_list.contains("File Path:"));
    assert!(!task_list.contains("plugins/"));

    write_todo_manifest(&plugin, &output_dir, None).expect("manifest should write");
    let manifest = fs::read_to_string(output_dir.join("to-do_list_manifest.json"))
        .expect("manifest should read");
    assert!(manifest.contains("\"step_id\": \"1.3\""));
    assert!(manifest.contains("\"complete_plan\": \"Test\""));
    assert!(manifest.contains("\"task_prompt\": \"Stay in the project directory"));
    assert!(manifest.contains("no-op result and continue immediately"));
    assert!(manifest.contains("Do not scan the entire operating system"));
    assert!(manifest.contains("\"implemented\": false"));
    assert!(manifest.contains("\"review_status\": \"none\""));
    assert!(!manifest.contains("transition_task_id"));
    assert!(!manifest.contains("\"global_plan\""));
    assert!(!manifest.contains("\"description\""));

    let state_root = plugin.state_root();
    let database = ProjectDatabase::initialize(&state_root).expect("database should initialize");
    CanonicalStateStore::register_project(
        &database,
        &StorageProject {
            project_id: "prj_00000000000000000000000000".to_string(),
            project_key: "transition-projection-project".to_string(),
            created_at: "2026-08-11T00:00:00Z".to_string(),
        },
    )
    .expect("project should register");
    CanonicalStateStore::register_workspace(
        &database,
        &StorageWorkspace {
            workspace_id: "wsp_00000000000000000000000000".to_string(),
            project_id: "prj_00000000000000000000000000".to_string(),
            workspace_key: "transition-projection-workspace".to_string(),
            created_at: "2026-08-11T00:00:00Z".to_string(),
        },
    )
    .expect("workspace should register");
    let writer = AtomicProjectProjectionWriter::new(database, state_root.clone())
        .expect("writer should bind to the state root");
    let context = TodoProjectionContext {
        metadata: ProjectionMetadata {
            project_id: "prj_00000000000000000000000000".to_string(),
            workspace_id: "wsp_00000000000000000000000000".to_string(),
            source_revision: 0,
            generation: 1,
            fingerprint: format!("{:064x}", 71),
            rendered_at: "2026-08-11T00:00:00Z".to_string(),
        },
        task_ref: Some(CanonicalTaskProjectionRef {
            task_id: "tsk_00000000000000000000000000".to_string(),
            task_revision: 1,
        }),
    };
    let audit_event = NewAuditEvent {
        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
        event_id: "evt_00000000000000000000000000".to_string(),
        project_id: context.metadata.project_id.clone(),
        workspace_id: Some(context.metadata.workspace_id.clone()),
        actor_kind: "service".to_string(),
        actor_subject_id: "antdelirium-service".to_string(),
        event_kind: "projection_rendered".to_string(),
        outcome: "succeeded".to_string(),
        occurred_at: "2026-08-11T00:00:00Z".to_string(),
        fingerprint: context.metadata.fingerprint.clone(),
        correlation_id: "transition-projection".to_string(),
        resource_id: Some("current_task".to_string()),
        payload: b"current-task-projection".to_vec(),
    };
    let receipt = write_todo_manifest(
        &plugin,
        &output_dir,
        Some(TodoManifestProjectionTarget {
            writer: &writer,
            context: &context,
            audit_event: &audit_event,
        }),
    )
    .expect("typed projection should write")
    .expect("projection receipt should exist");
    assert!(receipt.audit.inserted());
    let projection = fs::read_to_string(state_root.todo_projection_path())
        .expect("current task projection should read");
    assert!(projection.contains("\"task_id\": \"tsk_00000000000000000000000000\""));
    assert!(projection.contains("Stay in the project directory"));
    assert!(projection.contains("no-op result and continue immediately"));
    assert!(projection.contains("\"allowed_action\": \"implement\""));
    assert!(projection.contains("hierarchy.task.current.submit"));
    for forbidden in [
        "\"plan_id\"",
        "\"complete_plan\"",
        "\"implemented\"",
        "\"task_reviewed\"",
        "\"review_status\"",
        "plugins/",
        "/home/",
        "task_orchestrator.json",
    ] {
        assert!(
            !projection.contains(forbidden),
            "forbidden projection content leaked: {forbidden}"
        );
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn transition_completion_marks_internal_source_id_without_exposing_it() {
    let mut payload = TaskOrchestratorPayload {
        orchestration: TaskOrchestration {
            active_plan: "../plugins/Aesthetic-Red/plan/widgets/[1.29]_Plan___Content_Widget.json"
                .to_string(),
            current_step_id: "1.29.9".to_string(),
            mode: "implementation".to_string(),
            grade: 0.0,
            task_start_timestamp: 0,
        },
        transition: Some(TransitionTaskState {
            active: true,
            source_plan: "../plugins/Aesthetic-Red/plan/widgets/[1.29]_Plan___Content_Widget.json"
                .to_string(),
            source_plan_id: "1.29".to_string(),
            final_step_id: "1.29.8".to_string(),
            transition_source: "../plugins/Aesthetic-Red/Global/transition_task.json".to_string(),
            current_transition_id: "1.0".to_string(),
            current_projected_step_id: "1.29.9".to_string(),
            completed_transition_ids: Vec::new(),
        }),
    };

    mark_current_transition_task_implemented(&mut payload, "1.29.9")
        .expect("projected transition task should mark implemented");

    let transition = payload.transition.expect("transition state should remain");
    assert_eq!(transition.completed_transition_ids, vec!["1.0".to_string()]);
    assert_eq!(transition.current_projected_step_id, "1.29.9");
}

#[test]
fn projected_pending_count_adds_transition_template_only_for_live_flow() {
    let root = unique_temp_dir("ac_ant_delirium_transition_pending_count");
    let plugin = temp_plugin_context(&root, "Aesthetic-Red", "Aesthetic-Red", "Aesthetic-Red");
    let transition_dir = plugin.plugin_dir.join("Global");
    fs::create_dir_all(&transition_dir).expect("transition dir should exist");
    fs::write(
        transition_dir.join("transition_task.json"),
        r#"{
        "transition_tasks": [
            { "transition_task_id": "1.0", "task_prompt": "Inspect active background sessions." },
            { "transition_task_id": "1.1", "task_prompt": "Close task-owned background sessions." },
            { "transition_task_id": "1.2", "task_prompt": "Release task-owned resources." },
            { "transition_task_id": "1.3", "task_prompt": "Record cleanup evidence." }
        ]
    }"#,
    )
    .expect("transition task file should write");

    let implementation_payload = TaskOrchestratorPayload {
        orchestration: TaskOrchestration {
            active_plan:
                "../plugins/Aesthetic-Red/plan/widgets/[1.36]_Plan___TextBlock_Widget.json"
                    .to_string(),
            current_step_id: "1.36.1".to_string(),
            mode: "implementation".to_string(),
            grade: 0.0,
            task_start_timestamp: 0,
        },
        transition: None,
    };

    assert_eq!(
        transition_task_template_count(&plugin).expect("transition count should read"),
        4
    );
    assert_eq!(
        projected_implementation_pending_count(&plugin, &implementation_payload, "1.36", 4)
            .expect("pending count should project"),
        8
    );
    assert_eq!(
        projected_implementation_pending_count(&plugin, &implementation_payload, "1.35", 0)
            .expect("completed old plan should stay complete"),
        0
    );

    let active_transition_payload = TaskOrchestratorPayload {
        orchestration: TaskOrchestration {
            active_plan:
                "../plugins/Aesthetic-Red/plan/widgets/[1.36]_Plan___TextBlock_Widget.json"
                    .to_string(),
            current_step_id: "1.36.6".to_string(),
            mode: "implementation".to_string(),
            grade: 0.0,
            task_start_timestamp: 0,
        },
        transition: Some(TransitionTaskState {
            active: true,
            source_plan:
                "../plugins/Aesthetic-Red/plan/widgets/[1.36]_Plan___TextBlock_Widget.json"
                    .to_string(),
            source_plan_id: "1.36".to_string(),
            final_step_id: "1.36.4".to_string(),
            transition_source: "../plugins/Aesthetic-Red/Global/transition_task.json".to_string(),
            current_transition_id: "1.1".to_string(),
            current_projected_step_id: "1.36.6".to_string(),
            completed_transition_ids: vec!["1.0".to_string()],
        }),
    };

    assert_eq!(
        projected_implementation_pending_count(&plugin, &active_transition_payload, "1.36", 0)
            .expect("active transition should count remaining tasks"),
        3
    );

    let review_payload = TaskOrchestratorPayload {
        orchestration: TaskOrchestration {
            active_plan:
                "../plugins/Aesthetic-Red/plan/widgets/[1.36]_Plan___TextBlock_Widget.json"
                    .to_string(),
            current_step_id: "1.36.1".to_string(),
            mode: "implementation review".to_string(),
            grade: 0.0,
            task_start_timestamp: 0,
        },
        transition: None,
    };

    assert_eq!(
        projected_implementation_pending_count(&plugin, &review_payload, "1.36", 4)
            .expect("review mode should not include transition tasks"),
        4
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn render_next_action_prints_explicit_instructions_in_review_mode() {
    let root = unique_temp_dir("ac_ant_delirium_review_mode_action");
    let plugin = temp_plugin_context(
        &root,
        "AC_Rayon-Genetics",
        "AC_Rayon-Genetics",
        "Ant-Delirium-Agent-1_RAYON",
    );

    sync_agent_registration(&plugin).unwrap().unwrap();
    let mut agents = read_connected_agents(&plugin.connected_path()).unwrap();
    let mut registered = agents.pop().unwrap();
    registered.assigned_mission = "manager:TestTask []".to_string();
    write_connected_agents(&plugin.connected_path(), &[registered.clone()]).unwrap();

    write_manager_tasks(
        &plugin.manager_tasks_path(),
        &[ManagerTask {
            id: "TestTask".to_string(),
            title: "Test Task".to_string(),
            body: "Test Body".to_string(),
            file_path: "".to_string(),
            priority: 100,
            owner_id: registered.internal_id,
            status: "claimed".to_string(),
            created_at: 1,
            mode: Some("review".to_string()),
            grade: None,
        }],
    )
    .unwrap();

    let report = render_next_action(&plugin, false).unwrap();
    assert!(report.contains("[REVIEW MODE ACTIVE]"));
    assert!(report.contains("The goal is to solve the task, not just check the box as done"));

    let _ = fs::remove_dir_all(root);
}
