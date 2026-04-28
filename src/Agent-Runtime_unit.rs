use std::collections::{hash_map::DefaultHasher, BTreeMap, HashSet};
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ant_delirium::structs::{
    AuditResult, ConnectedAgent, GovernanceTask, HierarchyConfig, JsonEntry, JsonValue,
    ManagerTask, PluginContext, RuntimeCommandContext, SubmitRecord,
};

const DEFAULT_NEW_PROJECT_HIERARCHY: &str =
    include_str!("../get_toml_for_new/default_hierarchy.toml");
const DEFAULT_NEW_PROJECT_RULES: &str = include_str!("../get_toml_for_new/default_rules.toml");
const DEFAULT_NEW_PROJECT_BUILD_GUARD: &str =
    include_str!("../get_toml_for_new/ant_delirium_build_guard.rs");

pub fn discover_runtime_context(
    project_root: &Path,
    explicit_config_path: Option<&str>,
    argv0: Option<&str>,
) -> RuntimeCommandContext {
    if let Some(raw_config) = explicit_config_path {
        let resolved = resolve_cli_path(project_root, raw_config);
        let hierarchy = load_hierarchy_from_path(&resolved);
        let plugin = build_plugin_context_from_config(project_root, &resolved);
        let hint = hierarchy_hint_path(project_root, &resolved);
        return RuntimeCommandContext {
            plugin,
            hierarchy: Some(hierarchy),
            hierarchy_hint_path: Some(hint),
        };
    }

    if let Some(plugin) = discover_plugin_context(project_root, argv0) {
        let hierarchy = load_hierarchy_from_path(&plugin.config_path);
        let hint = hierarchy_hint_path(project_root, &plugin.config_path);
        return RuntimeCommandContext {
            plugin: Some(plugin),
            hierarchy: Some(hierarchy),
            hierarchy_hint_path: Some(hint),
        };
    }

    RuntimeCommandContext::default()
}

pub fn sync_agent_registration(plugin: &PluginContext) -> io::Result<Option<ConnectedAgent>> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        write_context_snapshot(plugin)?;

        let mut agents = read_connected_agents(&plugin.connected_path())?;
        let mut tasks = read_governance_tasks(&plugin.governance_path())?;
        let mut manager_tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
        let now = unix_timestamp();
        normalize_agent_topology(plugin, &mut agents, &mut tasks, &mut manager_tasks, now);
        ensure_agent_ids(&mut agents);
        ensure_agent_branches(&mut agents);
        ensure_agent_revisions(plugin, &mut agents);

        if let Some(existing_idx) = agents.iter().position(|agent| {
            normalize_path_str(&agent.worktree_path)
                == normalize_path(plugin.workspace_path.as_path())
        }) {
            let existing_agent_id = agents[existing_idx].agent_id.clone();
            let resolved_agent_id = resolve_persisted_agent_id(
                &existing_agent_id,
                plugin.workspace_path.as_path(),
                plugin.login_path().as_path(),
                &agents,
            );
            let persisted_sync = sync_metadata_from_snapshot(read_workspace_login_snapshot(
                plugin.login_path().as_path(),
            ));
            let existing = &mut agents[existing_idx];
            existing.last_ping = now;
            existing.agent_id = resolved_agent_id;
            apply_sync_metadata(existing, &persisted_sync);
            existing.login_path = plugin.login_path().to_string_lossy().to_string();
            if existing.internal_id == 0 {
                existing.assigned_mission = "global".to_string();
            } else if let Some(task) =
                claim_manager_task_for_agent(&mut manager_tasks, existing.internal_id)
            {
                existing.assigned_mission = manager_task_mission(&task);
            } else if let Some(task) = tasks
                .iter()
                .find(|task| task.owner_id == existing.internal_id)
            {
                existing.assigned_mission = task.file_path.clone();
            } else {
                existing.assigned_mission.clear();
            }
            let existing_snapshot = existing.clone();
            write_connected_agents(&plugin.connected_path(), &agents)?;
            write_manager_tasks(&plugin.manager_tasks_path(), &manager_tasks)?;
            write_workspace_login(plugin, &existing_snapshot)?;
            return Ok(Some(existing_snapshot));
        }

        let next_id = agents
            .iter()
            .map(|agent| agent.internal_id)
            .max()
            .unwrap_or(0)
            + 1;
        let assigned_mission =
            if let Some(task) = claim_manager_task_for_agent(&mut manager_tasks, next_id) {
                manager_task_mission(&task)
            } else {
                assign_governance_task(&mut tasks, next_id).unwrap_or_default()
            };
        let persisted_sync = sync_metadata_from_snapshot(read_workspace_login_snapshot(
            plugin.login_path().as_path(),
        ));
        let new_agent = ConnectedAgent {
            internal_id: next_id,
            agent_id: generate_unique_agent_id(&agents, plugin.workspace_path.as_path()),
            branch: persisted_sync.branch,
            base_revision: persisted_sync.base_revision,
            workspace_revision: persisted_sync.workspace_revision,
            repository_revision: persisted_sync.repository_revision,
            worktree_path: plugin.workspace_path.to_string_lossy().to_string(),
            login_path: plugin.login_path().to_string_lossy().to_string(),
            assigned_mission,
            last_ping: now,
        };
        agents.push(new_agent.clone());

        write_connected_agents(&plugin.connected_path(), &agents)?;
        write_governance_tasks(&plugin.governance_path(), &tasks)?;
        write_manager_tasks(&plugin.manager_tasks_path(), &manager_tasks)?;
        write_workspace_login(plugin, &new_agent)?;
        Ok(Some(new_agent))
    })
}

pub fn render_agents_report(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, tasks, manager_tasks, messages) = load_runtime_state(plugin)?;
        let feedback = load_feedback_entries(plugin)?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;

        if json_requested {
            return Ok(render_agents_report_json(
                plugin,
                &agents,
                &tasks,
                &manager_tasks,
                &messages,
                &feedback,
                &submit_queue,
            ));
        }

        let projects = summarize_projects(&agents);
        let mut out = String::new();
        out.push_str("Swarm Status\n\n");
        out.push_str(&format!(
            "Plugin dir: {}\nRepository root: {}\nWorkspace path: {}\nExecution context: {}\n\n",
            plugin.plugin_dir.to_string_lossy(),
            plugin.repository_path.to_string_lossy(),
            plugin.workspace_path.to_string_lossy(),
            plugin_execution_context(plugin),
        ));
        out.push_str(&format!(
            "Active agents: {}\nProjects: {}\nOpen governance tasks: {}\nOpen manager tasks: {}\nReady submits: {}\nPending submits: {}\nBlocked submits: {}\nConsumed submits: {}\nOpen submit records: {}\nAccepted submit records: {}\nRejected submit records: {}\nClosed submit records: {}\nSuperseded submit records: {}\n\n",
            agents
                .iter()
                .filter(|agent| unix_timestamp() - agent.last_ping <= 3600)
                .count(),
            projects.len(),
            tasks.iter().filter(|task| task.owner_id < 0).count(),
            manager_tasks
                .iter()
                .filter(|task| task.owner_id < 0 && is_manager_task_active(task))
                .count(),
            agents
                .iter()
                .filter(|agent| {
                    matches!(
                        resolve_agent_submit_state(
                            &agents,
                            agent,
                            &messages,
                            &feedback,
                            &submit_queue
                        )
                        .submit_state
                        .as_str(),
                        "ready_for_root"
                    )
                })
                .count(),
            agents
                .iter()
                .filter(|agent| {
                    matches!(
                        resolve_agent_submit_state(
                            &agents,
                            agent,
                            &messages,
                            &feedback,
                            &submit_queue
                        )
                            .submit_state
                            .as_str(),
                        "message_only" | "feedback_only"
                    )
                })
                .count(),
            agents
                .iter()
                .filter(|agent| {
                    matches!(
                        resolve_agent_submit_state(
                            &agents,
                            agent,
                            &messages,
                            &feedback,
                            &submit_queue
                        )
                        .submit_state
                        .as_str(),
                        "blocked_rebase"
                    )
                })
                .count(),
            agents
                .iter()
                .filter(|agent| {
                    matches!(
                        resolve_agent_submit_state(
                            &agents,
                            agent,
                            &messages,
                            &feedback,
                            &submit_queue
                        )
                        .submit_state
                        .as_str(),
                        "consumed"
                    )
                })
                .count(),
            submit_queue
                .iter()
                .filter(|record| record.status == "open")
                .count(),
            submit_queue
                .iter()
                .filter(|record| record.status == "accepted")
                .count(),
            submit_queue
                .iter()
                .filter(|record| record.status == "rejected")
                .count(),
            submit_queue
                .iter()
                .filter(|record| record.status == "closed")
                .count(),
            submit_queue
                .iter()
                .filter(|record| record.status == "superseded")
                .count()
        ));
        out.push_str("Projects:\n");
        if projects.is_empty() {
            out.push_str("- none\n\n");
        } else {
            for project in &projects {
                out.push_str(&format!(
                    "- repository={} agents={} active_agents={} worktrees={}\n",
                    project.repository_path,
                    project.agent_count,
                    project.active_agent_count,
                    project.worktree_count,
                ));
            }
            out.push('\n');
        }
        out.push_str("Agents:\n");
        if agents.is_empty() {
            out.push_str("- none\n");
            return Ok(out);
        }

        let now = unix_timestamp();
        for agent in &agents {
            let owned = agent_governance_scope(plugin, &agent, &tasks);
            let owned_manager: Vec<&ManagerTask> = manager_tasks
                .iter()
                .filter(|task| task.owner_id == agent.internal_id && is_manager_task_active(task))
                .collect();
            let manager_override = !owned_manager.is_empty();
            let pending_messages = count_pending_messages_for_agent(agent.internal_id, &messages);
            let violations: i64 = owned.iter().map(|task| task.violation_count).sum();
            let sync = resolve_agent_sync_state(&agents, &agent);
            let submit =
                resolve_agent_submit_state(&agents, &agent, &messages, &feedback, &submit_queue);
            let status = if now - agent.last_ping <= 3600 {
                "active"
            } else {
                "stale"
            };
            out.push_str(&format!(
                "- agent {} ({}) [{}] workspace={} files={} violations={} pending_messages={} mission={}\n",
                agent.internal_id,
                agent.agent_id,
                status,
                agent.worktree_path,
                owned.len(),
                violations,
                pending_messages,
                empty_or_dash(&agent.assigned_mission),
            ));
            out.push_str(&format!("  agent_id={}\n", empty_or_dash(&agent.agent_id),));
            out.push_str(&format!("  branch={}\n", empty_or_dash(&agent.branch),));
            out.push_str(&format!(
                "  sync_state={} rebase_required={}\n",
                sync.sync_state,
                if sync.rebase_required { "yes" } else { "no" }
            ));
            out.push_str(&format!(
                "  submit_state={} submit_queue_status={} submit_message={} submit_feedback={}\n",
                submit.submit_state,
                empty_or_dash(&submit.submit_queue_status),
                if submit.has_submit_message {
                    "yes"
                } else {
                    "no"
                },
                if submit.has_submit_feedback {
                    "yes"
                } else {
                    "no"
                }
            ));
            out.push_str(&format!(
                "  manager_override={} active_manager_tasks={} queued_violation_files={} queued_violations={}\n",
                if manager_override { "yes" } else { "no" },
                owned_manager.len(),
                owned.len(),
                violations
            ));
            out.push_str(&format!(
                "  base_revision={} workspace_revision={} repository_revision={}\n",
                empty_or_dash(&sync.base_revision),
                empty_or_dash(&sync.workspace_revision),
                empty_or_dash(&sync.repository_revision),
            ));
            out.push_str(&format!(
                "  login_path={}\n",
                empty_or_dash(&agent.login_path),
            ));
            out.push_str(&format!(
                "  execution_context={}\n",
                agent_execution_context(plugin, &agent)
            ));
            out.push_str(&format!(
                "  root_scope={}\n",
                if is_root_agent_context(plugin, &agent) {
                    "yes"
                } else {
                    "no"
                }
            ));
            if !owned_manager.is_empty() {
                out.push_str("  priority_backlog:\n");
            }
            for task in owned_manager {
                out.push_str(&format!(
                    "    manager-task={} priority={} file={} status={}\n",
                    task.title,
                    task.priority,
                    empty_or_dash(&task.file_path),
                    task.status
                ));
            }
            if !owned.is_empty() {
                out.push_str("  governance_backlog:\n");
            }
            for task in owned {
                out.push_str(&format!(
                    "    file={} violations={}\n",
                    task.file_path, task.violation_count
                ));
            }
        }

        Ok(out)
    })
}

pub fn render_projects_report(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, _tasks, _manager_tasks, _messages) = load_runtime_state(plugin)?;
        let projects = summarize_projects(&agents);

        if json_requested {
            let mut out = String::new();
            out.push_str("{\n");
            out.push_str("  \"command\": \"projects\",\n");
            out.push_str(&format!("  \"project_count\": {},\n", projects.len()));
            out.push_str("  \"projects\": [\n");
            for (idx, project) in projects.iter().enumerate() {
                let comma = if idx + 1 < projects.len() { "," } else { "" };
                out.push_str(&format!(
                    "    {{ \"repository_path\": \"{}\", \"agent_count\": {}, \"active_agent_count\": {}, \"worktree_count\": {} }}{}\n",
                    json_escape(&project.repository_path),
                    project.agent_count,
                    project.active_agent_count,
                    project.worktree_count,
                    comma
                ));
            }
            out.push_str("  ]\n");
            out.push_str("}\n");
            return Ok(out);
        }

        let mut out = String::new();
        out.push_str("Projects\n\n");
        out.push_str(&format!("Project count: {}\n\n", projects.len()));
        if projects.is_empty() {
            out.push_str("- none\n");
            return Ok(out);
        }
        for project in projects {
            out.push_str(&format!(
                "- repository={} agents={} active_agents={} worktrees={}\n",
                project.repository_path,
                project.agent_count,
                project.active_agent_count,
                project.worktree_count,
            ));
        }
        Ok(out)
    })
}

pub fn render_manager_tasks(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let tasks = read_manager_tasks(&plugin.manager_tasks_path())?;

        if json_requested {
            let mut out = String::new();
            out.push_str("{\n");
            out.push_str("  \"command\": \"manager_tasks\",\n");
            out.push_str(&format!("  \"count\": {},\n", tasks.len()));
            out.push_str("  \"tasks\": [\n");
            for (idx, task) in tasks.iter().enumerate() {
                let comma = if idx + 1 < tasks.len() { "," } else { "" };
                out.push_str(&format!(
                    "    {{ \"id\": \"{}\", \"title\": \"{}\", \"body\": \"{}\", \"file_path\": \"{}\", \"priority\": {}, \"owner_id\": {}, \"status\": \"{}\", \"created_at\": {} }}{}\n",
                    json_escape(&task.id),
                    json_escape(&task.title),
                    json_escape(&task.body),
                    json_escape(&task.file_path),
                    task.priority,
                    task.owner_id,
                    json_escape(&task.status),
                    task.created_at,
                    comma
                ));
            }
            out.push_str("  ]\n");
            out.push_str("}\n");
            return Ok(out);
        }

        let mut out = String::new();
        out.push_str("Manager Tasks\n\n");
        out.push_str(&format!("Count: {}\n\n", tasks.len()));
        if tasks.is_empty() {
            out.push_str("- none\n");
            return Ok(out);
        }
        for task in tasks {
            out.push_str(&format!(
                "- id={} priority={} owner={} status={} title={} file={}\n",
                task.id,
                task.priority,
                task.owner_id,
                task.status,
                task.title,
                empty_or_dash(&task.file_path)
            ));
            if !task.body.trim().is_empty() {
                out.push_str(&format!("  body={}\n", task.body));
            }
        }
        Ok(out)
    })
}

pub fn render_submit_queue(
    plugin: &PluginContext,
    json_requested: bool,
    filter: Option<&str>,
    submit_id: Option<&str>,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let entries = read_submit_records(&plugin.submit_queue_path())?;
        let view = filter.unwrap_or("all");
        let filtered: Vec<SubmitRecord> = if let Some(id) = submit_id.map(str::trim).filter(|value| !value.is_empty()) {
            entries
                .iter()
                .filter(|entry| entry.id == id)
                .cloned()
                .collect()
        } else {
            match view {
                "active" => entries
                    .iter()
                    .filter(|entry| entry.status == "open")
                    .cloned()
                    .collect(),
                "history" => entries
                    .iter()
                    .filter(|entry| entry.status != "open")
                    .cloned()
                    .collect(),
                _ => entries.clone(),
            }
        };

        if json_requested {
            let mut out = String::new();
            out.push_str("{\n");
            out.push_str("  \"command\": \"submit_queue\",\n");
            out.push_str(&format!("  \"view\": \"{}\",\n", json_escape(view)));
            out.push_str(&format!(
                "  \"submit_id\": \"{}\",\n",
                json_escape(submit_id.unwrap_or(""))
            ));
            out.push_str(&format!("  \"count\": {},\n", filtered.len()));
            out.push_str("  \"entries\": [\n");
            for (idx, entry) in filtered.iter().enumerate() {
                let comma = if idx + 1 < filtered.len() { "," } else { "" };
                out.push_str(&format!(
                    "    {{ \"id\": \"{}\", \"internal_id\": {}, \"agent_id\": \"{}\", \"title\": \"{}\", \"topic\": \"{}\", \"branch\": \"{}\", \"base_revision\": \"{}\", \"workspace_revision\": \"{}\", \"repository_revision\": \"{}\", \"status\": \"{}\", \"created_at\": {}, \"updated_at\": {} }}{}\n",
                    json_escape(&entry.id),
                    entry.internal_id,
                    json_escape(&entry.agent_id),
                    json_escape(&entry.title),
                    json_escape(&entry.topic),
                    json_escape(&entry.branch),
                    json_escape(&entry.base_revision),
                    json_escape(&entry.workspace_revision),
                    json_escape(&entry.repository_revision),
                    json_escape(&entry.status),
                    entry.created_at,
                    entry.updated_at,
                    comma
                ));
            }
            out.push_str("  ]\n");
            out.push_str("}\n");
            return Ok(out);
        }

        let mut out = String::new();
        out.push_str("Submit Queue\n\n");
        out.push_str(&format!(
            "View: {}\nSubmit ID: {}\nCount: {}\nOpen: {}\nAccepted: {}\nRejected: {}\nClosed: {}\nSuperseded: {}\n\n",
            view,
            empty_or_dash(submit_id.unwrap_or("")),
            filtered.len(),
            entries.iter().filter(|entry| entry.status == "open").count(),
            entries
                .iter()
                .filter(|entry| entry.status == "accepted")
                .count(),
            entries
                .iter()
                .filter(|entry| entry.status == "rejected")
                .count(),
            entries.iter().filter(|entry| entry.status == "closed").count(),
            entries
                .iter()
                .filter(|entry| entry.status == "superseded")
                .count(),
        ));
        if filtered.is_empty() {
            out.push_str("- none\n");
            return Ok(out);
        }
        for entry in filtered {
            out.push_str(&format!(
                "- id={} agent={} (# {}) status={} topic={} branch={} title={}\n",
                entry.id,
                entry.agent_id,
                entry.internal_id,
                entry.status,
                empty_or_dash(&entry.topic),
                empty_or_dash(&entry.branch),
                entry.title
            ));
        }
        Ok(out)
    })
}

pub fn update_submit_status(
    plugin: &PluginContext,
    submit_id: &str,
    status: &str,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        if !is_repository_root_context(plugin) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "submit status can only be updated from repository root",
            ));
        }
        let mut entries = read_submit_records(&plugin.submit_queue_path())?;
        let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.id == submit_id.trim())
        else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("submit {} was not found", submit_id),
            ));
        };
        entry.status = status.to_string();
        entry.updated_at = unix_timestamp();
        write_submit_records(&plugin.submit_queue_path(), &entries)?;
        Ok(format!("submit {} marked {}", submit_id.trim(), status))
    })
}

pub fn append_manager_task(
    plugin: &PluginContext,
    title: &str,
    body: Option<&str>,
    file_path: Option<&str>,
    priority: i64,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let mut tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
        let id = generate_manager_task_id(&tasks, title);
        tasks.push(ManagerTask {
            id: id.clone(),
            title: title.trim().to_string(),
            body: body.map(str::trim).unwrap_or("").to_string(),
            file_path: file_path.map(str::trim).unwrap_or("").to_string(),
            priority,
            owner_id: -1,
            status: "open".to_string(),
            created_at: unix_timestamp(),
        });
        write_manager_tasks(&plugin.manager_tasks_path(), &tasks)?;
        Ok(format!("manager task stored: {}", id))
    })
}

pub fn close_manager_task(plugin: &PluginContext, task_id: &str) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let mut tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
        let Some(task) = tasks.iter_mut().find(|task| task.id == task_id.trim()) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("manager task {} was not found", task_id),
            ));
        };
        task.status = "done".to_string();
        write_manager_tasks(&plugin.manager_tasks_path(), &tasks)?;
        Ok(format!("manager task marked done: {}", task_id.trim()))
    })
}

pub fn render_rebase_check(plugin: &PluginContext, json_requested: bool) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, _tasks, _manager_tasks, _messages) = load_runtime_state(plugin)?;
        let current_agent = resolve_current_agent(plugin, &agents).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "current workspace is not a reconciled agent",
            )
        })?;
        let sync = resolve_agent_sync_state(&agents, current_agent);

        if json_requested {
            let mut out = String::new();
            out.push_str("{\n");
            out.push_str("  \"command\": \"rebase_check\",\n");
            out.push_str(&format!(
                "  \"internal_id\": {},\n",
                current_agent.internal_id
            ));
            out.push_str(&format!(
                "  \"agent_id\": \"{}\",\n",
                json_escape(&current_agent.agent_id)
            ));
            out.push_str(&format!(
                "  \"branch\": \"{}\",\n",
                json_escape(&current_agent.branch)
            ));
            out.push_str(&format!(
                "  \"sync_state\": \"{}\",\n",
                json_escape(&sync.sync_state)
            ));
            out.push_str(&format!(
                "  \"rebase_required\": {},\n",
                if sync.rebase_required {
                    "true"
                } else {
                    "false"
                }
            ));
            out.push_str(&format!(
                "  \"repository_revision\": \"{}\",\n",
                json_escape(&sync.repository_revision)
            ));
            out.push_str(&format!(
                "  \"workspace_revision\": \"{}\",\n",
                json_escape(&sync.workspace_revision)
            ));
            out.push_str(&format!(
                "  \"base_revision\": \"{}\"\n",
                json_escape(&sync.base_revision)
            ));
            out.push_str("}\n");
            return Ok(out);
        }

        let mut out = String::new();
        out.push_str(&format!(
            "Rebase Check\n\nAgent: {} ({})\nBranch: {}\nSync state: {}\nRebase required: {}\nRepository revision: {}\nWorkspace revision: {}\nBase revision: {}\n",
            current_agent.internal_id,
            current_agent.agent_id,
            empty_or_dash(&current_agent.branch),
            sync.sync_state,
            if sync.rebase_required { "yes" } else { "no" },
            empty_or_dash(&sync.repository_revision),
            empty_or_dash(&sync.workspace_revision),
            empty_or_dash(&sync.base_revision),
        ));
        if sync.rebase_required {
            out.push_str(
                "\nPersisted metadata says the root changed after this agent base revision. Rebase this worktree outside the plugin and rerun the local audit before submit.\n",
            );
        } else if sync.sync_state == "unknown" {
            out.push_str(
                "\nSync metadata is incomplete. Provide --branch, --base-revision, --workspace-revision, and --repository-revision in a sync or submit message/feedback before relying on this status.\n",
            );
        } else {
            out.push_str(
                "\nPersisted metadata says this worktree already contains the current root revision or was created after the latest root change.\n",
            );
        }
        Ok(out)
    })
}

pub fn render_root_orchestration_violations(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<Option<String>> {
    if !is_repository_root_context(plugin) {
        return Ok(None);
    }

    with_runtime_lock(plugin, || {
        let findings = collect_root_orchestration_findings(plugin)?;
        if findings.is_empty() {
            return Ok(None);
        }

        if json_requested {
            let mut out = String::new();
            out.push_str("{\n");
            out.push_str("  \"command\": \"root_orchestration_checks\",\n");
            out.push_str("  \"ok\": false,\n");
            out.push_str(&format!("  \"count\": {},\n", findings.len()));
            out.push_str("  \"findings\": [\n");
            for (idx, finding) in findings.iter().enumerate() {
                let comma = if idx + 1 < findings.len() { "," } else { "" };
                out.push_str(&format!(
                    "    {{ \"code\": \"{}\", \"message\": \"{}\" }}{}\n",
                    json_escape(&finding.code),
                    json_escape(&finding.message),
                    comma
                ));
            }
            out.push_str("  ]\n");
            out.push_str("}\n");
            return Ok(Some(out));
        }

        let mut out = String::new();
        out.push_str("Root Orchestration Checks Failed\n\n");
        for finding in &findings {
            out.push_str(&format!("- {}: {}\n", finding.code, finding.message));
        }
        Ok(Some(out))
    })
}

pub fn render_current_agent_identity(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, tasks, manager_tasks, messages) = load_runtime_state(plugin)?;
        let feedback = load_feedback_entries(plugin)?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let current_agent = resolve_current_agent(plugin, &agents).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "current workspace is not a reconciled agent",
            )
        })?;
        Ok(render_single_agent_identity(
            plugin,
            &agents,
            current_agent,
            &tasks,
            &manager_tasks,
            &messages,
            &feedback,
            &submit_queue,
            json_requested,
        ))
    })
}

pub fn render_agent_identity(
    plugin: &PluginContext,
    agent_id: i64,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, tasks, manager_tasks, messages) = load_runtime_state(plugin)?;
        let feedback = load_feedback_entries(plugin)?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let agent = resolve_agent_by_id(&agents, agent_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("agent {} is not registered", agent_id),
            )
        })?;
        Ok(render_single_agent_identity(
            plugin,
            &agents,
            agent,
            &tasks,
            &manager_tasks,
            &messages,
            &feedback,
            &submit_queue,
            json_requested,
        ))
    })
}

pub fn render_agent_identity_by_agent_id(
    plugin: &PluginContext,
    agent_uid: &str,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, tasks, manager_tasks, messages) = load_runtime_state(plugin)?;
        let feedback = load_feedback_entries(plugin)?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let agent = resolve_agent_by_agent_id(&agents, agent_uid).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("agent {} is not registered", agent_uid),
            )
        })?;
        Ok(render_single_agent_identity(
            plugin,
            &agents,
            agent,
            &tasks,
            &manager_tasks,
            &messages,
            &feedback,
            &submit_queue,
            json_requested,
        ))
    })
}

pub fn refresh_governance_from_audit(
    plugin: &PluginContext,
    audit_result: &AuditResult,
) -> io::Result<Vec<GovernanceTask>> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let previous = read_governance_tasks(&plugin.governance_path())?;
        let owner_by_path: BTreeMap<String, i64> = previous
            .into_iter()
            .map(|task| (normalize_task_path(&task.file_path), task.owner_id))
            .collect();

        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        for violation in &audit_result.violations {
            let key = normalize_task_path(&violation.file_path);
            *counts.entry(key).or_insert(0) += 1;
        }

        let mut tasks: Vec<GovernanceTask> = counts
            .into_iter()
            .map(|(file_path, violation_count)| GovernanceTask {
                owner_id: owner_by_path.get(&file_path).copied().unwrap_or(-1),
                file_path,
                violation_count,
            })
            .collect();

        tasks.sort_by(|left, right| {
            right
                .violation_count
                .cmp(&left.violation_count)
                .then_with(|| left.file_path.cmp(&right.file_path))
        });

        write_governance_tasks(&plugin.governance_path(), &tasks)?;
        Ok(tasks)
    })
}

pub fn assign_task_to_current_agent_if_missing(
    plugin: &PluginContext,
) -> io::Result<Option<String>> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let mut agents = read_connected_agents(&plugin.connected_path())?;
        let mut tasks = read_governance_tasks(&plugin.governance_path())?;
        let mut manager_tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
        normalize_agent_topology(
            plugin,
            &mut agents,
            &mut tasks,
            &mut manager_tasks,
            unix_timestamp(),
        );
        let workspace = normalize_path(plugin.workspace_path.as_path());

        let Some(agent_idx) = agents
            .iter()
            .position(|agent| normalize_path_str(&agent.worktree_path) == workspace)
        else {
            return Ok(None);
        };

        let agent_id = agents[agent_idx].internal_id;
        if agent_id == 0 {
            if agents[agent_idx].assigned_mission != "global" {
                agents[agent_idx].assigned_mission = "global".to_string();
                agents[agent_idx].last_ping = unix_timestamp();
                write_connected_agents(&plugin.connected_path(), &agents)?;
                write_workspace_login(plugin, &agents[agent_idx])?;
            }
            write_governance_tasks(&plugin.governance_path(), &tasks)?;
            write_manager_tasks(&plugin.manager_tasks_path(), &manager_tasks)?;
            return Ok(Some("global".to_string()));
        }
        if let Some(task) = claim_manager_task_for_agent(&mut manager_tasks, agent_id) {
            let mission = manager_task_mission(&task);
            if agents[agent_idx].assigned_mission != mission {
                agents[agent_idx].assigned_mission = mission.clone();
                agents[agent_idx].last_ping = unix_timestamp();
                write_manager_tasks(&plugin.manager_tasks_path(), &manager_tasks)?;
                write_connected_agents(&plugin.connected_path(), &agents)?;
                write_workspace_login(plugin, &agents[agent_idx])?;
            }
            return Ok(Some(mission));
        }

        let owned = tasks.iter().find(|task| task.owner_id == agent_id);
        if let Some(task) = owned {
            if agents[agent_idx].assigned_mission != task.file_path {
                agents[agent_idx].assigned_mission = task.file_path.clone();
                write_connected_agents(&plugin.connected_path(), &agents)?;
                write_workspace_login(plugin, &agents[agent_idx])?;
            }
            return Ok(Some(task.file_path.clone()));
        }

        let Some(assigned) = assign_governance_task(&mut tasks, agent_id) else {
            if !agents[agent_idx].assigned_mission.trim().is_empty() {
                agents[agent_idx].assigned_mission.clear();
                agents[agent_idx].last_ping = unix_timestamp();
                write_connected_agents(&plugin.connected_path(), &agents)?;
                write_workspace_login(plugin, &agents[agent_idx])?;
            }
            return Ok(None);
        };
        agents[agent_idx].assigned_mission = assigned.clone();
        agents[agent_idx].last_ping = unix_timestamp();
        write_governance_tasks(&plugin.governance_path(), &tasks)?;
        write_connected_agents(&plugin.connected_path(), &agents)?;
        write_workspace_login(plugin, &agents[agent_idx])?;
        Ok(Some(assigned))
    })
}

pub fn isolate_audit_to_current_agent(
    plugin: &PluginContext,
    audit_result: &AuditResult,
) -> io::Result<AuditResult> {
    ensure_support_files(plugin)?;
    let agents = read_connected_agents(&plugin.connected_path())?;
    let tasks = read_governance_tasks(&plugin.governance_path())?;
    let manager_tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
    let workspace = normalize_path(plugin.workspace_path.as_path());
    let current_agent = agents
        .iter()
        .find(|agent| normalize_path_str(&agent.worktree_path) == workspace);

    let Some(agent) = current_agent else {
        return Ok(audit_result.clone());
    };

    if is_root_agent_context(plugin, agent) || is_repository_root_context(plugin) {
        let mut logs = audit_result.logs.clone();
        logs.push("[AGENT] repository root retains the full audit surface.".to_string());
        return Ok(AuditResult {
            violations: audit_result.violations.clone(),
            logs,
        });
    }

    if let Some(task) = manager_tasks
        .iter()
        .find(|task| task.owner_id == agent.internal_id && is_manager_task_active(task))
    {
        return Ok(AuditResult {
            violations: Vec::new(),
            logs: vec![format!(
                "[AGENT] normal audit suppressed while manager priority task is active: {}",
                task.title
            )],
        });
    }

    let owned_files: HashSet<String> = tasks
        .iter()
        .filter(|task| task.owner_id == agent.internal_id)
        .map(|task| normalize_task_path(&task.file_path))
        .collect();

    if owned_files.is_empty() {
        return Ok(AuditResult {
            violations: Vec::new(),
            logs: vec![format!(
                "[AGENT] workspace isolated for agent {} with no assigned files.",
                agent.internal_id
            )],
        });
    }

    let mut logs = audit_result.logs.clone();
    logs.push(format!(
        "[AGENT] isolating report for agent {} across {} file(s).",
        agent.internal_id,
        owned_files.len()
    ));

    Ok(AuditResult {
        violations: audit_result
            .violations
            .iter()
            .filter(|violation| owned_files.contains(&normalize_task_path(&violation.file_path)))
            .cloned()
            .collect(),
        logs,
    })
}

pub fn append_message(
    plugin: &PluginContext,
    target_agent_id: Option<i64>,
    target_agent_uid: Option<&str>,
    sender_agent_id_override: Option<i64>,
    sender_agent_uid_override: Option<&str>,
    topic: Option<&str>,
    operation_kind: Option<&str>,
    branch: Option<&str>,
    base_revision: Option<&str>,
    workspace_revision: Option<&str>,
    repository_revision: Option<&str>,
    body: &str,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (mut agents, _tasks, _manager_tasks, mut entries) = load_runtime_state(plugin)?;
        let current_agent = resolve_current_agent(plugin, &agents).cloned();
        let sender_agent = if let Some(agent_uid) = sender_agent_uid_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            resolve_agent_by_agent_id(&agents, agent_uid).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("sender agent {} is not registered", agent_uid),
                )
            })?
        } else if let Some(sender_agent_id) = sender_agent_id_override {
            resolve_agent_by_id(&agents, sender_agent_id).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("sender agent {} is not registered", sender_agent_id),
                )
            })?
        } else if let Some(agent) = current_agent.as_ref() {
            agent
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "current workspace is not a reconciled agent; use a valid --from-agent-id, --from-agent or login first",
            ));
        };
        let sender_agent = sender_agent.clone();
        let operation_kind = normalize_operation_kind(operation_kind);
        let cli_metadata = sync_metadata_from_cli(
            branch,
            base_revision,
            workspace_revision,
            repository_revision,
        );
        validate_operation_metadata("message", &operation_kind, &cli_metadata)?;
        let message_metadata = if sync_metadata_is_empty(&cli_metadata) {
            sync_metadata_from_agent(&sender_agent)
        } else {
            cli_metadata.clone()
        };
        let target_agent = if let Some(agent_uid) = target_agent_uid
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(
                resolve_agent_by_agent_id(&agents, agent_uid)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("target agent {} is not registered", agent_uid),
                        )
                    })?
                    .clone(),
            )
        } else if let Some(target_agent_id) = target_agent_id {
            if target_agent_id < 0 {
                None
            } else {
                Some(
                    resolve_agent_by_id(&agents, target_agent_id)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("target agent {} is not registered", target_agent_id),
                            )
                        })?
                        .clone(),
                )
            }
        } else {
            None
        };
        let target_internal_id = target_agent
            .as_ref()
            .map(|agent| agent.internal_id)
            .or(target_agent_id)
            .unwrap_or(-1);
        if let Some(current) = current_agent.as_ref() {
            if current.internal_id == sender_agent.internal_id {
                persist_current_agent_sync_metadata(
                    plugin,
                    &mut agents,
                    current,
                    &message_metadata,
                )?;
            }
        }
        let sender_execution_context = agent_execution_context(plugin, &sender_agent).to_string();
        let mut fields = vec![
            (
                "target_internal_id".to_string(),
                JsonValue::Number(target_internal_id),
            ),
            (
                "sender_internal_id".to_string(),
                JsonValue::Number(sender_agent.internal_id),
            ),
            (
                "sender_agent_id".to_string(),
                JsonValue::String(sender_agent.agent_id.clone()),
            ),
            (
                "target_agent_id".to_string(),
                JsonValue::String(if target_internal_id < 0 {
                    "broadcast".to_string()
                } else {
                    target_agent
                        .as_ref()
                        .map(|agent| agent.agent_id.clone())
                        .unwrap_or_default()
                }),
            ),
            (
                "body".to_string(),
                JsonValue::String(body.trim().to_string()),
            ),
            (
                "visibility".to_string(),
                JsonValue::String("public".to_string()),
            ),
            (
                "workspace_path".to_string(),
                JsonValue::String(sender_agent.worktree_path.clone()),
            ),
            (
                "login_path".to_string(),
                JsonValue::String(sender_agent.login_path.clone()),
            ),
            (
                "repository_path".to_string(),
                JsonValue::String(resolve_repository_path_for_agent(&sender_agent)),
            ),
            (
                "execution_context".to_string(),
                JsonValue::String(sender_execution_context),
            ),
            (
                "assigned_mission".to_string(),
                JsonValue::String(sender_agent.assigned_mission.clone()),
            ),
            (
                "topic".to_string(),
                JsonValue::String(
                    topic
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("")
                        .to_string(),
                ),
            ),
            (
                "created_at".to_string(),
                JsonValue::Number(unix_timestamp()),
            ),
        ];
        append_sync_metadata_fields(&mut fields, &operation_kind, &message_metadata);
        entries.push(JsonEntry { fields });
        write_entry_collection(&plugin.messages_path(), "messages", &entries)?;
        let target_label = if target_internal_id < 0 {
            "broadcast".to_string()
        } else {
            target_agent
                .as_ref()
                .map(|agent| format!("agent {} (# {})", agent.agent_id, agent.internal_id))
                .unwrap_or_else(|| format!("agent {}", target_internal_id))
        };
        Ok(format!(
            "message stored from agent {} to {}",
            sender_agent.agent_id, target_label
        ))
    })
}

pub fn append_feedback(
    plugin: &PluginContext,
    body: &str,
    feedback_context: Option<&str>,
    operation_kind: Option<&str>,
    branch: Option<&str>,
    base_revision: Option<&str>,
    workspace_revision: Option<&str>,
    repository_revision: Option<&str>,
    binary_name: &str,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (mut agents, _tasks, _manager_tasks, _messages) = load_runtime_state(plugin)?;
        let current_agent = resolve_current_agent(plugin, &agents)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "feedback requires the current workspace to be a reconciled agent",
                )
            })?;
        let operation_kind = normalize_operation_kind(operation_kind);
        let cli_metadata = sync_metadata_from_cli(
            branch,
            base_revision,
            workspace_revision,
            repository_revision,
        );
        validate_operation_metadata("feedback", &operation_kind, &cli_metadata)?;
        let feedback_metadata = if sync_metadata_is_empty(&cli_metadata) {
            sync_metadata_from_agent(&current_agent)
        } else {
            cli_metadata.clone()
        };
        persist_current_agent_sync_metadata(
            plugin,
            &mut agents,
            &current_agent,
            &feedback_metadata,
        )?;
        let mut entries = read_message_entries(&plugin.feedback_path(), "entries")?;
        if prune_feedback_entries(&mut entries) {
            write_entry_collection(&plugin.feedback_path(), "entries", &entries)?;
        }
        let mut fields = vec![
            (
                "body".to_string(),
                JsonValue::String(body.trim().to_string()),
            ),
            (
                "workspace_path".to_string(),
                JsonValue::String(current_agent.worktree_path.clone()),
            ),
            (
                "login_path".to_string(),
                JsonValue::String(current_agent.login_path.clone()),
            ),
            (
                "repository_path".to_string(),
                JsonValue::String(resolve_repository_path_for_agent(&current_agent)),
            ),
            (
                "created_at".to_string(),
                JsonValue::Number(unix_timestamp()),
            ),
        ];
        fields.push((
            "internal_id".to_string(),
            JsonValue::Number(current_agent.internal_id),
        ));
        fields.push((
            "agent_id".to_string(),
            JsonValue::String(current_agent.agent_id.clone()),
        ));
        fields.push((
            "sender_internal_id".to_string(),
            JsonValue::Number(current_agent.internal_id),
        ));
        fields.push((
            "sender_agent_id".to_string(),
            JsonValue::String(current_agent.agent_id.clone()),
        ));
        fields.push((
            "execution_context".to_string(),
            JsonValue::String(agent_execution_context(plugin, &current_agent).to_string()),
        ));
        fields.push((
            "assigned_mission".to_string(),
            JsonValue::String(current_agent.assigned_mission.clone()),
        ));
        append_sync_metadata_fields(&mut fields, &operation_kind, &feedback_metadata);
        if let Some(context) = feedback_context
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            fields.push((
                "taxonomy_path".to_string(),
                JsonValue::String(context.to_string()),
            ));
            fields.push((
                "api_taxonomy_command".to_string(),
                JsonValue::String(format!("{} --api {}", binary_name, context)),
            ));
            fields.push((
                "feedback_help_command".to_string(),
                JsonValue::String(format!("{} --help.feedback", binary_name)),
            ));
        }
        entries.push(JsonEntry { fields });
        write_entry_collection(&plugin.feedback_path(), "entries", &entries)?;
        if let Some(context) = feedback_context
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Ok(format!("feedback stored for {}", context))
        } else {
            Ok("feedback stored".to_string())
        }
    })
}

pub fn append_submit(
    plugin: &PluginContext,
    body: &str,
    topic: Option<&str>,
    feedback_context: Option<&str>,
    branch: Option<&str>,
    base_revision: Option<&str>,
    workspace_revision: Option<&str>,
    repository_revision: Option<&str>,
    binary_name: &str,
) -> io::Result<String> {
    let submit_topic = topic
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(Some("submit"));
    let submit_context = feedback_context
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(Some("hierarchy.submit"));

    append_message(
        plugin,
        Some(-1),
        None,
        None,
        None,
        submit_topic,
        Some("submit"),
        branch,
        base_revision,
        workspace_revision,
        repository_revision,
        body,
    )?;
    append_feedback(
        plugin,
        body,
        submit_context,
        Some("submit"),
        branch,
        base_revision,
        workspace_revision,
        repository_revision,
        binary_name,
    )?;
    append_submit_record(
        plugin,
        body,
        submit_topic,
        branch,
        base_revision,
        workspace_revision,
        repository_revision,
    )?;

    Ok("submit evidence stored: public message + feedback + queue record".to_string())
}

pub fn render_current_inbox(plugin: &PluginContext) -> io::Result<String> {
    let (agents, _tasks, _manager_tasks, entries) = load_runtime_state(plugin)?;
    let current_agent = resolve_current_agent(plugin, &agents);

    let Some(agent) = current_agent else {
        return Ok("Inbox unavailable: current workspace is not a registered agent.".to_string());
    };

    let relevant: Vec<JsonEntry> = entries
        .into_iter()
        .filter(|entry| {
            extract_entry_string(entry, "target_agent_id")
                .map(|target| target == agent.agent_id || target == "broadcast")
                .unwrap_or(false)
        })
        .collect();

    let mut out = String::new();
    out.push_str(&format!(
        "Inbox for agent {} ({}):\n",
        agent.internal_id, agent.agent_id
    ));
    if relevant.is_empty() {
        out.push_str("- empty\n");
        return Ok(out);
    }

    for entry in relevant {
        let body = extract_entry_string(&entry, "body").unwrap_or_default();
        let topic = extract_entry_string(&entry, "topic").unwrap_or_default();
        let sender = extract_entry_string(&entry, "workspace_path").unwrap_or_default();
        let sender_login = extract_entry_string(&entry, "login_path").unwrap_or_default();
        let sender_repository = extract_entry_string(&entry, "repository_path").unwrap_or_default();
        let sender_branch = extract_entry_string(&entry, "branch").unwrap_or_default();
        let base_revision = extract_entry_string(&entry, "base_revision").unwrap_or_default();
        let workspace_revision =
            extract_entry_string(&entry, "workspace_revision").unwrap_or_default();
        let repository_revision =
            extract_entry_string(&entry, "repository_revision").unwrap_or_default();
        let operation_kind = extract_entry_string(&entry, "operation_kind").unwrap_or("general");
        let sender_agent_id = extract_entry_string(&entry, "sender_agent_id").unwrap_or_default();
        let sender_internal_id = extract_entry_number(&entry, "sender_internal_id").unwrap_or(-1);
        let target_agent_id = extract_entry_string(&entry, "target_agent_id").unwrap_or_default();
        let target_internal_id = extract_entry_number(&entry, "target_internal_id").unwrap_or(-1);
        let execution_context =
            extract_entry_string(&entry, "execution_context").unwrap_or_default();
        let created_at = extract_entry_number(&entry, "created_at").unwrap_or_default();
        out.push_str(&format!(
            "- from_agent={} (# {}) to_agent={} (# {}) topic={} kind={} context={} workspace={} login={} repository={} branch={} base_revision={} workspace_revision={} repository_revision={} at={} body={}\n",
            empty_or_dash(&sender_agent_id),
            sender_internal_id,
            empty_or_dash(&target_agent_id),
            target_internal_id,
            empty_or_dash(&topic),
            empty_or_dash(&operation_kind),
            empty_or_dash(&execution_context),
            empty_or_dash(&sender),
            empty_or_dash(&sender_login),
            empty_or_dash(&sender_repository),
            empty_or_dash(&sender_branch),
            empty_or_dash(&base_revision),
            empty_or_dash(&workspace_revision),
            empty_or_dash(&repository_revision),
            created_at,
            empty_or_dash(&body)
        ));
    }

    Ok(out)
}

pub fn render_messages_feed(
    plugin: &PluginContext,
    topic_filter: Option<&str>,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, _tasks, _manager_tasks, messages) = load_runtime_state(plugin)?;
        let topic_filter = topic_filter
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let filtered: Vec<JsonEntry> = messages
            .into_iter()
            .filter(|entry| {
                topic_filter.map_or(true, |topic| {
                    extract_entry_string(entry, "topic")
                        .map(|value| value.trim() == topic)
                        .unwrap_or(false)
                })
            })
            .collect();

        if json_requested {
            let mut out = String::new();
            out.push_str("{\n");
            out.push_str("  \"command\": \"messages\",\n");
            out.push_str(&format!(
                "  \"topic_filter\": \"{}\",\n",
                json_escape(topic_filter.unwrap_or(""))
            ));
            out.push_str(&format!("  \"count\": {},\n", filtered.len()));
            out.push_str("  \"messages\": [\n");
            for (idx, entry) in filtered.iter().enumerate() {
                let comma = if idx + 1 < filtered.len() { "," } else { "" };
                out.push_str(&render_message_entry_json(entry, comma));
            }
            out.push_str("  ]\n");
            out.push_str("}\n");
            return Ok(out);
        }

        let mut out = String::new();
        if let Some(topic) = topic_filter {
            out.push_str(&format!("Thread: {}\n\n", topic));
        } else {
            out.push_str("Messages\n\n");
        }
        out.push_str(&format!("Count: {}\n\n", filtered.len()));
        if filtered.is_empty() {
            out.push_str("- empty\n");
            return Ok(out);
        }
        for entry in &filtered {
            out.push_str(&render_message_entry_text(entry, &agents));
        }
        Ok(out)
    })
}

pub fn render_plugin_help(plugin: Option<&PluginContext>, binary_name: &str) -> String {
    let mut out = String::new();
    out.push_str("Aesthetic Core Ant-Delirium\n\n");
    out.push_str("Direct plugin interface.\n");
    out.push_str("Project taxonomy commands remain isolated behind the `--api` entrypoint.\n\n");
    out.push_str("Quick start:\n");
    out.push_str(&format!("- {} --help\n", binary_name));
    out.push_str(&format!("- {} --version\n", binary_name));
    out.push_str(&format!("- {} --installation\n", binary_name));
    out.push_str(&format!("- {} --api\n", binary_name));
    out.push_str(&format!("- {} config init\n", binary_name));
    out.push_str(&format!("- {} config dump\n", binary_name));
    out.push_str(&format!("- {} swarm status\n", binary_name));
    out.push_str(&format!("- {} --getprojects\n", binary_name));
    out.push_str(&format!("- {} --whoami\n", binary_name));
    out.push_str(&format!("- {} --rebase-check\n", binary_name));
    out.push_str(&format!("- {} --show-agent <id>\n", binary_name));
    out.push_str(&format!("- {} --show-agent-id <agent_id>\n", binary_name));
    out.push_str(&format!("- {} --help.feedback\n", binary_name));
    out.push_str(&format!("- {} --manager-tasks\n", binary_name));
    out.push_str(&format!("- {} --submits\n", binary_name));
    out.push_str(&format!("- {} --submits-active\n", binary_name));
    out.push_str(&format!("- {} --submits-history\n", binary_name));
    out.push_str(&format!("- {} --submit-show <submit_id>\n", binary_name));
    out.push_str(&format!("- {} --submit-accept <submit_id>\n", binary_name));
    out.push_str(&format!("- {} --submit-reject <submit_id>\n", binary_name));
    out.push_str(&format!("- {} --submit-close <submit_id>\n", binary_name));
    out.push_str(&format!(
        "- {} --manager-task-add \"...\" --manager-task-priority <n>\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --manager-task-done <task_id>\n",
        binary_name
    ));
    out.push('\n');
    out.push_str("Direct plugin commands:\n");
    out.push_str(&format!("- {} --help\n", binary_name));
    out.push_str(&format!("- {} --version\n", binary_name));
    out.push_str(&format!("- {} --plugin.help\n", binary_name));
    out.push_str(&format!("- {} --installation\n", binary_name));
    out.push_str(&format!("- {} config init\n", binary_name));
    out.push_str(&format!("- {} config dump\n", binary_name));
    out.push_str(&format!("- {} swarm status\n", binary_name));
    out.push_str(&format!("- {} --getprojects\n", binary_name));
    out.push_str(&format!("- {} --whoami\n", binary_name));
    out.push_str(&format!("- {} --rebase-check\n", binary_name));
    out.push_str(&format!("- {} --show-agent <id>\n", binary_name));
    out.push_str(&format!("- {} --show-agent-id <agent_id>\n", binary_name));
    out.push_str(&format!("- {} --help.feedback\n", binary_name));
    out.push_str(&format!(
        "- {} --project-root <path> --config <plugin-config-dir>\n",
        binary_name
    ));
    out.push_str(&format!("- {} --search <hierarchy.path>\n", binary_name));
    out.push_str(&format!("- {} --getagents (legacy alias)\n", binary_name));
    out.push_str(&format!("- {} --getprojects\n", binary_name));
    out.push_str(&format!("- {} --whoami\n", binary_name));
    out.push_str(&format!("- {} --rebase-check\n", binary_name));
    out.push_str(&format!("- {} --show-agent <id>\n", binary_name));
    out.push_str(&format!("- {} --show-agent-id <agent_id>\n", binary_name));
    out.push_str(&format!("- {} --inbox\n", binary_name));
    out.push_str(&format!("- {} --messages\n", binary_name));
    out.push_str(&format!("- {} --thread <topic>\n", binary_name));
    out.push_str(&format!(
        "- {} --message \"...\" --to-agent <id> --topic <topic>\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --message \"...\" --to-agent-id <agent_id> --topic <topic>\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --message \"...\" --to-agent <id> --from-agent <id>\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --message \"...\" --to-agent-id <agent_id> --from-agent-id <agent_id>\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --submit \"...\" --branch <branch> --base-revision <rev> --workspace-revision <rev> --repository-revision <rev>\n",
        binary_name
    ));
    out.push_str(&format!("- {} --broadcast \"...\"\n", binary_name));
    out.push_str(&format!(
        "- {} --feedback \"...\" --context <taxonomy.path>\n",
        binary_name
    ));
    out.push_str(&format!("- {} --json <command>\n", binary_name));
    out.push('\n');
    out.push_str("Project API entrypoint:\n");
    out.push_str(&format!("- {} --api\n", binary_name));
    out.push_str("Run that command to open the project API welcome screen. Root-scoped commands such as `--api <root>.help` are documented there, not here.\n");
    out.push('\n');
    out.push_str("Direct command scope:\n");
    out.push_str("- plugin installation and build.rs integration\n");
    out.push_str("- TOML package layout beside the final binary\n");
    out.push_str("- shared public message feed for agent coordination\n");
    out.push_str("- git worktree execution model and branch isolation\n");
    out.push_str("- `cargo check` inside a repository root or worktree is the canonical login event for that agent\n");
    out.push_str("- `agent_id` is the stable communication identity; numeric agent ids are operational aliases only\n");
    out.push_str("- `--rebase-check` only evaluates metadata already stored for the agent; the plugin does not execute git itself\n");
    out.push_str("- sync and submit operations must pass `--branch`, `--base-revision`, `--workspace-revision`, and `--repository-revision`\n");
    out.push_str("- `--submit` is the canonical integration signal and stores both submit evidence artifacts for the current agent\n");
    out.push_str("- `submit_queue.json` is the canonical root-side submit lifecycle queue\n");
    out.push_str("- `manager_tasks.json` is the priority backlog that can override normal violation work until marked done\n");
    if let Some(plugin) = plugin {
        out.push('\n');
        out.push_str("Detected plugin context:\n");
        out.push_str(&format!(
            "Plugin directory: {}\nRepository root: {}\nWorkspace path: {}\n",
            plugin.plugin_dir.to_string_lossy(),
            plugin.repository_path.to_string_lossy(),
            plugin.workspace_path.to_string_lossy(),
        ));
    }
    out
}

pub fn render_plugin_feedback_help(plugin: Option<&PluginContext>, binary_name: &str) -> String {
    let mut out = String::new();
    out.push_str("Ant-Delirium feedback guide.\n\n");
    out.push_str("Purpose:\n");
    out.push_str("Use feedback when the current rule or taxonomy explanation is insufficient for a real remediation case. The feedback artifact preserves those missing explanations so the help surface can evolve.\n\n");
    out.push_str("Canonical command:\n");
    out.push_str(&format!(
        "- {} --feedback \"Explain better how to solve this case\" --context hierarchy.files.max_nesting\n\n",
        binary_name
    ));
    out.push_str("Guidelines:\n");
    out.push_str("1. Prefer a concrete taxonomy path in `--context`.\n");
    out.push_str("2. Describe the exact confusion or missing explanation.\n");
    out.push_str("3. Keep the message actionable so the taxonomy and API docs can be improved.\n");
    out.push_str("4. The runtime stores the sending agent identity when the current workspace is registered.\n");
    out.push_str("5. For `--feedback-kind sync` or `--feedback-kind submit`, pass `--branch`, `--base-revision`, `--workspace-revision`, and `--repository-revision`.\n");
    out.push_str("6. If those fields are missing, the plugin rejects the operation with an instructive error so the orchestrator can retry correctly.\n");
    out.push_str("7. Use `--submit` when the agent is explicitly asking the root to consider integration; it creates the public submit message and the submit feedback together.\n\n");
    out.push_str("Examples:\n");
    out.push_str(&format!(
        "- {} --feedback \"I found a case where the nesting limit collides with symmetry. Explain the preferred refactor.\" --context hierarchy.files.max_nesting\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --feedback \"The installation guide did not explain how to lock the sovereign files in this workspace.\" --context hierarchy.installation.security\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --feedback \"root advanced; submit blocked until rebase\" --feedback-kind sync --branch ant-agent-1 --base-revision abc123 --workspace-revision def456 --repository-revision 789xyz\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --submit \"range.rs fix is ready for review\" --topic submit.range --context hierarchy.submit --branch ant-agent-1 --base-revision abc123 --workspace-revision def456 --repository-revision 789xyz\n",
        binary_name
    ));
    if let Some(plugin) = plugin {
        out.push_str("\nRuntime artifact:\n");
        out.push_str(&format!(
            "Feedback file: {}\n",
            plugin.feedback_path().to_string_lossy()
        ));
    }
    out
}

pub fn render_plugin_installation(plugin: Option<&PluginContext>, binary_name: &str) -> String {
    let mut out = String::new();
    out.push_str("Ant-Delirium plugin installation guide.\n\n");
    out.push_str("Direct binary scope:\n");
    out.push_str("This command explains the plugin package itself. It is not project-taxonomy API and does not require `<root>`.\n\n");
    out.push_str("Required package layout:\n");
    out.push_str("1. Place the final binary at `plugins/<project>/ac_ant_delirium`.\n");
    out.push_str("2. Place the TOML package at `plugins/<project>/ant_delirium_config`.\n");
    out.push_str("3. Keep `default_hierarchy.toml` and `default_rules.toml` inside that external config folder.\n");
    out.push_str("4. Generate those two base TOMLs with `config init`; do not expose the internal test profiles type_1/type_2/type_3 to the target project.\n");
    out.push_str("5. Do not duplicate those TOMLs inside each crate or worktree; the plugin package beside the binary is the authoritative source reused across branches.\n\n");
    out.push_str("build.rs integration steps:\n");
    out.push_str("1. Run `config init` so the repository root receives `AC_Ant-Delirium_props/ant_delirium_build_guard.rs` together with the external TOMLs.\n");
    out.push_str("2. In each compilable package, call `ant_delirium_build_guard::run(...)` as the first operational statement inside `fn main()` of `build.rs`.\n");
    out.push_str("3. Resolve the external plugin folder from `CARGO_MANIFEST_DIR`.\n");
    out.push_str("4. Point `plugin_bin` to `../plugins/<project>/ac_ant_delirium`.\n");
    out.push_str("5. Point `config_dir` to `../plugins/<project>/ant_delirium_config`.\n");
    out.push_str("6. Abort the build when the guard exits with non-zero status.\n");
    out.push_str("7. Add `cargo:rerun-if-changed` for the crate sources, plugin binary, and plugin config directory.\n");
    out.push_str("8. Lock the guard file and the plugin TOMLs as readonly superuser-controlled assets before trusting the repository.\n\n");
    out.push_str("Minimal build.rs pattern:\n");
    out.push_str("#[path = \"../AC_Ant-Delirium_props/ant_delirium_build_guard.rs\"]\n");
    out.push_str("mod ant_delirium_build_guard;\n");
    out.push_str("fn main() {\n");
    out.push_str("    ant_delirium_build_guard::run(&[\"src\", \"tests\"]);\n");
    out.push_str("}\n\n");
    out.push_str("Git worktree model:\n");
    out.push_str("1. The human or orchestration layer creates each git worktree first.\n");
    out.push_str("2. The same plugin package is reused across repository root and worktrees.\n");
    out.push_str("3. ant_delirium compares the current workspace path with the authoritative repository path from `default_hierarchy.toml`.\n");
    out.push_str("4. If both paths match, execution is `repository root`.\n");
    out.push_str("5. If they differ, execution is `git worktree`, and the workspace is tracked as an isolated agent in the shared JSON state.\n\n");
    out.push_str("Direct plugin commands:\n");
    out.push_str(&format!("- {} --help\n", binary_name));
    out.push_str(&format!("- {} --version\n", binary_name));
    out.push_str(&format!("- {} --plugin.help\n", binary_name));
    out.push_str(&format!("- {} --installation\n", binary_name));
    out.push_str(&format!("- {} config init\n", binary_name));
    out.push_str(&format!("- {} config dump\n", binary_name));
    out.push_str(&format!("- {} swarm status\n", binary_name));
    out.push_str(&format!("- {} --inbox\n", binary_name));
    out.push_str(&format!(
        "- {} --message \"...\" --to-agent-id <agent_id>\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --message \"...\" --to-agent-id <agent_id> --from-agent-id <agent_id>\n",
        binary_name
    ));
    out.push_str(&format!("- {} --show-agent-id <agent_id>\n", binary_name));
    out.push_str(&format!("- {} --broadcast \"...\"\n", binary_name));
    out.push_str(&format!(
        "- {} --feedback \"...\" --context <taxonomy.path>\n",
        binary_name
    ));
    out.push_str(&format!("- {} --help.feedback\n", binary_name));
    out.push('\n');
    out.push_str("Project API entrypoint:\n");
    out.push_str(&format!("- {} --api\n", binary_name));
    out.push_str("Use the API welcome screen to discover the active `<root>` and then the project-specific taxonomy commands.\n");
    if let Some(plugin) = plugin {
        out.push('\n');
        out.push_str("Detected plugin package:\n");
        out.push_str(&format!(
            "Binary path: {}\nConfig path: {}\nRepository path: {}\nWorkspace path: {}\n",
            plugin.plugin_dir.join("ac_ant_delirium").to_string_lossy(),
            plugin.config_path.to_string_lossy(),
            plugin.repository_path.to_string_lossy(),
            plugin.workspace_path.to_string_lossy(),
        ));
    }
    out
}

pub fn materialize_project_local_plugin_package(
    project_root: &Path,
    explicit_project_root: Option<&str>,
    plugin: Option<&PluginContext>,
    binary_name: &str,
) -> io::Result<String> {
    let current_exe = env::current_exe()?;
    let (effective_project_root, plugin_dir) = match plugin {
        Some(context) => (context.repository_path.clone(), context.plugin_dir.clone()),
        None => infer_bootstrap_context(project_root, explicit_project_root, &current_exe),
    };
    let config_dir = plugin_dir.join("ant_delirium_config");
    let plugin_bin = plugin_dir.join("ac_ant_delirium");
    let props_dir = effective_project_root.join("AC_Ant-Delirium_props");
    let guard_path = props_dir.join("ant_delirium_build_guard.rs");

    fs::create_dir_all(&plugin_dir)?;
    fs::create_dir_all(&config_dir)?;
    fs::create_dir_all(&props_dir)?;

    let copied_binary = copy_binary_if_needed(&current_exe, &plugin_bin)?;
    let hierarchy_path = config_dir.join("default_hierarchy.toml");
    let rules_path = config_dir.join("default_rules.toml");

    let hierarchy_written = write_default_template_if_missing(
        &hierarchy_path,
        &render_new_project_hierarchy(&effective_project_root),
    )?;
    let rules_written = write_default_template_if_missing(&rules_path, DEFAULT_NEW_PROJECT_RULES)?;
    let guard_written =
        write_default_template_if_missing(&guard_path, DEFAULT_NEW_PROJECT_BUILD_GUARD)?;

    let mut out = String::new();
    out.push_str("Project-local plugin package ensured.\n\n");
    out.push_str(&format!(
        "Project root: {}\n",
        effective_project_root.to_string_lossy()
    ));
    out.push_str(&format!("Plugin dir: {}\n", plugin_dir.to_string_lossy()));
    out.push_str(&format!("Binary path: {}\n", plugin_bin.to_string_lossy()));
    out.push_str(&format!("Config dir: {}\n", config_dir.to_string_lossy()));
    out.push_str(&format!("Guard path: {}\n\n", guard_path.to_string_lossy()));
    out.push_str("Bootstrap actions:\n");
    out.push_str(&format!(
        "- binary: {}\n",
        if copied_binary {
            "copied/refreshed"
        } else {
            "kept existing"
        }
    ));
    out.push_str(&format!(
        "- default_hierarchy.toml: {}\n",
        if hierarchy_written {
            "created from get_toml_for_new"
        } else {
            "kept existing"
        }
    ));
    out.push_str(&format!(
        "- default_rules.toml: {}\n",
        if rules_written {
            "created from get_toml_for_new"
        } else {
            "kept existing"
        }
    ));
    out.push_str(&format!(
        "- ant_delirium_build_guard.rs: {}\n",
        if guard_written {
            "created from get_toml_for_new"
        } else {
            "kept existing"
        }
    ));
    out.push_str("\nNext steps:\n");
    out.push_str(&format!(
        "- {} --project-root {} --config {}\n",
        binary_name,
        shell_escape(&effective_project_root),
        shell_escape(&config_dir),
    ));
    out.push_str("- wire each compilable package build.rs to call AC_Ant-Delirium_props/ant_delirium_build_guard.rs as the first operational statement\n");
    out.push_str("- review repository, target.core, target.tests, and libres in default_hierarchy.toml before enforcing the build hook\n");
    Ok(out)
}

pub fn build_api_overrides(
    plugin: &PluginContext,
    hierarchy: &HierarchyConfig,
    binary_name: &str,
) -> io::Result<Vec<(String, String)>> {
    ensure_support_files(plugin)?;
    let root = api_root(hierarchy);
    let agents = read_connected_agents(&plugin.connected_path())?;
    let tasks = read_governance_tasks(&plugin.governance_path())?;
    let manager_tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
    let current_agent = agents.iter().find(|agent| {
        normalize_path_str(&agent.worktree_path) == normalize_path(plugin.workspace_path.as_path())
    });
    let active_agents = agents
        .iter()
        .filter(|agent| unix_timestamp() - agent.last_ping <= 3600)
        .count();
    let unassigned_tasks = tasks.iter().filter(|task| task.owner_id < 0).count();
    let unassigned_manager_tasks = manager_tasks
        .iter()
        .filter(|task| task.owner_id < 0 && is_manager_task_active(task))
        .count();
    let top_unassigned = tasks
        .iter()
        .filter(|task| task.owner_id < 0)
        .max_by_key(|task| task.violation_count);
    let top_manager = manager_tasks
        .iter()
        .filter(|task| task.owner_id < 0 && is_manager_task_active(task))
        .max_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| right.created_at.cmp(&left.created_at))
        });

    let execution_context = if plugin.is_worktree {
        "git worktree"
    } else {
        "repository root"
    };
    let plugin_binary_path = plugin.plugin_dir.join("ac_ant_delirium");

    let mut installation = String::new();
    installation.push_str("Multi-agent execution system.\n\n");
    installation.push_str("Purpose:\n");
    installation.push_str(
        "This plugin does not create git worktrees. The human or external orchestrator creates each workspace first. ant_delirium only detects the current workspace, connects it to the shared agent state, and keeps task ownership synchronized through JSON artifacts stored beside the plugin binary.\n\n",
    );
    installation.push_str("Current runtime:\n");
    installation.push_str(&format!(
        "Plugin path: {}\nRepository path: {}\nWorkspace path: {}\nExecution context: {}\n",
        plugin.plugin_dir.to_string_lossy(),
        plugin.repository_path.to_string_lossy(),
        plugin.workspace_path.to_string_lossy(),
        execution_context
    ));
    installation.push('\n');
    installation.push_str("How registration works:\n");
    installation.push_str("1. Place the project binary inside the project plugin folder, for example `plugins/<project>/ac_ant_delirium`.\n");
    installation.push_str(
        "2. Place the project configuration folder at `plugins/<project>/ant_delirium_config`.\n",
    );
    installation.push_str("3. Set the authoritative repository path in `default_hierarchy.toml` inside that config folder.\n");
    installation.push_str("4. Run the same project-local ant_delirium binary from either the repository root or a git worktree of that repository.\n");
    installation
        .push_str("5. ant_delirium compares the current `project_root` with `repository`.\n");
    installation
        .push_str("6. If both paths are equal, the execution context is `repository root`.\n");
    installation.push_str("7. If the paths differ, the execution context is `git worktree` and the current workspace is registered in `connected.json` as an isolated agent.\n\n");
    installation.push_str("Build.rs integration template:\n");
    installation.push_str("1. Keep the final binary at the project plugin path and do not copy it into the crate itself.\n");
    installation.push_str(&format!(
        "   Binary path: `{}`\n",
        plugin_binary_path.to_string_lossy()
    ));
    installation.push_str(&format!(
        "   Config path: `{}`\n",
        plugin.config_path.to_string_lossy()
    ));
    installation.push_str("2. Run `config init` once so the repository root also receives `AC_Ant-Delirium_props/ant_delirium_build_guard.rs`.\n");
    installation.push_str("3. Keep `default_hierarchy.toml` and `default_rules.toml` inside that config folder, beside the binary, so multiple branches and worktrees reuse the same authoritative governance package.\n");
    installation.push_str("4. In the target crate `build.rs`, resolve the external plugin folder and call `ant_delirium_build_guard::run(...)` before any other operational statement.\n");
    installation.push_str(
        "5. The shared guard passes both `--project-root <crate-root>` and `--config <plugin-config-dir>`.\n",
    );
    installation.push_str("6. If the audit exits with non-zero status, panic in `build.rs` to block `cargo check` and `cargo build`.\n");
    installation.push_str("7. Mark `build.rs`, the source roots, the plugin binary, and the config directory with `cargo:rerun-if-changed`.\n");
    installation.push_str("8. Minimal execution pattern:\n");
    installation
        .push_str("   let manifest_dir = PathBuf::from(env::var(\"CARGO_MANIFEST_DIR\")?);\n");
    installation.push_str("   let plugin_dir = manifest_dir.join(\"../plugins/<project>\");\n");
    installation.push_str("   let plugin_bin = plugin_dir.join(\"ac_ant_delirium\");\n");
    installation.push_str("   let config_dir = plugin_dir.join(\"ant_delirium_config\");\n");
    installation.push_str("   let status = Command::new(&plugin_bin)\n");
    installation.push_str("       .current_dir(&manifest_dir)\n");
    installation.push_str("       .arg(\"--project-root\").arg(&manifest_dir)\n");
    installation.push_str("       .arg(\"--config\").arg(&config_dir)\n");
    installation.push_str("       .status()?;\n");
    installation
        .push_str("   if !status.success() { panic!(\"governance violations detected\"); }\n\n");
    installation.push_str("Shared state files:\n");
    installation.push_str(&format!(
        "- `connected.json`: registered agents and last known mission.\n"
    ));
    installation.push_str(&format!(
        "- `governance.json`: file ownership and violation totals per file.\n"
    ));
    installation.push_str(&format!(
        "- `context.json`: current repository/workspace snapshot.\n"
    ));
    installation.push_str(&format!(
        "- `messages.json`: public message feed with routed agent messages.\n"
    ));
    installation.push_str(&format!(
        "- `feedback.json`: human feedback for the system.\n"
    ));
    installation.push_str(&format!(
        "- `submit_queue.json`: canonical lifecycle queue for submit requests.\n"
    ));
    installation.push_str(&format!(
        "- `manager_tasks.json`: optional priority tasks above normal violation distribution.\n\n"
    ));
    installation.push_str("What ant_delirium does automatically:\n");
    installation.push_str("- registers the current workspace when plugin context is detected\n");
    installation.push_str("- refreshes `governance.json` from the latest audit result\n");
    installation.push_str("- assigns one unowned task to the current agent when possible\n");
    installation
        .push_str("- filters the local audit output so a worktree sees only its owned files\n");
    installation.push_str(
        "- records public routed messages and human feedback into shared JSON artifacts\n\n",
    );
    installation.push_str("What ant_delirium does not do:\n");
    installation.push_str("- it does not create or delete git worktrees\n");
    installation.push_str("- it does not move files between agents automatically\n");
    installation.push_str("- it does not execute git, pull, rebase, merge, or other external vcs commands on behalf of the agent\n");
    installation.push_str("- it does not prevent a human from editing files outside the assigned mission; isolation is operational and driven by ownership data\n\n");
    installation.push_str("Current shared-state summary:\n");
    installation.push_str(&format!(
        "Agents registered: {} total, {} active\n",
        agents.len(),
        active_agents
    ));
    if let Some(agent) = current_agent {
        installation.push_str(&format!(
            "Current agent: {} mission={}\n",
            agent.internal_id,
            empty_or_dash(&agent.assigned_mission)
        ));
    } else {
        installation.push_str("Current agent: none\n");
    }
    installation.push_str(&format!("Unassigned tasks: {}\n", unassigned_tasks));
    installation.push_str(&format!(
        "Unassigned manager priority tasks: {}\n",
        unassigned_manager_tasks
    ));
    if let Some(task) = top_unassigned {
        installation.push_str(&format!(
            "Highest unassigned task: {} ({} violations)\n",
            task.file_path, task.violation_count
        ));
    }
    if let Some(task) = top_manager {
        installation.push_str(&format!(
            "Highest manager priority task: {} (priority {}, file={})\n",
            task.title,
            task.priority,
            empty_or_dash(&task.file_path)
        ));
    }
    installation.push('\n');
    installation.push_str("Direct plugin commands:\n");
    installation.push_str(&format!("- {} --installation\n", binary_name));
    installation.push_str(&format!("- {} config init\n", binary_name));
    installation.push_str(&format!("- {} swarm status\n", binary_name));
    installation.push_str(&format!("- {} --rebase-check\n", binary_name));
    installation.push_str(&format!("- {} --inbox\n", binary_name));
    installation.push_str(&format!("- {} --messages\n", binary_name));
    installation.push_str(&format!("- {} --thread <topic>\n", binary_name));
    installation.push_str(&format!(
        "- {} --message \"...\" --to-agent-id <agent_id> --topic <topic>\n",
        binary_name
    ));
    installation.push_str(&format!(
        "- {} --message \"...\" --to-agent-id <agent_id> --from-agent-id <agent_id>\n",
        binary_name
    ));
    installation.push_str(&format!(
        "- {} --submit \"...\" --branch <branch> --base-revision <rev> --workspace-revision <rev> --repository-revision <rev>\n",
        binary_name
    ));
    installation.push_str(&format!("- {} --submits\n", binary_name));
    installation.push_str(&format!("- {} --submits-active\n", binary_name));
    installation.push_str(&format!("- {} --submits-history\n", binary_name));
    installation.push_str(&format!("- {} --submit-show <submit_id>\n", binary_name));
    installation.push_str(&format!("- {} --submit-accept <submit_id>\n", binary_name));
    installation.push_str(&format!("- {} --submit-reject <submit_id>\n", binary_name));
    installation.push_str(&format!("- {} --submit-close <submit_id>\n", binary_name));
    installation.push_str(&format!("- {} --show-agent-id <agent_id>\n", binary_name));
    installation.push_str(&format!("- {} --manager-tasks\n", binary_name));
    installation.push_str(&format!(
        "- {} --manager-task-add \"...\" --manager-task-body \"...\" --manager-task-file <path> --manager-task-priority <n>\n",
        binary_name
    ));
    installation.push_str(&format!(
        "- {} --manager-task-done <task_id>\n",
        binary_name
    ));
    installation.push_str(&format!("- {} --broadcast \"...\"\n", binary_name));
    installation.push_str(&format!(
        "- {} --agent <id> (legacy recipient alias)\n",
        binary_name
    ));
    installation.push_str("Identity and sync contract:\n");
    installation.push_str("- `cargo check` is the login event.\n");
    installation.push_str("- `agent_id` is the stable communication id.\n");
    installation.push_str("- `internal_id` is the short operational alias.\n");
    installation.push_str("- `branch`, `base_revision`, `workspace_revision`, and `repository_revision` are metadata supplied by the AI/orchestrator.\n");
    installation.push_str(
        "- ant_delirium does not discover those values itself; it validates and persists them.\n",
    );
    installation.push_str("- `--message-kind sync|submit` and `--feedback-kind sync|submit` require all four metadata flags.\n");
    installation.push_str("- `--submit` is the canonical integration request and writes both the public submit message and the submit feedback.\n");
    installation.push_str("- `submit_queue.json` is the root-side lifecycle queue for submit records and decisions.\n");
    installation.push_str("- `manager_tasks.json` is the user-priority backlog; active manager tasks suppress ordinary violation work for the claimed agent until marked done.\n");
    installation
        .push_str("- `--rebase-check` reads the persisted metadata and does not call git.\n\n");
    installation.push_str(&format!(
        "- {} --feedback \"...\" --context <taxonomy.path>\n",
        binary_name
    ));
    installation.push_str(&format!("- {} --help.feedback\n", binary_name));
    installation.push('\n');
    installation.push_str("Project API commands:\n");
    installation.push_str("These commands are scoped by the active taxonomy root and belong to the project API, not to the direct plugin command surface.\n");
    installation.push_str(&format!(
        "- {} --api {}.installation.context\n",
        binary_name, root
    ));
    installation.push_str(&format!(
        "- {} --api {}.installation.agents\n",
        binary_name, root
    ));
    installation.push_str(&format!(
        "- {} --api {}.installation.messages\n",
        binary_name, root
    ));
    installation.push_str(&format!(
        "- {} --api {}.installation.feedback\n",
        binary_name, root
    ));

    let context = format!(
        "Context artifact: {}\nRepository root: {}\nActive workspace: {}\nPlugin directory: {}\n",
        plugin.context_path().to_string_lossy(),
        plugin.repository_path.to_string_lossy(),
        plugin.workspace_path.to_string_lossy(),
        plugin.plugin_dir.to_string_lossy(),
    );

    let agents_doc = render_agents_report(plugin, false)?;
    let messages = format!(
        "Messages artifact: {}\n{}\nUse {} --message \"...\" --to-agent-id <agent_id> to append a directed instruction with stable identity.\nUse {} --message \"...\" --to-agent <id> for the legacy numeric alias.\nUse {} --message \"...\" --to-agent-id <agent_id> --from-agent-id <agent_id> to override sender identity when needed.\nUse {} --message \"...\" --message-kind sync --branch <branch> --base-revision <base> --workspace-revision <workspace> --repository-revision <root> when the communication depends on VCS state.\nUse {} --broadcast \"...\" for a global message.\nUse {} --inbox to read the current agent inbox.\n",
        plugin.messages_path().to_string_lossy(),
        render_current_inbox(plugin)?,
        binary_name,
        binary_name,
        binary_name,
        binary_name,
        binary_name,
        binary_name,
    );
    let feedback = format!(
        "Feedback artifact: {}\nUse {} --feedback \"...\" --context <taxonomy.path> to append human guidance for the system.\nUse {} --feedback-kind sync --branch <branch> --base-revision <base> --workspace-revision <workspace> --repository-revision <root> when feedback depends on VCS state.\nFeedback records agent_id, internal_id, branch, workspace_path and login_path for later reconciliation.\nUse {} --help.feedback for detailed usage examples.\n",
        plugin.feedback_path().to_string_lossy(),
        binary_name,
        binary_name,
        binary_name,
    );

    Ok(vec![
        (format!("{}.installation", root), installation),
        (format!("{}.installation.context", root), context),
        (format!("{}.installation.agents", root), agents_doc),
        (format!("{}.installation.messages", root), messages),
        (format!("{}.installation.feedback", root), feedback),
    ])
}

fn discover_plugin_context(project_root: &Path, argv0: Option<&str>) -> Option<PluginContext> {
    if let Some(context) = plugin_context_from_binary_parent(project_root, argv0) {
        return Some(context);
    }
    if let Some(context) = plugin_context_from_current_exe(project_root) {
        return Some(context);
    }

    let mut best_match: Option<(usize, PluginContext)> = None;
    for plugins_root in candidate_plugin_roots(project_root, argv0) {
        let Ok(entries) = fs::read_dir(&plugins_root) else {
            continue;
        };

        for entry in entries.filter_map(|entry| entry.ok()) {
            let plugin_dir = entry.path();
            let Some(config_path) = resolve_plugin_config_path(&plugin_dir) else {
                continue;
            };

            let Some(repository_path) = extract_repository_path(&config_path) else {
                continue;
            };

            let score = match_plugin_context(project_root, &plugin_dir, &repository_path);
            if score == 0 {
                continue;
            }

            let context = PluginContext {
                plugin_dir: plugin_dir.clone(),
                config_path,
                repository_path: repository_path.clone(),
                workspace_path: resolve_workspace_path(project_root, &repository_path, &plugin_dir),
                is_worktree: normalize_path(&resolve_workspace_path(
                    project_root,
                    &repository_path,
                    &plugin_dir,
                )) != normalize_path(&repository_path),
            };

            match &best_match {
                Some((best_score, _)) if *best_score >= score => {}
                _ => best_match = Some((score, context)),
            }
        }
    }

    best_match.map(|(_, context)| context)
}

fn plugin_context_from_binary_parent(
    project_root: &Path,
    argv0: Option<&str>,
) -> Option<PluginContext> {
    let bin = argv0?;
    let parent = Path::new(bin).parent()?;
    let config_path = resolve_plugin_config_path(parent)?;
    build_plugin_context_from_config(project_root, &config_path)
}

fn plugin_context_from_current_exe(project_root: &Path) -> Option<PluginContext> {
    let current_exe = env::current_exe().ok()?;
    let parent = current_exe.parent()?;
    let config_path = resolve_plugin_config_path(parent)?;
    build_plugin_context_from_config(project_root, &config_path)
}

fn build_plugin_context_from_config(
    project_root: &Path,
    config_path: &Path,
) -> Option<PluginContext> {
    let plugin_dir = plugin_dir_from_config_path(config_path)?;
    let repository_path = extract_repository_path(config_path)?;
    let workspace_path = resolve_workspace_path(project_root, &repository_path, &plugin_dir);
    Some(PluginContext {
        plugin_dir,
        config_path: config_path.to_path_buf(),
        repository_path: repository_path.clone(),
        workspace_path: workspace_path.clone(),
        is_worktree: normalize_path(&workspace_path) != normalize_path(&repository_path),
    })
}

fn resolve_workspace_path(
    project_root: &Path,
    repository_path: &Path,
    plugin_dir: &Path,
) -> PathBuf {
    let normalized_project = normalize_path(project_root);
    let normalized_repository = normalize_path(repository_path);
    let normalized_plugin_dir = normalize_path(plugin_dir);

    if normalized_project == normalized_plugin_dir
        || normalized_project == normalize_path(&plugin_dir.parent().unwrap_or(plugin_dir))
    {
        return repository_path.to_path_buf();
    }

    if normalized_project != normalized_repository
        && normalized_repository.starts_with(&normalized_project)
        && normalized_plugin_dir.starts_with(&normalized_project)
    {
        return repository_path.to_path_buf();
    }

    project_root.to_path_buf()
}

fn resolve_plugin_config_path(plugin_dir: &Path) -> Option<PathBuf> {
    let config_dir = plugin_dir.join("ant_delirium_config");
    if config_dir.is_dir() {
        return Some(config_dir);
    }

    let legacy_toml = plugin_dir.join("delirium.toml");
    if legacy_toml.is_file() {
        return Some(legacy_toml);
    }

    None
}

fn default_plugin_dir_for_project(project_root: &Path) -> PathBuf {
    let project_name = project_root
        .file_name()
        .unwrap_or_else(|| panic!("project root should have a terminal folder name"));
    project_root
        .parent()
        .unwrap_or(project_root)
        .join("plugins")
        .join(project_name)
}

fn infer_bootstrap_context(
    project_root: &Path,
    explicit_project_root: Option<&str>,
    current_exe: &Path,
) -> (PathBuf, PathBuf) {
    if explicit_project_root.is_some() {
        return (
            project_root.to_path_buf(),
            default_plugin_dir_for_project(project_root),
        );
    }

    if let Some(plugin_dir) = current_exe.parent() {
        if looks_like_project_plugin_dir(plugin_dir) {
            let inferred_root = infer_repository_root_from_plugin_dir(plugin_dir)
                .unwrap_or_else(|| project_root.to_path_buf());
            return (inferred_root, plugin_dir.to_path_buf());
        }
    }

    (
        project_root.to_path_buf(),
        default_plugin_dir_for_project(project_root),
    )
}

fn looks_like_project_plugin_dir(path: &Path) -> bool {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|value| value.to_str())
        == Some("plugins")
}

fn infer_repository_root_from_plugin_dir(plugin_dir: &Path) -> Option<PathBuf> {
    let project_name = plugin_dir.file_name()?;
    let repository_root = plugin_dir.parent()?.parent()?.join(project_name);
    Some(repository_root)
}

fn render_new_project_hierarchy(project_root: &Path) -> String {
    let project_name = project_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project");
    DEFAULT_NEW_PROJECT_HIERARCHY
        .replace(
            "__ANT_DELIRIUM_REPOSITORY__",
            &project_root.to_string_lossy().replace('\\', "/"),
        )
        .replace("__ANT_DELIRIUM_PROJECT__", project_name)
}

fn copy_binary_if_needed(source: &Path, target: &Path) -> io::Result<bool> {
    if paths_resolve_to_same_file(source, target)? {
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, target)?;
    Ok(true)
}

fn paths_resolve_to_same_file(left: &Path, right: &Path) -> io::Result<bool> {
    if left == right && right.is_file() {
        return Ok(true);
    }

    if !left.exists() || !right.exists() {
        return Ok(false);
    }

    let left_canonical = fs::canonicalize(left)?;
    let right_canonical = fs::canonicalize(right)?;
    Ok(left_canonical == right_canonical)
}

fn write_default_template_if_missing(path: &Path, content: &str) -> io::Result<bool> {
    if path.is_file() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(true)
}

fn shell_escape(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    if raw.contains(char::is_whitespace) {
        format!("\"{}\"", raw.replace('"', "\\\""))
    } else {
        raw
    }
}

fn plugin_dir_from_config_path(config_path: &Path) -> Option<PathBuf> {
    if config_path.is_dir()
        && config_path.file_name().and_then(|value| value.to_str()) == Some("ant_delirium_config")
    {
        return config_path.parent().map(Path::to_path_buf);
    }

    if config_path.file_name().and_then(|value| value.to_str()) == Some("delirium.toml") {
        return config_path.parent().map(Path::to_path_buf);
    }

    None
}

fn candidate_plugin_roots(project_root: &Path, argv0: Option<&str>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    roots.push(project_root.join("plugins"));
    if let Some(parent) = project_root.parent() {
        roots.push(parent.join("plugins"));
    }
    if let Some(bin) = argv0 {
        let bin_path = Path::new(bin);
        if let Some(parent) = bin_path.parent() {
            roots.push(parent.to_path_buf());
            if let Some(grand_parent) = parent.parent() {
                roots.push(grand_parent.join("plugins"));
            }
        }
    }
    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            roots.push(parent.to_path_buf());
            if let Some(grand_parent) = parent.parent() {
                roots.push(grand_parent.join("plugins"));
            }
        }
    }

    dedup_paths(roots)
}

fn api_root(hierarchy: &HierarchyConfig) -> String {
    for preferred_root in ["hierarchy", "rules", "regras"] {
        if has_semantic_root(hierarchy, preferred_root) {
            return preferred_root.to_string();
        }
    }
    "hierarchy".to_string()
}

fn match_plugin_context(project_root: &Path, plugin_dir: &Path, repository_path: &Path) -> usize {
    let normalized_workspace = normalize_path(project_root);
    let normalized_repository = normalize_path(repository_path);

    if normalized_workspace == normalized_repository {
        return 100;
    }

    let workspace_name = project_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let repository_name = repository_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let plugin_name = plugin_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    if !workspace_name.is_empty() && workspace_name == repository_name {
        return 90;
    }

    if !workspace_name.is_empty() && plugin_name.ends_with(workspace_name) {
        return 70;
    }

    if !repository_name.is_empty() && plugin_name.ends_with(repository_name) {
        return 60;
    }

    0
}

fn extract_repository_path(config_path: &Path) -> Option<PathBuf> {
    let source_path = if config_path.is_dir() {
        let default_hierarchy = config_path.join("default_hierarchy.toml");
        if default_hierarchy.is_file() {
            default_hierarchy
        } else {
            return None;
        }
    } else {
        config_path.to_path_buf()
    };

    let input = fs::read_to_string(source_path).ok()?;
    let hierarchy = HierarchyConfig::parse_brute(&input);
    for root in ["hierarchy", "rules", "regras"] {
        if let Some(repository) = hierarchy.search_params(&format!("{}.repository", root)) {
            let raw = repository.trim();
            if raw.is_empty() {
                continue;
            }
            return Some(PathBuf::from(raw));
        }
    }
    None
}

fn render_agents_report_json(
    plugin: &PluginContext,
    agents: &[ConnectedAgent],
    tasks: &[GovernanceTask],
    manager_tasks: &[ManagerTask],
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    submit_queue: &[SubmitRecord],
) -> String {
    let now = unix_timestamp();
    let active_agents = agents
        .iter()
        .filter(|agent| now - agent.last_ping <= 3600)
        .count();
    let projects = summarize_projects(agents);
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"command\": \"swarm_status\",\n");
    out.push_str(&format!(
        "  \"plugin_dir\": \"{}\",\n",
        json_escape(&plugin.plugin_dir.to_string_lossy())
    ));
    out.push_str(&format!(
        "  \"repository_root\": \"{}\",\n",
        json_escape(&plugin.repository_path.to_string_lossy())
    ));
    out.push_str(&format!(
        "  \"workspace_path\": \"{}\",\n",
        json_escape(&plugin.workspace_path.to_string_lossy())
    ));
    out.push_str(&format!(
        "  \"execution_context\": \"{}\",\n",
        plugin_execution_context(plugin)
    ));
    out.push_str("  \"summary\": {\n");
    out.push_str(&format!("    \"active_agents\": {},\n", active_agents));
    out.push_str(&format!("    \"projects\": {},\n", projects.len()));
    out.push_str(&format!(
        "    \"open_governance_tasks\": {},\n",
        tasks.iter().filter(|task| task.owner_id < 0).count()
    ));
    out.push_str(&format!(
        "    \"open_manager_tasks\": {},\n",
        manager_tasks
            .iter()
            .filter(|task| task.owner_id < 0 && is_manager_task_active(task))
            .count()
    ));
    out.push_str(&format!(
        "    \"ready_submits\": {},\n",
        agents
            .iter()
            .filter(|agent| {
                matches!(
                    resolve_agent_submit_state(agents, agent, messages, feedback, submit_queue)
                        .submit_state
                        .as_str(),
                    "ready_for_root"
                )
            })
            .count()
    ));
    out.push_str(&format!(
        "    \"pending_submits\": {},\n",
        agents
            .iter()
            .filter(|agent| {
                matches!(
                    resolve_agent_submit_state(agents, agent, messages, feedback, submit_queue)
                        .submit_state
                        .as_str(),
                    "message_only" | "feedback_only"
                )
            })
            .count()
    ));
    out.push_str(&format!(
        "    \"blocked_submits\": {},\n",
        agents
            .iter()
            .filter(|agent| {
                matches!(
                    resolve_agent_submit_state(agents, agent, messages, feedback, submit_queue)
                        .submit_state
                        .as_str(),
                    "blocked_rebase"
                )
            })
            .count()
    ));
    out.push_str(&format!(
        "    \"consumed_submits\": {},\n",
        agents
            .iter()
            .filter(|agent| {
                matches!(
                    resolve_agent_submit_state(agents, agent, messages, feedback, submit_queue)
                        .submit_state
                        .as_str(),
                    "consumed"
                )
            })
            .count()
    ));
    out.push_str(&format!(
        "    \"open_submit_records\": {},\n",
        submit_queue
            .iter()
            .filter(|record| record.status == "open")
            .count()
    ));
    out.push_str(&format!(
        "    \"accepted_submit_records\": {},\n",
        submit_queue
            .iter()
            .filter(|record| record.status == "accepted")
            .count()
    ));
    out.push_str(&format!(
        "    \"rejected_submit_records\": {},\n",
        submit_queue
            .iter()
            .filter(|record| record.status == "rejected")
            .count()
    ));
    out.push_str(&format!(
        "    \"closed_submit_records\": {},\n",
        submit_queue
            .iter()
            .filter(|record| record.status == "closed")
            .count()
    ));
    out.push_str(&format!(
        "    \"superseded_submit_records\": {}\n",
        submit_queue
            .iter()
            .filter(|record| record.status == "superseded")
            .count()
    ));
    out.push_str("  },\n");
    out.push_str("  \"projects\": [\n");
    for (idx, project) in projects.iter().enumerate() {
        let comma = if idx + 1 < projects.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{ \"repository_path\": \"{}\", \"agent_count\": {}, \"active_agent_count\": {}, \"worktree_count\": {} }}{}\n",
            json_escape(&project.repository_path),
            project.agent_count,
            project.active_agent_count,
            project.worktree_count,
            comma
        ));
    }
    out.push_str("  ],\n");
    out.push_str("  \"agents\": [\n");
    for (agent_idx, agent) in agents.iter().enumerate() {
        let owned = agent_governance_scope(plugin, agent, tasks);
        let owned_manager: Vec<&ManagerTask> = manager_tasks
            .iter()
            .filter(|task| task.owner_id == agent.internal_id && is_manager_task_active(task))
            .collect();
        let manager_override = !owned_manager.is_empty();
        let pending_messages = count_pending_messages_for_agent(agent.internal_id, messages);
        let violations: i64 = owned.iter().map(|task| task.violation_count).sum();
        let sync = resolve_agent_sync_state(agents, agent);
        let submit = resolve_agent_submit_state(agents, agent, messages, feedback, submit_queue);
        let status = if now - agent.last_ping <= 3600 {
            "active"
        } else {
            "stale"
        };
        let agent_comma = if agent_idx + 1 < agents.len() {
            ","
        } else {
            ""
        };
        out.push_str("    {\n");
        out.push_str(&format!("      \"internal_id\": {},\n", agent.internal_id));
        out.push_str(&format!(
            "      \"agent_id\": \"{}\",\n",
            json_escape(&agent.agent_id)
        ));
        out.push_str(&format!("      \"status\": \"{}\",\n", json_escape(status)));
        out.push_str(&format!(
            "      \"root_scope\": {},\n",
            if is_root_agent_context(plugin, agent) {
                "true"
            } else {
                "false"
            }
        ));
        out.push_str(&format!(
            "      \"branch\": \"{}\",\n",
            json_escape(&agent.branch)
        ));
        out.push_str(&format!(
            "      \"sync_state\": \"{}\",\n",
            json_escape(&sync.sync_state)
        ));
        out.push_str(&format!(
            "      \"rebase_required\": {},\n",
            if sync.rebase_required {
                "true"
            } else {
                "false"
            }
        ));
        out.push_str(&format!(
            "      \"submit_state\": \"{}\",\n",
            json_escape(&submit.submit_state)
        ));
        out.push_str(&format!(
            "      \"submit_queue_status\": \"{}\",\n",
            json_escape(&submit.submit_queue_status)
        ));
        out.push_str(&format!(
            "      \"has_submit_message\": {},\n",
            if submit.has_submit_message {
                "true"
            } else {
                "false"
            }
        ));
        out.push_str(&format!(
            "      \"has_submit_feedback\": {},\n",
            if submit.has_submit_feedback {
                "true"
            } else {
                "false"
            }
        ));
        out.push_str(&format!(
            "      \"manager_override\": {},\n",
            if manager_override { "true" } else { "false" }
        ));
        out.push_str(&format!(
            "      \"active_manager_task_count\": {},\n",
            owned_manager.len()
        ));
        out.push_str(&format!(
            "      \"queued_violation_files\": {},\n",
            owned.len()
        ));
        out.push_str(&format!(
            "      \"queued_violation_count\": {},\n",
            violations
        ));
        out.push_str(&format!(
            "      \"base_revision\": \"{}\",\n",
            json_escape(&sync.base_revision)
        ));
        out.push_str(&format!(
            "      \"workspace_revision\": \"{}\",\n",
            json_escape(&sync.workspace_revision)
        ));
        out.push_str(&format!(
            "      \"repository_revision\": \"{}\",\n",
            json_escape(&sync.repository_revision)
        ));
        out.push_str(&format!(
            "      \"worktree_path\": \"{}\",\n",
            json_escape(&agent.worktree_path)
        ));
        out.push_str(&format!(
            "      \"login_path\": \"{}\",\n",
            json_escape(&agent.login_path)
        ));
        out.push_str(&format!(
            "      \"execution_context\": \"{}\",\n",
            json_escape(agent_execution_context(plugin, agent))
        ));
        out.push_str(&format!(
            "      \"assigned_mission\": \"{}\",\n",
            json_escape(&agent.assigned_mission)
        ));
        out.push_str(&format!(
            "      \"pending_messages\": {},\n",
            pending_messages
        ));
        out.push_str(&format!("      \"violation_count\": {},\n", violations));
        out.push_str("      \"manager_tasks\": [\n");
        for (idx, task) in owned_manager.iter().enumerate() {
            let comma = if idx + 1 < owned_manager.len() {
                ","
            } else {
                ""
            };
            out.push_str(&format!(
                "        {{ \"title\": \"{}\", \"priority\": {}, \"file_path\": \"{}\", \"status\": \"{}\" }}{}\n",
                json_escape(&task.title),
                task.priority,
                json_escape(&task.file_path),
                json_escape(&task.status),
                comma
            ));
        }
        out.push_str("      ],\n");
        out.push_str("      \"governance_tasks\": [\n");
        for (idx, task) in owned.iter().enumerate() {
            let comma = if idx + 1 < owned.len() { "," } else { "" };
            out.push_str(&format!(
                "        {{ \"file_path\": \"{}\", \"violation_count\": {} }}{}\n",
                json_escape(&task.file_path),
                task.violation_count,
                comma
            ));
        }
        out.push_str("      ]\n");
        out.push_str(&format!("    }}{}\n", agent_comma));
    }
    out.push_str("  ]\n");
    out.push_str("}\n");
    out
}

fn render_single_agent_identity(
    plugin: &PluginContext,
    agents: &[ConnectedAgent],
    agent: &ConnectedAgent,
    tasks: &[GovernanceTask],
    manager_tasks: &[ManagerTask],
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    submit_queue: &[SubmitRecord],
    json_requested: bool,
) -> String {
    let owned = agent_governance_scope(plugin, agent, tasks);
    let owned_manager: Vec<&ManagerTask> = manager_tasks
        .iter()
        .filter(|task| task.owner_id == agent.internal_id && is_manager_task_active(task))
        .collect();
    let manager_override = !owned_manager.is_empty();
    let pending_messages = count_pending_messages_for_agent(agent.internal_id, messages);
    let violations: i64 = owned.iter().map(|task| task.violation_count).sum();
    let repository_path = resolve_repository_path_for_agent(agent);
    let sync = resolve_agent_sync_state(agents, agent);
    let submit = resolve_agent_submit_state(agents, agent, messages, feedback, submit_queue);
    let status = if unix_timestamp() - agent.last_ping <= 3600 {
        "active"
    } else {
        "stale"
    };

    if json_requested {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str("  \"command\": \"agent_identity\",\n");
        out.push_str(&format!("  \"internal_id\": {},\n", agent.internal_id));
        out.push_str(&format!(
            "  \"agent_id\": \"{}\",\n",
            json_escape(&agent.agent_id)
        ));
        out.push_str(&format!("  \"status\": \"{}\",\n", json_escape(status)));
        out.push_str(&format!(
            "  \"root_scope\": {},\n",
            if is_root_agent_context(plugin, agent) {
                "true"
            } else {
                "false"
            }
        ));
        out.push_str(&format!(
            "  \"branch\": \"{}\",\n",
            json_escape(&agent.branch)
        ));
        out.push_str(&format!(
            "  \"sync_state\": \"{}\",\n",
            json_escape(&sync.sync_state)
        ));
        out.push_str(&format!(
            "  \"rebase_required\": {},\n",
            if sync.rebase_required {
                "true"
            } else {
                "false"
            }
        ));
        out.push_str(&format!(
            "  \"submit_state\": \"{}\",\n",
            json_escape(&submit.submit_state)
        ));
        out.push_str(&format!(
            "  \"submit_queue_status\": \"{}\",\n",
            json_escape(&submit.submit_queue_status)
        ));
        out.push_str(&format!(
            "  \"has_submit_message\": {},\n",
            if submit.has_submit_message {
                "true"
            } else {
                "false"
            }
        ));
        out.push_str(&format!(
            "  \"has_submit_feedback\": {},\n",
            if submit.has_submit_feedback {
                "true"
            } else {
                "false"
            }
        ));
        out.push_str(&format!(
            "  \"manager_override\": {},\n",
            if manager_override { "true" } else { "false" }
        ));
        out.push_str(&format!(
            "  \"active_manager_task_count\": {},\n",
            owned_manager.len()
        ));
        out.push_str(&format!("  \"queued_violation_files\": {},\n", owned.len()));
        out.push_str(&format!("  \"queued_violation_count\": {},\n", violations));
        out.push_str(&format!(
            "  \"repository_revision\": \"{}\",\n",
            json_escape(&sync.repository_revision)
        ));
        out.push_str(&format!(
            "  \"workspace_revision\": \"{}\",\n",
            json_escape(&sync.workspace_revision)
        ));
        out.push_str(&format!(
            "  \"base_revision\": \"{}\",\n",
            json_escape(&sync.base_revision)
        ));
        out.push_str(&format!(
            "  \"repository_path\": \"{}\",\n",
            json_escape(&repository_path)
        ));
        out.push_str(&format!(
            "  \"workspace_path\": \"{}\",\n",
            json_escape(&agent.worktree_path)
        ));
        out.push_str(&format!(
            "  \"login_path\": \"{}\",\n",
            json_escape(&agent.login_path)
        ));
        out.push_str(&format!(
            "  \"execution_context\": \"{}\",\n",
            json_escape(agent_execution_context(plugin, agent))
        ));
        out.push_str(&format!(
            "  \"assigned_mission\": \"{}\",\n",
            json_escape(&agent.assigned_mission)
        ));
        out.push_str(&format!("  \"pending_messages\": {},\n", pending_messages));
        out.push_str(&format!("  \"violation_count\": {},\n", violations));
        out.push_str("  \"manager_tasks\": [\n");
        for (idx, task) in owned_manager.iter().enumerate() {
            let comma = if idx + 1 < owned_manager.len() {
                ","
            } else {
                ""
            };
            out.push_str(&format!(
                "    {{ \"title\": \"{}\", \"priority\": {}, \"file_path\": \"{}\", \"status\": \"{}\" }}{}\n",
                json_escape(&task.title),
                task.priority,
                json_escape(&task.file_path),
                json_escape(&task.status),
                comma
            ));
        }
        out.push_str("  ],\n");
        out.push_str("  \"governance_tasks\": [\n");
        for (idx, task) in owned.iter().enumerate() {
            let comma = if idx + 1 < owned.len() { "," } else { "" };
            out.push_str(&format!(
                "    {{ \"file_path\": \"{}\", \"violation_count\": {} }}{}\n",
                json_escape(&task.file_path),
                task.violation_count,
                comma
            ));
        }
        out.push_str("  ]\n");
        out.push_str("}\n");
        return out;
    }

    let mut out = String::new();
    out.push_str(&format!(
        "Agent {} ({})\n\n",
        agent.internal_id, agent.agent_id
    ));
    out.push_str(&format!(
        "Agent ID: {}\nStatus: {}\nRoot scope: {}\nBranch: {}\nSync state: {}\nRebase required: {}\nSubmit state: {}\nSubmit queue status: {}\nSubmit message: {}\nSubmit feedback: {}\nManager override: {}\nActive manager tasks: {}\nQueued violation files: {}\nQueued violations: {}\nRepository revision: {}\nWorkspace revision: {}\nBase revision: {}\nRepository: {}\nWorkspace: {}\nLogin: {}\nExecution context: {}\nMission: {}\nPending messages: {}\nViolations: {}\n",
        agent.agent_id,
        status,
        if is_root_agent_context(plugin, agent) { "yes" } else { "no" },
        empty_or_dash(&agent.branch),
        sync.sync_state,
        if sync.rebase_required { "yes" } else { "no" },
        submit.submit_state,
        empty_or_dash(&submit.submit_queue_status),
        if submit.has_submit_message { "yes" } else { "no" },
        if submit.has_submit_feedback { "yes" } else { "no" },
        if manager_override { "yes" } else { "no" },
        owned_manager.len(),
        owned.len(),
        violations,
        empty_or_dash(&sync.repository_revision),
        empty_or_dash(&sync.workspace_revision),
        empty_or_dash(&sync.base_revision),
        repository_path,
        agent.worktree_path,
        agent.login_path,
        agent_execution_context(plugin, agent),
        empty_or_dash(&agent.assigned_mission),
        pending_messages,
        violations
    ));
    if !owned_manager.is_empty() {
        out.push_str("\nManager tasks:\n");
        for task in owned_manager {
            out.push_str(&format!(
                "- {} priority={} file={} status={}\n",
                task.title,
                task.priority,
                empty_or_dash(&task.file_path),
                task.status
            ));
        }
    }
    if !owned.is_empty() {
        out.push_str("\nGovernance tasks:\n");
        for task in owned {
            out.push_str(&format!(
                "- {} violations={}\n",
                task.file_path, task.violation_count
            ));
        }
    }
    out
}

fn render_message_entry_text(entry: &JsonEntry, agents: &[ConnectedAgent]) -> String {
    let sender_agent_id = extract_entry_string(entry, "sender_agent_id").unwrap_or_default();
    let sender_internal_id = extract_entry_number(entry, "sender_internal_id").unwrap_or(-1);
    let target_agent_id = extract_entry_string(entry, "target_agent_id").unwrap_or_default();
    let target_internal_id = extract_entry_number(entry, "target_internal_id").unwrap_or(-1);
    let topic = extract_entry_string(entry, "topic").unwrap_or_default();
    let visibility = extract_entry_string(entry, "visibility").unwrap_or("public");
    let body = extract_entry_string(entry, "body").unwrap_or_default();
    let workspace = extract_entry_string(entry, "workspace_path").unwrap_or_default();
    let login = extract_entry_string(entry, "login_path").unwrap_or_default();
    let repository = extract_entry_string(entry, "repository_path").unwrap_or_default();
    let branch = extract_entry_string(entry, "branch").unwrap_or_default();
    let base_revision = extract_entry_string(entry, "base_revision").unwrap_or_default();
    let workspace_revision = extract_entry_string(entry, "workspace_revision").unwrap_or_default();
    let repository_revision =
        extract_entry_string(entry, "repository_revision").unwrap_or_default();
    let operation_kind = extract_entry_string(entry, "operation_kind").unwrap_or("general");
    let execution_context = extract_entry_string(entry, "execution_context").unwrap_or_default();
    let assigned_mission = extract_entry_string(entry, "assigned_mission").unwrap_or_default();
    let created_at = extract_entry_number(entry, "created_at").unwrap_or_default();
    let sender_status = resolve_agent_by_agent_id(agents, sender_agent_id)
        .map(|_| "known")
        .unwrap_or("unknown");

    format!(
        "- from_agent={} (# {}) [{}] to_agent={} (# {}) topic={} kind={} visibility={} context={} repository={} branch={} base_revision={} workspace_revision={} repository_revision={} workspace={} login={} mission={} at={} body={}\n",
        empty_or_dash(&sender_agent_id),
        sender_internal_id,
        sender_status,
        empty_or_dash(&target_agent_id),
        target_internal_id,
        empty_or_dash(&topic),
        empty_or_dash(&operation_kind),
        empty_or_dash(&visibility),
        empty_or_dash(&execution_context),
        empty_or_dash(&repository),
        empty_or_dash(&branch),
        empty_or_dash(&base_revision),
        empty_or_dash(&workspace_revision),
        empty_or_dash(&repository_revision),
        empty_or_dash(&workspace),
        empty_or_dash(&login),
        empty_or_dash(&assigned_mission),
        created_at,
        empty_or_dash(&body)
    )
}

fn render_message_entry_json(entry: &JsonEntry, comma: &str) -> String {
    let mut out = String::new();
    out.push_str("    {\n");
    for (field_idx, (name, value)) in entry.fields.iter().enumerate() {
        let field_comma = if field_idx + 1 < entry.fields.len() {
            ","
        } else {
            ""
        };
        match value {
            JsonValue::String(value) => out.push_str(&format!(
                "      \"{}\": \"{}\"{}\n",
                json_escape(name),
                json_escape(value),
                field_comma
            )),
            JsonValue::Number(value) => out.push_str(&format!(
                "      \"{}\": {}{}\n",
                json_escape(name),
                value,
                field_comma
            )),
        }
    }
    out.push_str(&format!("    }}{}\n", comma));
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectSummary {
    repository_path: String,
    agent_count: usize,
    active_agent_count: usize,
    worktree_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceLoginSnapshot {
    agent_id: String,
    branch: String,
    base_revision: String,
    workspace_revision: String,
    repository_revision: String,
    repository_path: String,
    workspace_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SyncMetadata {
    branch: String,
    base_revision: String,
    workspace_revision: String,
    repository_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentSyncState {
    sync_state: String,
    repository_revision: String,
    workspace_revision: String,
    base_revision: String,
    rebase_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentSubmitState {
    submit_state: String,
    has_submit_message: bool,
    has_submit_feedback: bool,
    submit_queue_status: String,
    latest_submit_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RootOrchestrationFinding {
    code: String,
    message: String,
}

fn load_runtime_state(
    plugin: &PluginContext,
) -> io::Result<(
    Vec<ConnectedAgent>,
    Vec<GovernanceTask>,
    Vec<ManagerTask>,
    Vec<JsonEntry>,
)> {
    ensure_support_files(plugin)?;
    let mut agents = read_connected_agents(&plugin.connected_path())?;
    let mut tasks = read_governance_tasks(&plugin.governance_path())?;
    let mut manager_tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
    let now = unix_timestamp();
    normalize_agent_topology(plugin, &mut agents, &mut tasks, &mut manager_tasks, now);
    reconcile_agents_with_login_paths(plugin, &mut agents, &mut tasks, &mut manager_tasks, now)?;
    let mut messages = read_message_entries(&plugin.messages_path(), "messages")?;
    if prune_message_entries(&agents, &mut messages) {
        write_entry_collection(&plugin.messages_path(), "messages", &messages)?;
    }
    let mut feedback = read_message_entries(&plugin.feedback_path(), "entries")?;
    if prune_feedback_entries(&mut feedback) {
        write_entry_collection(&plugin.feedback_path(), "entries", &feedback)?;
    }
    Ok((agents, tasks, manager_tasks, messages))
}

fn load_feedback_entries(plugin: &PluginContext) -> io::Result<Vec<JsonEntry>> {
    let mut feedback = read_message_entries(&plugin.feedback_path(), "entries")?;
    if prune_feedback_entries(&mut feedback) {
        write_entry_collection(&plugin.feedback_path(), "entries", &feedback)?;
    }
    Ok(feedback)
}

fn reconcile_agents_with_login_paths(
    plugin: &PluginContext,
    agents: &mut Vec<ConnectedAgent>,
    tasks: &mut Vec<GovernanceTask>,
    manager_tasks: &mut Vec<ManagerTask>,
    now: i64,
) -> io::Result<()> {
    let before_agents = agents.clone();
    let before_tasks = tasks.clone();
    let before_manager_tasks = manager_tasks.clone();

    agents.retain(|agent| {
        if agent.internal_id == 0
            && normalize_path_str(&agent.worktree_path)
                == normalize_path(plugin.repository_path.as_path())
        {
            return true;
        }

        let login_path = Path::new(&agent.login_path);
        if !login_path.is_file() {
            return false;
        }

        let Some(snapshot) = read_workspace_login_snapshot(login_path) else {
            return false;
        };

        normalize_path_str(&snapshot.workspace_path) == normalize_path_str(&agent.worktree_path)
    });

    normalize_agent_topology(plugin, agents, tasks, manager_tasks, now);

    if *agents != before_agents {
        write_connected_agents(&plugin.connected_path(), agents)?;
    }
    if *tasks != before_tasks {
        write_governance_tasks(&plugin.governance_path(), tasks)?;
    }
    if *manager_tasks != before_manager_tasks {
        write_manager_tasks(&plugin.manager_tasks_path(), manager_tasks)?;
    }

    Ok(())
}

fn collect_root_orchestration_findings(
    plugin: &PluginContext,
) -> io::Result<Vec<RootOrchestrationFinding>> {
    ensure_support_files(plugin)?;
    let mut agents = read_connected_agents(&plugin.connected_path())?;
    let mut tasks = read_governance_tasks(&plugin.governance_path())?;
    let mut manager_tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
    let now = unix_timestamp();
    normalize_agent_topology(plugin, &mut agents, &mut tasks, &mut manager_tasks, now);
    reconcile_agents_with_login_paths(plugin, &mut agents, &mut tasks, &mut manager_tasks, now)?;
    let messages = read_message_entries(&plugin.messages_path(), "messages")?;
    let feedback = read_message_entries(&plugin.feedback_path(), "entries")?;
    let submit_queue = read_submit_records(&plugin.submit_queue_path())?;

    let mut findings = Vec::new();
    findings.extend(check_duplicate_agent_identities(&agents));
    findings.extend(check_governance_ownership_integrity(&agents, &tasks));
    findings.extend(check_manager_task_ownership_integrity(
        &agents,
        &manager_tasks,
    ));
    findings.extend(check_message_integrity(&agents, &messages));
    findings.extend(check_feedback_integrity(&feedback));
    findings.extend(check_submit_evidence(
        &agents,
        &messages,
        &feedback,
        &submit_queue,
    ));
    findings.extend(check_submit_queue_integrity(&agents, &submit_queue));
    findings.extend(check_submit_sync_conflicts(
        &agents,
        &messages,
        &feedback,
        &submit_queue,
    ));
    findings.extend(check_stale_active_agents(&agents));

    Ok(findings)
}

fn check_duplicate_agent_identities(agents: &[ConnectedAgent]) -> Vec<RootOrchestrationFinding> {
    let mut findings = Vec::new();
    let mut agent_ids: BTreeMap<String, usize> = BTreeMap::new();
    let mut login_paths: BTreeMap<String, usize> = BTreeMap::new();

    for agent in agents {
        *agent_ids.entry(agent.agent_id.clone()).or_default() += 1;
        *login_paths
            .entry(normalize_path_str(&agent.login_path))
            .or_default() += 1;
    }

    for (agent_id, count) in agent_ids {
        if !agent_id.trim().is_empty() && count > 1 {
            findings.push(RootOrchestrationFinding {
                code: "duplicate_agent_identity".to_string(),
                message: format!(
                    "agent_id {} appears {} times in connected.json",
                    agent_id, count
                ),
            });
        }
    }

    for (login_path, count) in login_paths {
        if !login_path.trim().is_empty() && count > 1 {
            findings.push(RootOrchestrationFinding {
                code: "duplicate_login_path".to_string(),
                message: format!(
                    "login_path {} appears {} times in connected.json",
                    login_path, count
                ),
            });
        }
    }

    findings
}

fn check_governance_ownership_integrity(
    agents: &[ConnectedAgent],
    tasks: &[GovernanceTask],
) -> Vec<RootOrchestrationFinding> {
    let mut findings = Vec::new();
    let valid_agent_ids: HashSet<i64> = agents.iter().map(|agent| agent.internal_id).collect();
    let mut owner_by_file: BTreeMap<String, i64> = BTreeMap::new();

    for task in tasks {
        let normalized_path = normalize_task_path(&task.file_path);
        if task.owner_id >= 0 && !valid_agent_ids.contains(&task.owner_id) {
            findings.push(RootOrchestrationFinding {
                code: "invalid_governance_owner".to_string(),
                message: format!(
                    "file {} is owned by unknown agent {}",
                    task.file_path, task.owner_id
                ),
            });
        }
        if normalized_path.starts_with("plugins/")
            || normalized_path.contains("/plugins/")
            || normalized_path.starts_with("AC_Ant-Delirium_props/")
            || normalized_path.contains("/AC_Ant-Delirium_props/")
        {
            findings.push(RootOrchestrationFinding {
                code: "sovereign_file_in_governance".to_string(),
                message: format!(
                    "file {} entered governance ownership but should stay under root-only control",
                    task.file_path
                ),
            });
        }
        if let Some(existing_owner) = owner_by_file.get(&normalized_path) {
            if *existing_owner != task.owner_id {
                findings.push(RootOrchestrationFinding {
                    code: "ambiguous_governance_ownership".to_string(),
                    message: format!(
                        "file {} appears with conflicting owners {} and {}",
                        task.file_path, existing_owner, task.owner_id
                    ),
                });
            }
        } else {
            owner_by_file.insert(normalized_path, task.owner_id);
        }
    }

    findings
}

fn check_manager_task_ownership_integrity(
    agents: &[ConnectedAgent],
    manager_tasks: &[ManagerTask],
) -> Vec<RootOrchestrationFinding> {
    let valid_agent_ids: HashSet<i64> = agents.iter().map(|agent| agent.internal_id).collect();
    manager_tasks
        .iter()
        .filter(|task| task.owner_id >= 0 && !valid_agent_ids.contains(&task.owner_id))
        .map(|task| RootOrchestrationFinding {
            code: "invalid_manager_task_owner".to_string(),
            message: format!(
                "manager task {} is claimed by unknown agent {}",
                task.title, task.owner_id
            ),
        })
        .collect()
}

fn check_message_integrity(
    agents: &[ConnectedAgent],
    messages: &[JsonEntry],
) -> Vec<RootOrchestrationFinding> {
    let mut findings = Vec::new();
    for entry in messages {
        if !is_valid_message_entry(agents, entry) {
            findings.push(RootOrchestrationFinding {
                code: "invalid_message_entry".to_string(),
                message: format!(
                    "message entry at {} has invalid routing or incomplete sync metadata",
                    extract_entry_number(entry, "created_at").unwrap_or_default()
                ),
            });
        }
    }
    findings
}

fn check_feedback_integrity(feedback: &[JsonEntry]) -> Vec<RootOrchestrationFinding> {
    let mut findings = Vec::new();
    for entry in feedback {
        if !is_valid_feedback_entry(entry) {
            findings.push(RootOrchestrationFinding {
                code: "invalid_feedback_entry".to_string(),
                message: format!(
                    "feedback entry at {} has invalid identity or incomplete sync metadata",
                    extract_entry_number(entry, "created_at").unwrap_or_default()
                ),
            });
        }
    }
    findings
}

fn check_submit_evidence(
    agents: &[ConnectedAgent],
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    submit_queue: &[SubmitRecord],
) -> Vec<RootOrchestrationFinding> {
    let mut findings = Vec::new();

    for agent in agents.iter().filter(|agent| agent.internal_id != 0) {
        let has_any_submit_record = latest_submit_record_for_agent(submit_queue, &agent.agent_id).is_some();
        let has_active_submit_record =
            latest_active_submit_record_for_agent(submit_queue, &agent.agent_id).is_some();
        let has_submit_message = messages.iter().any(|entry| {
            extract_entry_string(entry, "sender_agent_id")
                .map(|sender| sender == agent.agent_id)
                .unwrap_or(false)
                && extract_entry_string(entry, "operation_kind")
                    .map(|kind| kind == "submit")
                    .unwrap_or(false)
        });
        let has_submit_feedback = feedback.iter().any(|entry| {
            extract_entry_string(entry, "sender_agent_id")
                .map(|sender| sender == agent.agent_id)
                .unwrap_or(false)
                && extract_entry_string(entry, "operation_kind")
                    .map(|kind| kind == "submit")
                    .unwrap_or(false)
        });

        if !has_active_submit_record {
            if (has_submit_message || has_submit_feedback) && !has_any_submit_record {
                findings.push(RootOrchestrationFinding {
                    code: "missing_submit_queue_record".to_string(),
                    message: format!(
                        "agent {} has submit evidence but no submit queue record",
                        agent.agent_id
                    ),
                });
            }
            continue;
        }

        if has_submit_message && !has_submit_feedback {
            findings.push(RootOrchestrationFinding {
                code: "missing_submit_feedback".to_string(),
                message: format!(
                    "agent {} announced submit publicly but has no matching submit feedback",
                    agent.agent_id
                ),
            });
        }
        if has_submit_feedback && !has_submit_message {
            findings.push(RootOrchestrationFinding {
                code: "missing_submit_message".to_string(),
                message: format!(
                    "agent {} stored submit feedback but has no matching public submit message",
                    agent.agent_id
                ),
            });
        }
    }

    findings
}

fn check_submit_sync_conflicts(
    agents: &[ConnectedAgent],
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    submit_queue: &[SubmitRecord],
) -> Vec<RootOrchestrationFinding> {
    let mut findings = Vec::new();

    for agent in agents.iter().filter(|agent| agent.internal_id != 0) {
        if latest_active_submit_record_for_agent(submit_queue, &agent.agent_id).is_none() {
            continue;
        }
        let submit = resolve_agent_submit_state(agents, agent, messages, feedback, submit_queue);
        if matches!(submit.submit_state.as_str(), "blocked_rebase") {
            findings.push(RootOrchestrationFinding {
                code: "submit_requires_rebase".to_string(),
                message: format!(
                    "agent {} requested submit but persisted metadata says rebase is still required",
                    agent.agent_id
                ),
            });
        }
    }

    findings
}

fn check_submit_queue_integrity(
    agents: &[ConnectedAgent],
    submit_queue: &[SubmitRecord],
) -> Vec<RootOrchestrationFinding> {
    let valid_agent_ids: HashSet<String> =
        agents.iter().map(|agent| agent.agent_id.clone()).collect();
    submit_queue
        .iter()
        .filter(|entry| !valid_agent_ids.contains(&entry.agent_id))
        .map(|entry| RootOrchestrationFinding {
            code: "invalid_submit_queue_entry".to_string(),
            message: format!(
                "submit {} references unknown agent {}",
                entry.id, entry.agent_id
            ),
        })
        .collect()
}

fn check_stale_active_agents(agents: &[ConnectedAgent]) -> Vec<RootOrchestrationFinding> {
    let now = unix_timestamp();
    agents
        .iter()
        .filter(|agent| agent.internal_id != 0)
        .filter(|agent| now - agent.last_ping > 3600)
        .filter(|agent| !agent.assigned_mission.trim().is_empty())
        .map(|agent| RootOrchestrationFinding {
            code: "stale_agent_with_active_mission".to_string(),
            message: format!(
                "agent {} is stale but still holds active mission {}",
                agent.agent_id, agent.assigned_mission
            ),
        })
        .collect()
}

fn summarize_projects(agents: &[ConnectedAgent]) -> Vec<ProjectSummary> {
    let now = unix_timestamp();
    let mut grouped: BTreeMap<String, ProjectSummary> = BTreeMap::new();

    for agent in agents {
        let repository_path = resolve_repository_path_for_agent(agent);

        let entry = grouped
            .entry(repository_path.clone())
            .or_insert(ProjectSummary {
                repository_path,
                agent_count: 0,
                active_agent_count: 0,
                worktree_count: 0,
            });
        entry.agent_count += 1;
        if now - agent.last_ping <= 3600 {
            entry.active_agent_count += 1;
        }
        if agent.internal_id != 0 {
            entry.worktree_count += 1;
        }
    }

    grouped.into_values().collect()
}

fn read_workspace_login_snapshot(path: &Path) -> Option<WorkspaceLoginSnapshot> {
    let raw = fs::read_to_string(path).ok()?;
    let workspace_path = extract_string_field(&raw, "workspace_path")?;
    Some(WorkspaceLoginSnapshot {
        agent_id: extract_string_field(&raw, "agent_id")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| agent_id_for_workspace_path(Path::new(&workspace_path))),
        branch: extract_string_field(&raw, "branch").unwrap_or_default(),
        base_revision: extract_string_field(&raw, "base_revision").unwrap_or_default(),
        workspace_revision: extract_string_field(&raw, "workspace_revision").unwrap_or_default(),
        repository_revision: extract_string_field(&raw, "repository_revision").unwrap_or_default(),
        repository_path: extract_string_field(&raw, "repository_path")?,
        workspace_path,
    })
}

fn prune_message_entries(agents: &[ConnectedAgent], entries: &mut Vec<JsonEntry>) -> bool {
    let original_len = entries.len();
    entries.retain(|entry| is_valid_message_entry(agents, entry));
    entries.len() != original_len
}

fn prune_feedback_entries(entries: &mut Vec<JsonEntry>) -> bool {
    let original_len = entries.len();
    entries.retain(is_valid_feedback_entry);
    entries.len() != original_len
}

fn is_valid_message_entry(agents: &[ConnectedAgent], entry: &JsonEntry) -> bool {
    let Some(body) = extract_entry_string(entry, "body").map(str::trim) else {
        return false;
    };
    if body.is_empty() {
        return false;
    }

    let Some(sender_agent_id) = extract_entry_string(entry, "sender_agent_id") else {
        return false;
    };
    if sender_agent_id.trim().is_empty() || sender_agent_id == "broadcast" {
        return false;
    }

    let Some(target_agent_id) = extract_entry_string(entry, "target_agent_id") else {
        return false;
    };
    let Some(sender_internal_id) = extract_entry_number(entry, "sender_internal_id") else {
        return false;
    };
    let Some(target_internal_id) = extract_entry_number(entry, "target_internal_id") else {
        return false;
    };
    if target_agent_id != "broadcast"
        && resolve_agent_by_agent_id(agents, target_agent_id).is_none()
    {
        return false;
    }

    let Some(workspace_path) = extract_entry_string(entry, "workspace_path") else {
        return false;
    };
    let Some(login_path) = extract_entry_string(entry, "login_path") else {
        return false;
    };
    let Some(repository_path) = extract_entry_string(entry, "repository_path") else {
        return false;
    };
    let Some(visibility) = extract_entry_string(entry, "visibility") else {
        return false;
    };
    if visibility.trim() != "public" {
        return false;
    }
    let operation_kind = extract_entry_string(entry, "operation_kind").unwrap_or("general");
    let metadata = sync_metadata_from_cli(
        extract_entry_string(entry, "branch"),
        extract_entry_string(entry, "base_revision"),
        extract_entry_string(entry, "workspace_revision"),
        extract_entry_string(entry, "repository_revision"),
    );
    if validate_operation_metadata("message", operation_kind, &metadata).is_err() {
        return false;
    }

    let Some(sender_agent) = resolve_agent_by_agent_id(agents, sender_agent_id) else {
        return false;
    };
    if sender_agent.internal_id != sender_internal_id {
        return false;
    }
    if target_agent_id == "broadcast" {
        if target_internal_id != -1 {
            return false;
        }
    } else if let Some(target_agent) = resolve_agent_by_agent_id(agents, target_agent_id) {
        if target_agent.internal_id != target_internal_id {
            return false;
        }
    } else {
        return false;
    }
    if normalize_path_str(&sender_agent.worktree_path) != normalize_path_str(workspace_path) {
        return false;
    }
    if normalize_path_str(&sender_agent.login_path) != normalize_path_str(login_path) {
        return false;
    }

    let Some(snapshot) = read_workspace_login_snapshot(Path::new(login_path)) else {
        return false;
    };
    if snapshot.agent_id != sender_agent.agent_id {
        return false;
    }
    normalize_path_str(&snapshot.workspace_path) == normalize_path_str(workspace_path)
        && normalize_path_str(&snapshot.repository_path) == normalize_path_str(repository_path)
}

fn is_valid_feedback_entry(entry: &JsonEntry) -> bool {
    let Some(body) = extract_entry_string(entry, "body").map(str::trim) else {
        return false;
    };
    if body.is_empty() {
        return false;
    }

    let Some(agent_id) = extract_entry_string(entry, "agent_id") else {
        return false;
    };
    let Some(sender_agent_id) = extract_entry_string(entry, "sender_agent_id") else {
        return false;
    };
    if agent_id.trim().is_empty() || sender_agent_id != agent_id {
        return false;
    }
    let Some(internal_id) = extract_entry_number(entry, "internal_id") else {
        return false;
    };
    let Some(sender_internal_id) = extract_entry_number(entry, "sender_internal_id") else {
        return false;
    };
    if internal_id < 0 || sender_internal_id != internal_id {
        return false;
    }

    let Some(workspace_path) = extract_entry_string(entry, "workspace_path") else {
        return false;
    };
    let Some(login_path) = extract_entry_string(entry, "login_path") else {
        return false;
    };
    let Some(repository_path) = extract_entry_string(entry, "repository_path") else {
        return false;
    };
    let operation_kind = extract_entry_string(entry, "operation_kind").unwrap_or("general");
    let metadata = sync_metadata_from_cli(
        extract_entry_string(entry, "branch"),
        extract_entry_string(entry, "base_revision"),
        extract_entry_string(entry, "workspace_revision"),
        extract_entry_string(entry, "repository_revision"),
    );
    if validate_operation_metadata("feedback", operation_kind, &metadata).is_err() {
        return false;
    }

    let Some(snapshot) = read_workspace_login_snapshot(Path::new(login_path)) else {
        return false;
    };
    if snapshot.agent_id != agent_id {
        return false;
    }
    normalize_path_str(&snapshot.workspace_path) == normalize_path_str(workspace_path)
        && normalize_path_str(&snapshot.repository_path) == normalize_path_str(repository_path)
}

fn resolve_repository_path_for_agent(agent: &ConnectedAgent) -> String {
    if let Some(snapshot) = read_workspace_login_snapshot(Path::new(&agent.login_path)) {
        return snapshot.repository_path;
    }

    if agent.internal_id == 0 {
        return agent.worktree_path.clone();
    }

    Path::new(&agent.worktree_path)
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(extract_repository_name_from_workspace_name)
        .and_then(|repository_name| {
            Path::new(&agent.worktree_path)
                .parent()
                .map(|parent| parent.join(repository_name))
        })
        .unwrap_or_else(|| PathBuf::from(&agent.worktree_path))
        .to_string_lossy()
        .to_string()
}

fn extract_repository_name_from_workspace_name(workspace_name: &str) -> Option<&str> {
    workspace_name
        .split_once("_")
        .map(|(_, repository_name)| repository_name)
}

fn ensure_support_files(plugin: &PluginContext) -> io::Result<()> {
    fs::create_dir_all(plugin.login_dir())?;
    if !plugin.connected_path().exists() {
        write_connected_agents(&plugin.connected_path(), &[])?;
    }
    if !plugin.governance_path().exists() {
        write_governance_tasks(&plugin.governance_path(), &[])?;
    }
    if !plugin.manager_tasks_path().exists() {
        write_manager_tasks(&plugin.manager_tasks_path(), &[])?;
    }
    if !plugin.messages_path().exists() {
        write_entry_collection(&plugin.messages_path(), "messages", &[])?;
    }
    if !plugin.feedback_path().exists() {
        write_entry_collection(&plugin.feedback_path(), "entries", &[])?;
    }
    if !plugin.submit_queue_path().exists() {
        write_submit_records(&plugin.submit_queue_path(), &[])?;
    }
    Ok(())
}

fn write_context_snapshot(plugin: &PluginContext) -> io::Result<()> {
    let agents = read_connected_agents(&plugin.connected_path()).unwrap_or_default();
    let manager_tasks = read_manager_tasks(&plugin.manager_tasks_path()).unwrap_or_default();
    let workspace = normalize_path(plugin.workspace_path.as_path());
    let current_agent = agents
        .iter()
        .find(|agent| normalize_path_str(&agent.worktree_path) == workspace);
    let current_manager_task = current_agent.and_then(|agent| {
        manager_tasks
            .iter()
            .find(|task| task.owner_id == agent.internal_id && is_manager_task_active(task))
    });
    let output = format!(
        "{{\n  \"repository_path\": \"{}\",\n  \"active_workspace_path\": \"{}\",\n  \"login_path\": \"{}\",\n  \"plugin_path\": \"{}\",\n  \"execution_context\": \"{}\",\n  \"root_scope\": {},\n  \"current_agent_id\": {},\n  \"current_agent_uid\": \"{}\",\n  \"current_branch\": \"{}\",\n  \"current_base_revision\": \"{}\",\n  \"current_workspace_revision\": \"{}\",\n  \"current_repository_revision\": \"{}\",\n  \"current_manager_task\": \"{}\"\n}}\n",
        json_escape(&plugin.repository_path.to_string_lossy()),
        json_escape(&plugin.workspace_path.to_string_lossy()),
        json_escape(&plugin.login_path().to_string_lossy()),
        json_escape(&plugin.plugin_dir.to_string_lossy()),
        plugin_execution_context(plugin),
        if current_agent
            .map(|agent| is_root_agent_context(plugin, agent))
            .unwrap_or_else(|| is_repository_root_context(plugin))
        {
            "true"
        } else {
            "false"
        },
        current_agent.map(|agent| agent.internal_id).unwrap_or(-1),
        json_escape(current_agent.map(|agent| agent.agent_id.as_str()).unwrap_or("")),
        json_escape(current_agent.map(|agent| agent.branch.as_str()).unwrap_or("")),
        json_escape(current_agent.map(|agent| agent.base_revision.as_str()).unwrap_or("")),
        json_escape(current_agent.map(|agent| agent.workspace_revision.as_str()).unwrap_or("")),
        json_escape(current_agent.map(|agent| agent.repository_revision.as_str()).unwrap_or("")),
        json_escape(
            current_manager_task
                .map(|task| task.title.as_str())
                .unwrap_or(""),
        ),
    );
    fs::write(plugin.context_path(), output)
}

fn read_connected_agents(path: &Path) -> io::Result<Vec<ConnectedAgent>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, "agents");
    Ok(objects
        .into_iter()
        .map(|object| {
            let worktree_path = extract_string_field(&object, "worktree_path").unwrap_or_default();
            let login_path = extract_string_field(&object, "login_path")
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    expected_login_path_from_workspace_path(Path::new(&worktree_path))
                        .to_string_lossy()
                        .to_string()
                });
            let agent_id = extract_string_field(&object, "agent_id")
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    read_workspace_login_snapshot(Path::new(&login_path))
                        .map(|snapshot| snapshot.agent_id)
                })
                .unwrap_or_else(|| agent_id_for_workspace_path(Path::new(&worktree_path)));
            let branch = extract_string_field(&object, "branch")
                .or_else(|| {
                    read_workspace_login_snapshot(Path::new(&login_path))
                        .map(|snapshot| snapshot.branch)
                })
                .unwrap_or_default();
            let base_revision = extract_string_field(&object, "base_revision")
                .or_else(|| {
                    read_workspace_login_snapshot(Path::new(&login_path))
                        .map(|snapshot| snapshot.base_revision)
                })
                .unwrap_or_default();
            let workspace_revision = extract_string_field(&object, "workspace_revision")
                .or_else(|| {
                    read_workspace_login_snapshot(Path::new(&login_path))
                        .map(|snapshot| snapshot.workspace_revision)
                })
                .unwrap_or_default();
            let repository_revision = extract_string_field(&object, "repository_revision")
                .or_else(|| {
                    read_workspace_login_snapshot(Path::new(&login_path))
                        .map(|snapshot| snapshot.repository_revision)
                })
                .unwrap_or_default();
            ConnectedAgent {
                internal_id: extract_number_field(&object, "internal_id").unwrap_or(-1),
                agent_id,
                branch,
                base_revision,
                workspace_revision,
                repository_revision,
                worktree_path,
                login_path,
                assigned_mission: extract_string_field(&object, "assigned_mission")
                    .unwrap_or_default(),
                last_ping: extract_number_field(&object, "last_ping").unwrap_or_default(),
            }
        })
        .collect())
}

fn write_connected_agents(path: &Path, agents: &[ConnectedAgent]) -> io::Result<()> {
    let mut out = String::new();
    out.push_str("{\n  \"agents\": [\n");
    for (idx, agent) in agents.iter().enumerate() {
        let comma = if idx + 1 < agents.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{\n      \"internal_id\": {},\n      \"agent_id\": \"{}\",\n      \"branch\": \"{}\",\n      \"base_revision\": \"{}\",\n      \"workspace_revision\": \"{}\",\n      \"repository_revision\": \"{}\",\n      \"worktree_path\": \"{}\",\n      \"login_path\": \"{}\",\n      \"assigned_mission\": \"{}\",\n      \"last_ping\": {}\n    }}{}\n",
            agent.internal_id,
            json_escape(&agent.agent_id),
            json_escape(&agent.branch),
            json_escape(&agent.base_revision),
            json_escape(&agent.workspace_revision),
            json_escape(&agent.repository_revision),
            json_escape(&agent.worktree_path),
            json_escape(&agent.login_path),
            json_escape(&agent.assigned_mission),
            agent.last_ping,
            comma
        ));
    }
    out.push_str("  ]\n}\n");
    fs::write(path, out)
}

fn read_governance_tasks(path: &Path) -> io::Result<Vec<GovernanceTask>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, "tasks");
    Ok(objects
        .into_iter()
        .map(|object| GovernanceTask {
            file_path: extract_string_field(&object, "file_path").unwrap_or_default(),
            violation_count: extract_number_field(&object, "violation_count").unwrap_or_default(),
            owner_id: extract_number_field(&object, "owner_id").unwrap_or(-1),
        })
        .collect())
}

fn write_governance_tasks(path: &Path, tasks: &[GovernanceTask]) -> io::Result<()> {
    let mut out = String::new();
    out.push_str("{\n  \"tasks\": [\n");
    for (idx, task) in tasks.iter().enumerate() {
        let comma = if idx + 1 < tasks.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{\n      \"file_path\": \"{}\",\n      \"violation_count\": {},\n      \"owner_id\": {}\n    }}{}\n",
            json_escape(&task.file_path),
            task.violation_count,
            task.owner_id,
            comma
        ));
    }
    out.push_str("  ]\n}\n");
    fs::write(path, out)
}

fn read_manager_tasks(path: &Path) -> io::Result<Vec<ManagerTask>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, "tasks");
    Ok(objects
        .into_iter()
        .map(|object| ManagerTask {
            id: extract_string_field(&object, "id").unwrap_or_default(),
            title: extract_string_field(&object, "title").unwrap_or_default(),
            body: extract_string_field(&object, "body").unwrap_or_default(),
            file_path: extract_string_field(&object, "file_path").unwrap_or_default(),
            priority: extract_number_field(&object, "priority").unwrap_or_default(),
            owner_id: extract_number_field(&object, "owner_id").unwrap_or(-1),
            status: extract_string_field(&object, "status").unwrap_or_else(|| "open".to_string()),
            created_at: extract_number_field(&object, "created_at").unwrap_or_default(),
        })
        .collect())
}

fn write_manager_tasks(path: &Path, tasks: &[ManagerTask]) -> io::Result<()> {
    let mut out = String::new();
    out.push_str("{\n  \"tasks\": [\n");
    for (idx, task) in tasks.iter().enumerate() {
        let comma = if idx + 1 < tasks.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{\n      \"id\": \"{}\",\n      \"title\": \"{}\",\n      \"body\": \"{}\",\n      \"file_path\": \"{}\",\n      \"priority\": {},\n      \"owner_id\": {},\n      \"status\": \"{}\",\n      \"created_at\": {}\n    }}{}\n",
            json_escape(&task.id),
            json_escape(&task.title),
            json_escape(&task.body),
            json_escape(&task.file_path),
            task.priority,
            task.owner_id,
            json_escape(&task.status),
            task.created_at,
            comma
        ));
    }
    out.push_str("  ]\n}\n");
    fs::write(path, out)
}

fn read_submit_records(path: &Path) -> io::Result<Vec<SubmitRecord>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, "entries");
    Ok(objects
        .into_iter()
        .map(|object| SubmitRecord {
            id: extract_string_field(&object, "id").unwrap_or_default(),
            internal_id: extract_number_field(&object, "internal_id").unwrap_or(-1),
            agent_id: extract_string_field(&object, "agent_id").unwrap_or_default(),
            title: extract_string_field(&object, "title").unwrap_or_default(),
            topic: extract_string_field(&object, "topic").unwrap_or_default(),
            branch: extract_string_field(&object, "branch").unwrap_or_default(),
            base_revision: extract_string_field(&object, "base_revision").unwrap_or_default(),
            workspace_revision: extract_string_field(&object, "workspace_revision")
                .unwrap_or_default(),
            repository_revision: extract_string_field(&object, "repository_revision")
                .unwrap_or_default(),
            status: extract_string_field(&object, "status").unwrap_or_else(|| "open".to_string()),
            created_at: extract_number_field(&object, "created_at").unwrap_or_default(),
            updated_at: extract_number_field(&object, "updated_at").unwrap_or_default(),
        })
        .collect())
}

fn write_submit_records(path: &Path, entries: &[SubmitRecord]) -> io::Result<()> {
    let mut out = String::new();
    out.push_str("{\n  \"entries\": [\n");
    for (idx, entry) in entries.iter().enumerate() {
        let comma = if idx + 1 < entries.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{\n      \"id\": \"{}\",\n      \"internal_id\": {},\n      \"agent_id\": \"{}\",\n      \"title\": \"{}\",\n      \"topic\": \"{}\",\n      \"branch\": \"{}\",\n      \"base_revision\": \"{}\",\n      \"workspace_revision\": \"{}\",\n      \"repository_revision\": \"{}\",\n      \"status\": \"{}\",\n      \"created_at\": {},\n      \"updated_at\": {}\n    }}{}\n",
            json_escape(&entry.id),
            entry.internal_id,
            json_escape(&entry.agent_id),
            json_escape(&entry.title),
            json_escape(&entry.topic),
            json_escape(&entry.branch),
            json_escape(&entry.base_revision),
            json_escape(&entry.workspace_revision),
            json_escape(&entry.repository_revision),
            json_escape(&entry.status),
            entry.created_at,
            entry.updated_at,
            comma
        ));
    }
    out.push_str("  ]\n}\n");
    fs::write(path, out)
}

fn assign_governance_task(tasks: &mut [GovernanceTask], owner_id: i64) -> Option<String> {
    let mut best_idx = None;
    let mut best_count = i64::MIN;
    for (idx, task) in tasks.iter().enumerate() {
        if task.owner_id >= 0 {
            continue;
        }
        if task.violation_count > best_count {
            best_count = task.violation_count;
            best_idx = Some(idx);
        }
    }

    let idx = best_idx?;
    tasks[idx].owner_id = owner_id;
    Some(tasks[idx].file_path.clone())
}

fn claim_manager_task_for_agent(tasks: &mut [ManagerTask], owner_id: i64) -> Option<ManagerTask> {
    if let Some(existing) = tasks
        .iter()
        .find(|task| task.owner_id == owner_id && is_manager_task_active(task))
    {
        return Some(existing.clone());
    }

    let mut best_idx = None;
    let mut best_priority = i64::MIN;
    let mut best_created_at = i64::MIN;
    for (idx, task) in tasks.iter().enumerate() {
        if task.owner_id >= 0 || !is_manager_task_active(task) {
            continue;
        }
        if task.priority > best_priority
            || (task.priority == best_priority && task.created_at > best_created_at)
        {
            best_priority = task.priority;
            best_created_at = task.created_at;
            best_idx = Some(idx);
        }
    }

    let idx = best_idx?;
    tasks[idx].owner_id = owner_id;
    if tasks[idx].status.trim().is_empty() || tasks[idx].status == "open" {
        tasks[idx].status = "claimed".to_string();
    }
    Some(tasks[idx].clone())
}

fn manager_task_mission(task: &ManagerTask) -> String {
    if task.file_path.trim().is_empty() {
        format!("manager:{}", task.title)
    } else {
        format!("manager:{} [{}]", task.title, task.file_path)
    }
}

fn is_manager_task_active(task: &ManagerTask) -> bool {
    !matches!(task.status.trim(), "done" | "closed" | "resolved")
}

fn normalize_task_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn normalize_agent_topology(
    plugin: &PluginContext,
    agents: &mut Vec<ConnectedAgent>,
    tasks: &mut [GovernanceTask],
    manager_tasks: &mut [ManagerTask],
    now: i64,
) {
    let repository_path = normalize_path(plugin.repository_path.as_path());
    let mut latest_by_path: BTreeMap<String, ConnectedAgent> = BTreeMap::new();
    let mut old_id_to_path: BTreeMap<i64, String> = BTreeMap::new();

    for agent in agents.iter() {
        let path = normalize_path_str(&agent.worktree_path);
        if !is_valid_registered_workspace_path(&path) {
            continue;
        }
        old_id_to_path.insert(agent.internal_id, path.clone());
        match latest_by_path.get(&path) {
            Some(existing) if existing.last_ping >= agent.last_ping => {}
            _ => {
                latest_by_path.insert(path, agent.clone());
            }
        }
    }

    let root_agent = latest_by_path
        .remove(&repository_path)
        .map(|mut agent| {
            let persisted_sync = sync_metadata_from_snapshot(read_workspace_login_snapshot(
                expected_login_path_from_workspace_path(plugin.repository_path.as_path()).as_path(),
            ));
            agent.internal_id = 0;
            agent.agent_id = resolve_persisted_agent_id(
                &agent.agent_id,
                plugin.repository_path.as_path(),
                expected_login_path_from_workspace_path(plugin.repository_path.as_path()).as_path(),
                agents,
            );
            apply_sync_metadata(&mut agent, &persisted_sync);
            agent.worktree_path = plugin.repository_path.to_string_lossy().to_string();
            agent.login_path =
                expected_login_path_from_workspace_path(plugin.repository_path.as_path())
                    .to_string_lossy()
                    .to_string();
            agent.assigned_mission = "global".to_string();
            agent.last_ping = agent.last_ping.max(now);
            agent
        })
        .unwrap_or_else(|| {
            let persisted_sync = sync_metadata_from_snapshot(read_workspace_login_snapshot(
                expected_login_path_from_workspace_path(plugin.repository_path.as_path()).as_path(),
            ));
            ConnectedAgent {
                internal_id: 0,
                agent_id: resolve_persisted_agent_id(
                    "",
                    plugin.repository_path.as_path(),
                    expected_login_path_from_workspace_path(plugin.repository_path.as_path())
                        .as_path(),
                    agents,
                ),
                branch: persisted_sync.branch,
                base_revision: persisted_sync.base_revision,
                workspace_revision: persisted_sync.workspace_revision,
                repository_revision: persisted_sync.repository_revision,
                worktree_path: plugin.repository_path.to_string_lossy().to_string(),
                login_path: expected_login_path_from_workspace_path(
                    plugin.repository_path.as_path(),
                )
                .to_string_lossy()
                .to_string(),
                assigned_mission: "global".to_string(),
                last_ping: now,
            }
        });

    let mut others: Vec<ConnectedAgent> = latest_by_path.into_values().collect();
    others.sort_by(|left, right| {
        left.internal_id
            .cmp(&right.internal_id)
            .then_with(|| left.worktree_path.cmp(&right.worktree_path))
    });

    let mut normalized_agents = vec![root_agent];
    let mut path_to_new_id = BTreeMap::new();
    path_to_new_id.insert(repository_path.clone(), 0i64);

    for (offset, mut agent) in others.into_iter().enumerate() {
        let new_id = (offset as i64) + 1;
        let path = normalize_path_str(&agent.worktree_path);
        let persisted_sync = sync_metadata_from_snapshot(read_workspace_login_snapshot(
            expected_login_path_from_workspace_path(Path::new(&agent.worktree_path)).as_path(),
        ));
        agent.internal_id = new_id;
        agent.agent_id = resolve_persisted_agent_id(
            &agent.agent_id,
            Path::new(&agent.worktree_path),
            expected_login_path_from_workspace_path(Path::new(&agent.worktree_path)).as_path(),
            &normalized_agents,
        );
        apply_sync_metadata(&mut agent, &persisted_sync);
        agent.login_path = expected_login_path_from_workspace_path(Path::new(&agent.worktree_path))
            .to_string_lossy()
            .to_string();
        path_to_new_id.insert(path, new_id);
        normalized_agents.push(agent);
    }

    let mut old_to_new = BTreeMap::new();
    for (old_id, path) in old_id_to_path {
        let new_id = if path == repository_path {
            0
        } else {
            *path_to_new_id.get(&path).unwrap_or(&0)
        };
        old_to_new.insert(old_id, new_id);
    }

    for task in tasks.iter_mut() {
        if task.owner_id >= 0 {
            task.owner_id = *old_to_new.get(&task.owner_id).unwrap_or(&task.owner_id);
        }
    }
    for task in manager_tasks.iter_mut() {
        if task.owner_id >= 0 {
            task.owner_id = *old_to_new.get(&task.owner_id).unwrap_or(&task.owner_id);
        }
    }

    *agents = normalized_agents;
}

fn is_valid_registered_workspace_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }

    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return false;
    }

    fs::metadata(candidate)
        .map(|meta| meta.is_dir())
        .unwrap_or(false)
}

fn expected_login_path_from_workspace_path(workspace_path: &Path) -> PathBuf {
    workspace_path
        .join("AC_Ant-Delirium_props")
        .join("login")
        .join("login.json")
}

fn ensure_agent_ids(agents: &mut [ConnectedAgent]) {
    for agent in agents {
        if agent.agent_id.trim().is_empty() {
            agent.agent_id = resolve_persisted_agent_id(
                "",
                Path::new(&agent.worktree_path),
                Path::new(&agent.login_path),
                &[],
            );
        }
    }
}

fn ensure_agent_branches(agents: &mut [ConnectedAgent]) {
    for agent in agents {
        if agent.branch.trim().is_empty() {
            agent.branch = read_workspace_login_snapshot(Path::new(&agent.login_path))
                .map(|snapshot| snapshot.branch)
                .unwrap_or_default();
        }
    }
}

fn ensure_agent_revisions(plugin: &PluginContext, agents: &mut [ConnectedAgent]) {
    let root_login_path = expected_login_path_from_workspace_path(plugin.repository_path.as_path());
    let root_repository_revision = read_workspace_login_snapshot(root_login_path.as_path())
        .map(|snapshot| snapshot.repository_revision)
        .unwrap_or_default();
    for agent in agents {
        if agent.base_revision.trim().is_empty() {
            agent.base_revision = read_workspace_login_snapshot(Path::new(&agent.login_path))
                .map(|snapshot| snapshot.base_revision)
                .unwrap_or_else(|| root_repository_revision.clone());
        }
        if agent.workspace_revision.trim().is_empty() {
            agent.workspace_revision = read_workspace_login_snapshot(Path::new(&agent.login_path))
                .map(|snapshot| snapshot.workspace_revision)
                .unwrap_or_default();
        }
        if agent.repository_revision.trim().is_empty() {
            agent.repository_revision = read_workspace_login_snapshot(Path::new(&agent.login_path))
                .map(|snapshot| snapshot.repository_revision)
                .unwrap_or_else(|| root_repository_revision.clone());
        }
    }
}

fn agent_id_for_workspace_path(workspace_path: &Path) -> String {
    let normalized = normalize_path(workspace_path);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    to_base36_upper(hasher.finish(), 8)
}

fn to_base36_upper(mut value: u64, width: usize) -> String {
    const ALPHABET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut chars = vec!['0'; width];
    for idx in (0..width).rev() {
        let digit = (value % 36) as usize;
        chars[idx] = ALPHABET[digit] as char;
        value /= 36;
    }
    chars.into_iter().collect()
}

fn sync_metadata_from_snapshot(snapshot: Option<WorkspaceLoginSnapshot>) -> SyncMetadata {
    if let Some(snapshot) = snapshot {
        return SyncMetadata {
            branch: snapshot.branch,
            base_revision: snapshot.base_revision,
            workspace_revision: snapshot.workspace_revision,
            repository_revision: snapshot.repository_revision,
        };
    }
    SyncMetadata {
        branch: String::new(),
        base_revision: String::new(),
        workspace_revision: String::new(),
        repository_revision: String::new(),
    }
}

fn sync_metadata_from_agent(agent: &ConnectedAgent) -> SyncMetadata {
    SyncMetadata {
        branch: agent.branch.clone(),
        base_revision: agent.base_revision.clone(),
        workspace_revision: agent.workspace_revision.clone(),
        repository_revision: agent.repository_revision.clone(),
    }
}

fn sync_metadata_from_cli(
    branch: Option<&str>,
    base_revision: Option<&str>,
    workspace_revision: Option<&str>,
    repository_revision: Option<&str>,
) -> SyncMetadata {
    SyncMetadata {
        branch: branch.unwrap_or("").trim().to_string(),
        base_revision: base_revision.unwrap_or("").trim().to_string(),
        workspace_revision: workspace_revision.unwrap_or("").trim().to_string(),
        repository_revision: repository_revision.unwrap_or("").trim().to_string(),
    }
}

fn sync_metadata_is_empty(metadata: &SyncMetadata) -> bool {
    metadata.branch.trim().is_empty()
        && metadata.base_revision.trim().is_empty()
        && metadata.workspace_revision.trim().is_empty()
        && metadata.repository_revision.trim().is_empty()
}

fn apply_sync_metadata(agent: &mut ConnectedAgent, metadata: &SyncMetadata) {
    if !metadata.branch.trim().is_empty() {
        agent.branch = metadata.branch.clone();
    }
    if !metadata.base_revision.trim().is_empty() {
        agent.base_revision = metadata.base_revision.clone();
    }
    if !metadata.workspace_revision.trim().is_empty() {
        agent.workspace_revision = metadata.workspace_revision.clone();
    }
    if !metadata.repository_revision.trim().is_empty() {
        agent.repository_revision = metadata.repository_revision.clone();
    }
}

fn normalize_operation_kind(raw_kind: Option<&str>) -> String {
    raw_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("general")
        .to_ascii_lowercase()
}

fn missing_sync_metadata_fields(metadata: &SyncMetadata) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if metadata.branch.trim().is_empty() {
        missing.push("--branch");
    }
    if metadata.base_revision.trim().is_empty() {
        missing.push("--base-revision");
    }
    if metadata.workspace_revision.trim().is_empty() {
        missing.push("--workspace-revision");
    }
    if metadata.repository_revision.trim().is_empty() {
        missing.push("--repository-revision");
    }
    missing
}

fn validate_operation_metadata(
    command_name: &str,
    operation_kind: &str,
    metadata: &SyncMetadata,
) -> io::Result<()> {
    if operation_kind != "sync" && operation_kind != "submit" {
        return Ok(());
    }
    let missing = missing_sync_metadata_fields(metadata);
    if missing.is_empty() {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "{} metadata missing for {} operation: provide {}",
            command_name,
            operation_kind,
            missing.join(", ")
        ),
    ))
}

fn append_sync_metadata_fields(
    fields: &mut Vec<(String, JsonValue)>,
    operation_kind: &str,
    metadata: &SyncMetadata,
) {
    fields.push((
        "operation_kind".to_string(),
        JsonValue::String(operation_kind.to_string()),
    ));
    fields.push((
        "branch".to_string(),
        JsonValue::String(metadata.branch.clone()),
    ));
    fields.push((
        "base_revision".to_string(),
        JsonValue::String(metadata.base_revision.clone()),
    ));
    fields.push((
        "workspace_revision".to_string(),
        JsonValue::String(metadata.workspace_revision.clone()),
    ));
    fields.push((
        "repository_revision".to_string(),
        JsonValue::String(metadata.repository_revision.clone()),
    ));
}

fn persist_current_agent_sync_metadata(
    plugin: &PluginContext,
    agents: &mut [ConnectedAgent],
    current_agent: &ConnectedAgent,
    metadata: &SyncMetadata,
) -> io::Result<()> {
    if sync_metadata_is_empty(metadata) {
        return Ok(());
    }
    if let Some(agent) = agents.iter_mut().find(|agent| {
        agent.internal_id == current_agent.internal_id
            && normalize_path_str(&agent.login_path)
                == normalize_path(plugin.login_path().as_path())
    }) {
        apply_sync_metadata(agent, metadata);
        let snapshot = agent.clone();
        write_connected_agents(&plugin.connected_path(), agents)?;
        write_workspace_login(plugin, &snapshot)?;
    }
    Ok(())
}

fn resolve_agent_sync_state(agents: &[ConnectedAgent], agent: &ConnectedAgent) -> AgentSyncState {
    let repository_revision = if agent.internal_id == 0 {
        agent.repository_revision.clone()
    } else {
        resolve_agent_by_id(agents, 0)
            .map(|root| {
                if root.repository_revision.trim().is_empty() {
                    if agent.repository_revision.trim().is_empty() {
                        root.workspace_revision.clone()
                    } else {
                        agent.repository_revision.clone()
                    }
                } else {
                    root.repository_revision.clone()
                }
            })
            .unwrap_or_else(|| agent.repository_revision.clone())
    };
    let workspace_revision = agent.workspace_revision.clone();
    let base_revision = agent.base_revision.clone();

    let (sync_state, rebase_required) = if agent.internal_id == 0 {
        ("root".to_string(), false)
    } else if repository_revision.trim().is_empty() || workspace_revision.trim().is_empty() {
        ("unknown".to_string(), false)
    } else if repository_revision == workspace_revision {
        ("clean".to_string(), false)
    } else if repository_revision != base_revision {
        ("stale".to_string(), true)
    } else {
        ("diverged".to_string(), true)
    };

    AgentSyncState {
        sync_state,
        repository_revision,
        workspace_revision,
        base_revision,
        rebase_required,
    }
}

fn resolve_agent_submit_state(
    agents: &[ConnectedAgent],
    agent: &ConnectedAgent,
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    queue: &[SubmitRecord],
) -> AgentSubmitState {
    let has_submit_message = messages.iter().any(|entry| {
        extract_entry_string(entry, "sender_agent_id")
            .map(|sender| sender == agent.agent_id)
            .unwrap_or(false)
            && extract_entry_string(entry, "operation_kind")
                .map(|kind| kind == "submit")
                .unwrap_or(false)
    });
    let has_submit_feedback = feedback.iter().any(|entry| {
        extract_entry_string(entry, "sender_agent_id")
            .map(|sender| sender == agent.agent_id)
            .unwrap_or(false)
            && extract_entry_string(entry, "operation_kind")
                .map(|kind| kind == "submit")
                .unwrap_or(false)
    });
    let sync = resolve_agent_sync_state(agents, agent);
    let submit_queue_status = latest_submit_record_for_agent(queue, &agent.agent_id)
        .map(|entry| entry.status.clone())
        .unwrap_or_default();
    let submit_state = if matches!(
        submit_queue_status.as_str(),
        "accepted" | "rejected" | "closed"
    ) {
        "consumed"
    } else if !has_submit_message && !has_submit_feedback {
        "none"
    } else if has_submit_message && !has_submit_feedback {
        "message_only"
    } else if !has_submit_message && has_submit_feedback {
        "feedback_only"
    } else if sync.rebase_required {
        "blocked_rebase"
    } else {
        "ready_for_root"
    };

    AgentSubmitState {
        submit_state: submit_state.to_string(),
        has_submit_message,
        has_submit_feedback,
        submit_queue_status: submit_queue_status.clone(),
        latest_submit_status: submit_queue_status,
    }
}

fn latest_submit_record_for_agent<'a>(
    queue: &'a [SubmitRecord],
    agent_id: &str,
) -> Option<&'a SubmitRecord> {
    queue
        .iter()
        .filter(|entry| entry.agent_id == agent_id)
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.created_at.cmp(&right.created_at))
        })
}

fn latest_active_submit_record_for_agent<'a>(
    queue: &'a [SubmitRecord],
    agent_id: &str,
) -> Option<&'a SubmitRecord> {
    queue
        .iter()
        .filter(|entry| entry.agent_id == agent_id && entry.status == "open")
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.created_at.cmp(&right.created_at))
        })
}

fn resolve_persisted_agent_id(
    current_agent_id: &str,
    workspace_path: &Path,
    login_path: &Path,
    existing_agents: &[ConnectedAgent],
) -> String {
    if !current_agent_id.trim().is_empty() {
        return current_agent_id.trim().to_string();
    }
    if let Some(snapshot) = read_workspace_login_snapshot(login_path) {
        if !snapshot.agent_id.trim().is_empty() {
            return snapshot.agent_id;
        }
    }
    generate_unique_agent_id(existing_agents, workspace_path)
}

fn generate_unique_agent_id(existing_agents: &[ConnectedAgent], workspace_path: &Path) -> String {
    let used: HashSet<String> = existing_agents
        .iter()
        .map(|agent| agent.agent_id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();

    for salt in 0_u64..1024 {
        let mut hasher = DefaultHasher::new();
        normalize_path(workspace_path).hash(&mut hasher);
        unix_timestamp().hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        salt.hash(&mut hasher);
        let candidate = to_base36_upper(hasher.finish(), 8);
        if !used.contains(&candidate) {
            return candidate;
        }
    }

    agent_id_for_workspace_path(workspace_path)
}

fn generate_manager_task_id(existing_tasks: &[ManagerTask], title: &str) -> String {
    let used: HashSet<String> = existing_tasks
        .iter()
        .map(|task| task.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();

    for salt in 0_u64..1024 {
        let mut hasher = DefaultHasher::new();
        title.trim().hash(&mut hasher);
        unix_timestamp().hash(&mut hasher);
        salt.hash(&mut hasher);
        let candidate = to_base36_upper(hasher.finish(), 8);
        if !used.contains(&candidate) {
            return candidate;
        }
    }

    to_base36_upper(unix_timestamp() as u64, 8)
}

fn generate_submit_id(existing_entries: &[SubmitRecord], title: &str) -> String {
    let used: HashSet<String> = existing_entries
        .iter()
        .map(|entry| entry.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();

    for salt in 0_u64..1024 {
        let mut hasher = DefaultHasher::new();
        title.trim().hash(&mut hasher);
        unix_timestamp().hash(&mut hasher);
        salt.hash(&mut hasher);
        let candidate = to_base36_upper(hasher.finish(), 8);
        if !used.contains(&candidate) {
            return candidate;
        }
    }

    to_base36_upper(unix_timestamp() as u64, 8)
}

fn append_submit_record(
    plugin: &PluginContext,
    body: &str,
    topic: Option<&str>,
    branch: Option<&str>,
    base_revision: Option<&str>,
    workspace_revision: Option<&str>,
    repository_revision: Option<&str>,
) -> io::Result<()> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let (agents, _tasks, _manager_tasks, _messages) = load_runtime_state(plugin)?;
        let current_agent = resolve_current_agent(plugin, &agents).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "submit requires the current workspace to be a reconciled agent",
            )
        })?;
        let mut entries = read_submit_records(&plugin.submit_queue_path())?;
        let now = unix_timestamp();
        for entry in entries
            .iter_mut()
            .filter(|entry| entry.agent_id == current_agent.agent_id && entry.status == "open")
        {
            entry.status = "superseded".to_string();
            entry.updated_at = now;
        }
        entries.push(SubmitRecord {
            id: generate_submit_id(&entries, body),
            internal_id: current_agent.internal_id,
            agent_id: current_agent.agent_id.clone(),
            title: body.trim().to_string(),
            topic: topic.unwrap_or("submit").trim().to_string(),
            branch: branch.unwrap_or("").trim().to_string(),
            base_revision: base_revision.unwrap_or("").trim().to_string(),
            workspace_revision: workspace_revision.unwrap_or("").trim().to_string(),
            repository_revision: repository_revision.unwrap_or("").trim().to_string(),
            status: "open".to_string(),
            created_at: now,
            updated_at: now,
        });
        write_submit_records(&plugin.submit_queue_path(), &entries)
    })
}

fn write_workspace_login(plugin: &PluginContext, agent: &ConnectedAgent) -> io::Result<()> {
    fs::create_dir_all(plugin.login_dir())?;
    let output = format!(
        "{{\n  \"internal_id\": {},\n  \"agent_id\": \"{}\",\n  \"branch\": \"{}\",\n  \"base_revision\": \"{}\",\n  \"workspace_revision\": \"{}\",\n  \"repository_revision\": \"{}\",\n  \"repository_path\": \"{}\",\n  \"workspace_path\": \"{}\",\n  \"login_path\": \"{}\",\n  \"plugin_path\": \"{}\",\n  \"execution_context\": \"{}\",\n  \"assigned_mission\": \"{}\",\n  \"last_ping\": {}\n}}\n",
        agent.internal_id,
        json_escape(&agent.agent_id),
        json_escape(&agent.branch),
        json_escape(&agent.base_revision),
        json_escape(&agent.workspace_revision),
        json_escape(&agent.repository_revision),
        json_escape(&plugin.repository_path.to_string_lossy()),
        json_escape(&plugin.workspace_path.to_string_lossy()),
        json_escape(&plugin.login_path().to_string_lossy()),
        json_escape(&plugin.plugin_dir.to_string_lossy()),
        if plugin.is_worktree {
            "git worktree"
        } else {
            "repository root"
        },
        json_escape(&agent.assigned_mission),
        agent.last_ping
    );
    fs::write(plugin.login_path(), output)
}

fn agent_execution_context(plugin: &PluginContext, agent: &ConnectedAgent) -> &'static str {
    if agent.internal_id == 0
        || normalize_path_str(&agent.worktree_path)
            == normalize_path(plugin.repository_path.as_path())
    {
        "repository root"
    } else {
        "git worktree"
    }
}

fn plugin_execution_context(plugin: &PluginContext) -> &'static str {
    if is_repository_root_context(plugin) {
        "repository root"
    } else {
        "git worktree"
    }
}

fn is_repository_root_context(plugin: &PluginContext) -> bool {
    normalize_path(plugin.workspace_path.as_path())
        == normalize_path(plugin.repository_path.as_path())
}

fn is_root_agent_context(plugin: &PluginContext, agent: &ConnectedAgent) -> bool {
    agent.internal_id == 0
        && normalize_path_str(&agent.worktree_path)
            == normalize_path(plugin.repository_path.as_path())
}

fn agent_governance_scope<'a>(
    _plugin: &PluginContext,
    agent: &ConnectedAgent,
    tasks: &'a [GovernanceTask],
) -> Vec<&'a GovernanceTask> {
    if agent.internal_id == 0 {
        tasks.iter().collect()
    } else {
        tasks
            .iter()
            .filter(|task| task.owner_id == agent.internal_id)
            .collect()
    }
}

fn resolve_current_agent<'a>(
    plugin: &PluginContext,
    agents: &'a [ConnectedAgent],
) -> Option<&'a ConnectedAgent> {
    let workspace = normalize_path(plugin.workspace_path.as_path());
    let login_path = normalize_path(plugin.login_path().as_path());
    agents.iter().find(|agent| {
        normalize_path_str(&agent.worktree_path) == workspace
            && normalize_path_str(&agent.login_path) == login_path
    })
}

fn resolve_agent_by_id(agents: &[ConnectedAgent], agent_id: i64) -> Option<&ConnectedAgent> {
    agents.iter().find(|agent| agent.internal_id == agent_id)
}

fn resolve_agent_by_agent_id<'a>(
    agents: &'a [ConnectedAgent],
    agent_id: &str,
) -> Option<&'a ConnectedAgent> {
    agents.iter().find(|agent| agent.agent_id == agent_id)
}

fn count_pending_messages_for_agent(agent_id: i64, messages: &[JsonEntry]) -> usize {
    messages
        .iter()
        .filter(|entry| {
            extract_entry_number(entry, "target_internal_id")
                .map(|target| target == agent_id || target < 0)
                .unwrap_or(false)
        })
        .count()
}

fn read_message_entries(path: &Path, key: &str) -> io::Result<Vec<JsonEntry>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, key);
    Ok(objects
        .into_iter()
        .map(|object| {
            let mut fields = Vec::new();
            if let Some(value) = extract_number_field(&object, "target_internal_id") {
                fields.push(("target_internal_id".to_string(), JsonValue::Number(value)));
            }
            if let Some(value) = extract_number_field(&object, "sender_internal_id") {
                fields.push(("sender_internal_id".to_string(), JsonValue::Number(value)));
            }
            if let Some(value) = extract_string_field(&object, "target_agent_id") {
                fields.push(("target_agent_id".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "sender_agent_id") {
                fields.push(("sender_agent_id".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "body") {
                fields.push(("body".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "workspace_path") {
                fields.push(("workspace_path".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "visibility") {
                fields.push(("visibility".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "login_path") {
                fields.push(("login_path".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "repository_path") {
                fields.push(("repository_path".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "branch") {
                fields.push(("branch".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "base_revision") {
                fields.push(("base_revision".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "workspace_revision") {
                fields.push(("workspace_revision".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "repository_revision") {
                fields.push(("repository_revision".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "operation_kind") {
                fields.push(("operation_kind".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "topic") {
                fields.push(("topic".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_number_field(&object, "internal_id") {
                fields.push(("internal_id".to_string(), JsonValue::Number(value)));
            }
            if let Some(value) = extract_string_field(&object, "agent_id") {
                fields.push(("agent_id".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "execution_context") {
                fields.push(("execution_context".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "assigned_mission") {
                fields.push(("assigned_mission".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "taxonomy_path") {
                fields.push(("taxonomy_path".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "api_taxonomy_command") {
                fields.push(("api_taxonomy_command".to_string(), JsonValue::String(value)));
            }
            if let Some(value) = extract_string_field(&object, "feedback_help_command") {
                fields.push((
                    "feedback_help_command".to_string(),
                    JsonValue::String(value),
                ));
            }
            if let Some(value) = extract_number_field(&object, "created_at") {
                fields.push(("created_at".to_string(), JsonValue::Number(value)));
            }
            JsonEntry { fields }
        })
        .collect())
}

fn extract_entry_string<'a>(entry: &'a JsonEntry, field: &str) -> Option<&'a str> {
    entry.fields.iter().find_map(|(name, value)| match value {
        JsonValue::String(value) if name == field => Some(value.as_str()),
        _ => None,
    })
}

fn extract_entry_number(entry: &JsonEntry, field: &str) -> Option<i64> {
    entry.fields.iter().find_map(|(name, value)| match value {
        JsonValue::Number(value) if name == field => Some(*value),
        _ => None,
    })
}

fn write_entry_collection(path: &Path, key: &str, entries: &[JsonEntry]) -> io::Result<()> {
    let mut out = String::new();
    out.push_str(&format!("{{\n  \"{}\": [\n", key));
    for (idx, entry) in entries.iter().enumerate() {
        let comma = if idx + 1 < entries.len() { "," } else { "" };
        out.push_str("    {\n");
        for (field_idx, (name, value)) in entry.fields.iter().enumerate() {
            let field_comma = if field_idx + 1 < entry.fields.len() {
                ","
            } else {
                ""
            };
            match value {
                JsonValue::String(value) => out.push_str(&format!(
                    "      \"{}\": \"{}\"{}\n",
                    json_escape(name),
                    json_escape(value),
                    field_comma
                )),
                JsonValue::Number(value) => out.push_str(&format!(
                    "      \"{}\": {}{}\n",
                    json_escape(name),
                    value,
                    field_comma
                )),
            }
        }
        out.push_str(&format!("    }}{}\n", comma));
    }
    out.push_str("  ]\n}\n");
    fs::write(path, out)
}

fn parse_object_array(raw: &str, key: &str) -> Vec<String> {
    let Some(array_content) = extract_named_array(raw, key) else {
        return Vec::new();
    };
    split_top_level_objects(&array_content)
}

fn extract_named_array(raw: &str, key: &str) -> Option<String> {
    let key_pattern = format!("\"{}\"", key);
    let start = raw.find(&key_pattern)?;
    let bracket_idx = raw[start..].find('[')? + start;
    let end = find_matching_delimiter(raw, bracket_idx, '[', ']')?;
    Some(raw[bracket_idx + 1..end].to_string())
}

fn split_top_level_objects(raw: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    let mut in_string = false;
    let mut previous = '\0';

    for (idx, ch) in raw.char_indices() {
        if ch == '"' && previous != '\\' {
            in_string = !in_string;
        }
        if in_string {
            previous = ch;
            continue;
        }
        if ch == '{' {
            if depth == 0 {
                start = Some(idx);
            }
            depth += 1;
        } else if ch == '}' {
            if depth > 0 {
                depth -= 1;
                if depth == 0 {
                    if let Some(object_start) = start.take() {
                        objects.push(raw[object_start..=idx].to_string());
                    }
                }
            }
        }
        previous = ch;
    }

    objects
}

fn extract_string_field(raw: &str, field: &str) -> Option<String> {
    let field_pattern = format!("\"{}\"", field);
    let start = raw.find(&field_pattern)?;
    let colon = raw[start..].find(':')? + start;
    let quote_start = raw[colon..].find('"')? + colon + 1;
    let mut escaped = false;
    for (offset, ch) in raw[quote_start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return Some(unescape_json_string(
                &raw[quote_start..quote_start + offset],
            ));
        }
    }
    None
}

fn extract_number_field(raw: &str, field: &str) -> Option<i64> {
    let field_pattern = format!("\"{}\"", field);
    let start = raw.find(&field_pattern)?;
    let colon = raw[start..].find(':')? + start;
    let tail = raw[colon + 1..].trim_start();
    let len = tail
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .count();
    if len == 0 {
        return None;
    }
    tail[..len].parse().ok()
}

fn find_matching_delimiter(raw: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut previous = '\0';
    for (idx, ch) in raw.char_indices().skip_while(|(idx, _)| *idx < start) {
        if ch == '"' && previous != '\\' {
            in_string = !in_string;
        }
        if in_string {
            previous = ch;
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
        previous = ch;
    }
    None
}

fn read_text_or_empty(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err),
    }
}

fn resolve_cli_path(project_root: &Path, raw_path: &str) -> PathBuf {
    let candidate = Path::new(raw_path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    }
}

fn load_hierarchy_from_path(path: &Path) -> HierarchyConfig {
    let is_dir = fs::metadata(path)
        .map(|meta| meta.is_dir())
        .unwrap_or(false);
    if is_dir {
        let selected = select_runtime_profile_folder(path);
        HierarchyConfig::load_from_folder(&selected.to_string_lossy())
    } else {
        let input = fs::read_to_string(path).unwrap_or_default();
        HierarchyConfig::parse_brute(&input)
    }
}

fn select_runtime_profile_folder(path: &Path) -> PathBuf {
    let type_1 = path.join("type_1");
    if type_1.is_dir() {
        type_1
    } else {
        path.to_path_buf()
    }
}

fn hierarchy_hint_path(project_root: &Path, source: &Path) -> String {
    let hint = if source.is_dir() {
        let selected = select_runtime_profile_folder(source);
        let default_hierarchy = selected.join("default_hierarchy.toml");
        if default_hierarchy.is_file() {
            default_hierarchy
        } else {
            selected
        }
    } else {
        source.to_path_buf()
    };

    hint.strip_prefix(project_root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| hint.to_string_lossy().replace('\\', "/"))
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn with_runtime_lock<T, F>(plugin: &PluginContext, operation: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T>,
{
    let _guard = RuntimeLockGuard::acquire(&plugin.plugin_dir)?;
    operation()
}

struct RuntimeLockGuard {
    path: PathBuf,
}

impl RuntimeLockGuard {
    fn acquire(plugin_dir: &Path) -> io::Result<Self> {
        let lock_path = plugin_dir.join(".ac_ant_delirium_runtime.lock");
        let mut attempts = 0usize;

        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(_) => return Ok(Self { path: lock_path }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists && attempts < 400 => {
                    attempts += 1;
                    thread::sleep(Duration::from_millis(25));
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "timed out waiting for runtime lock at {}",
                            lock_path.to_string_lossy()
                        ),
                    ));
                }
                Err(err) => return Err(err),
            }
        }
    }
}

impl Drop for RuntimeLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if unique
            .iter()
            .any(|seen: &PathBuf| normalize_path(seen) == normalize_path(&path))
        {
            continue;
        }
        unique.push(path);
    }
    unique
}

fn has_semantic_root(hierarchy: &HierarchyConfig, root: &str) -> bool {
    [
        "structural",
        "identity",
        "pedagogical",
        "sanitization",
        "dependency_governance",
        "files",
    ]
    .iter()
    .any(|branch| {
        hierarchy
            .nodes
            .contains_key(&format!("{}.{}", root, branch))
    })
}

fn normalize_path(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn normalize_path_str(path: &str) -> String {
    normalize_path(Path::new(path))
}

fn json_escape(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn unescape_json_string(input: &str) -> String {
    input
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

fn empty_or_dash(value: &str) -> &str {
    if value.trim().is_empty() {
        "-"
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ac_ant_delirium::engine::Swarm_Engine_unit::SwarmEngine;
    use ac_ant_delirium::structs::{
        AuditConfig, AuditResult, AuditViolation, GovernanceMode, RuntimeConfig,
    };

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
    fn assigns_highest_unowned_governance_task() {
        let mut tasks = vec![
            GovernanceTask {
                file_path: "a.rs".to_string(),
                violation_count: 10,
                owner_id: -1,
            },
            GovernanceTask {
                file_path: "b.rs".to_string(),
                violation_count: 50,
                owner_id: -1,
            },
        ];

        let assigned = assign_governance_task(&mut tasks, 7);
        assert_eq!(assigned.as_deref(), Some("b.rs"));
        assert_eq!(tasks[1].owner_id, 7);
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
            },
        ];

        let claimed = claim_manager_task_for_agent(&mut tasks, 2).expect("task should be claimed");
        assert_eq!(claimed.id, "b");
        assert_eq!(tasks[1].owner_id, 2);
        assert_eq!(tasks[1].status, "claimed");
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("second submit should persist");

        let queue = read_submit_records(&plugin.submit_queue_path()).expect("queue should load");
        assert_eq!(queue.len(), 2);
        assert_eq!(
            queue.iter().filter(|entry| entry.status == "open").count(),
            1
        );
        assert_eq!(
            queue.iter()
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("submit should persist");

        let queue =
            read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
        let submit_id = queue[0].id.clone();
        update_submit_status(&plugin_root, &submit_id, "accepted").expect("root should accept");

        let accepted_status =
            render_agents_report(&plugin_root, true).expect("accepted status should render");
        assert!(accepted_status.contains("\"consumed_submits\": 1"));
        assert!(accepted_status.contains("\"accepted_submit_records\": 1"));
        assert!(accepted_status.contains("\"submit_state\": \"consumed\""));
        assert!(accepted_status.contains("\"submit_queue_status\": \"accepted\""));

        update_submit_status(&plugin_root, &submit_id, "closed").expect("root should close");

        let queue =
            read_submit_records(&plugin_root.submit_queue_path()).expect("queue should reload");
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("submit b should persist");

        let queue =
            read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
        let submit_a = queue[0].id.clone();
        let submit_b = queue[1].id.clone();

        update_submit_status(&plugin_root, &submit_a, "accepted").expect("root should accept");
        update_submit_status(&plugin_root, &submit_b, "closed").expect("root should close");

        let report = render_submit_queue(&plugin_root, false, None, None)
            .expect("queue report should render");
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("submit b should persist");

        let queue =
            read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
        let submit_a = queue[0].id.clone();
        let submit_b = queue[1].id.clone();
        update_submit_status(&plugin_root, &submit_a, "accepted").expect("root should accept");

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

        let tasks =
            read_governance_tasks(&plugin.governance_path()).expect("governance should reload");
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

        append_submit(
            &plugin,
            "range.rs fix is ready for review",
            Some("submit.range"),
            Some("hierarchy.submit"),
            Some("ant-agent-1"),
            Some("BASE1"),
            Some("WORK2"),
            Some("ROOT3"),
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("submit should store both artifacts");

        let raw_messages =
            fs::read_to_string(plugin.messages_path()).expect("messages should exist");
        let raw_feedback =
            fs::read_to_string(plugin.feedback_path()).expect("feedback should exist");
        assert!(raw_messages.contains("\"operation_kind\": \"submit\""));
        assert!(raw_messages.contains("\"target_agent_id\": \"broadcast\""));
        assert!(raw_messages.contains(&format!("\"sender_agent_id\": \"{}\"", agent.agent_id)));
        assert!(raw_feedback.contains("\"operation_kind\": \"submit\""));
        assert!(raw_feedback.contains("\"taxonomy_path\": \"hierarchy.submit\""));
        assert!(raw_feedback.contains(&format!("\"sender_agent_id\": \"{}\"", agent.agent_id)));

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
        let raw_login =
            fs::read_to_string(plugin.login_path()).expect("root login should be readable");
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("submit should persist");

        let status = render_agents_report(&plugin_root, false).expect("status should render");
        assert!(status.contains("Ready submits: 1"));
        assert!(status.contains("Open submit records: 1"));
        assert!(status.contains("submit_state=ready_for_root"));
        assert!(status.contains("submit_queue_status=open"));

        let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
        assert!(whoami.contains("Submit state: ready_for_root"));
        assert!(whoami.contains("Submit queue status: open"));
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("stale submit should still persist");

        let status_json =
            render_agents_report(&plugin_root, true).expect("json status should render");
        assert!(status_json.contains("\"blocked_submits\": 1"));
        assert!(status_json.contains("\"open_submit_records\": 1"));
        assert!(status_json.contains("\"submit_state\": \"blocked_rebase\""));
        assert!(status_json.contains("\"submit_queue_status\": \"open\""));

        let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("stale submit should persist");

        let queue =
            read_submit_records(&plugin_root.submit_queue_path()).expect("queue should load");
        let submit_id = queue[0].id.clone();
        update_submit_status(&plugin_root, &submit_id, "accepted").expect("root should accept");

        let report = render_root_orchestration_violations(&plugin_root, false)
            .expect("root report should render");
        assert!(
            report.is_none(),
            "accepted submit should be treated as consumed, not as an active rebase conflict"
        );

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

        let whoami = render_current_agent_identity(&plugin_a, false).expect("whoami should render");
        assert!(whoami.contains("Agent 1"));
        assert!(whoami.contains(&agent_a.agent_id));
        assert!(whoami.contains(plugin_a.login_path().to_string_lossy().as_ref()));

        let by_id = render_agent_identity(&plugin_root, agent_a.internal_id, true)
            .expect("agent should render");
        assert!(by_id.contains("\"internal_id\": 1"));
        assert!(by_id.contains(&format!("\"agent_id\": \"{}\"", agent_a.agent_id)));
        assert!(by_id.contains("\"login_path\":"));

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

        let thread = render_messages_feed(&plugin_root, Some("range.rs"), false)
            .expect("thread should render");
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("feedback should be stored");

        let raw = fs::read_to_string(plugin.feedback_path()).expect("feedback file should exist");
        assert!(raw.contains("\"internal_id\": 0"));
        assert!(raw.contains("\"agent_id\": \""));
        assert!(raw.contains("\"execution_context\": \"repository root\""));
        assert!(raw.contains("\"taxonomy_path\": \"hierarchy.files.max_nesting\""));
        assert!(raw.contains("\"api_taxonomy_command\": \"./plugins/AC_Rayon-Genetics/ac_ant_delirium --api hierarchy.files.max_nesting\""));
        assert!(raw.contains("\"feedback_help_command\": \"./plugins/AC_Rayon-Genetics/ac_ant_delirium --help.feedback\""));
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
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
        let rendered = render_plugin_help(None, "./plugins/AC_Rayon-Genetics/ac_ant_delirium");
        assert!(rendered.contains("--installation"));
        assert!(!rendered.contains("--instalation"));
        assert!(!rendered.contains("--installtion"));
        assert!(rendered.contains("config init"));
        assert!(rendered.contains("config dump"));
        assert!(rendered.contains("swarm status"));
        assert!(rendered.contains("--help.feedback"));
    }

    #[test]
    fn config_init_materializes_default_tomls_and_build_guard_for_new_project() {
        let root = unique_temp_dir("ac_ant_delirium_config_init");
        let project_root = root.join("AC_Rayon-Genetics");
        fs::create_dir_all(&project_root).expect("project root should exist");

        let fake_bin = root.join("delirium_cli");
        fs::write(&fake_bin, "fake-binary").expect("fake binary should exist");

        let plugin_dir = root.join("plugins").join("AC_Rayon-Genetics");
        fs::create_dir_all(&plugin_dir).expect("plugin dir should exist");

        let copied_binary = copy_binary_if_needed(&fake_bin, &plugin_dir.join("ac_ant_delirium"))
            .expect("binary copy should succeed");
        assert!(copied_binary);

        let hierarchy_written = write_default_template_if_missing(
            &plugin_dir.join("ant_delirium_config/default_hierarchy.toml"),
            &render_new_project_hierarchy(&project_root),
        )
        .expect("hierarchy should be written");
        let rules_written = write_default_template_if_missing(
            &plugin_dir.join("ant_delirium_config/default_rules.toml"),
            DEFAULT_NEW_PROJECT_RULES,
        )
        .expect("rules should be written");
        let guard_written = write_default_template_if_missing(
            &project_root.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"),
            DEFAULT_NEW_PROJECT_BUILD_GUARD,
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
        let guard = fs::read_to_string(
            project_root.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"),
        )
        .expect("guard should be readable");
        assert!(guard.contains("pub(crate) fn run"));
        assert!(guard.contains("ant_delirium"));

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

        let report = materialize_project_local_plugin_package(
            &plugin.repository_path,
            None,
            Some(&plugin),
            "./plugins/AC_Rayon-Genetics/ac_ant_delirium",
        )
        .expect("materialization should succeed");

        let guard_path = plugin
            .repository_path
            .join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs");
        assert!(guard_path.is_file());
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

        let fake_exe = plugin_dir.join("ac_ant_delirium");
        let (effective_root, effective_plugin_dir) =
            infer_bootstrap_context(&plugin_dir, None, &fake_exe);

        assert_eq!(effective_root, project_root);
        assert_eq!(effective_plugin_dir, plugin_dir);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn copy_binary_if_needed_skips_when_paths_point_to_same_existing_file() {
        let root = unique_temp_dir("ac_ant_delirium_copy_binary_same_file");
        let bin_path = root
            .join("plugins")
            .join("AC_Rayon-Genetics")
            .join("ac_ant_delirium");
        if let Some(parent) = bin_path.parent() {
            fs::create_dir_all(parent).expect("parent should exist");
        }
        fs::write(&bin_path, "fake-binary").expect("binary should exist");

        let canonical = fs::canonicalize(&bin_path).expect("canonical path should resolve");
        let copied =
            copy_binary_if_needed(&canonical, &bin_path).expect("copy check should succeed");

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

        let mut missions = Vec::new();
        for plugin in &plugins {
            let mission = assign_task_to_current_agent_if_missing(plugin)
                .expect("task assignment should succeed")
                .expect("each Rayon worktree should receive a mission while tasks remain");
            missions.push((plugin.workspace_path.clone(), mission));
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
            let isolated = isolate_audit_to_current_agent(plugin, &audit_result)
                .expect("audit should isolate");
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
}
