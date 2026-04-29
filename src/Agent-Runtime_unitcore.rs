use super::*;

pub(super) const DEFAULT_NEW_PROJECT_HIERARCHY: &str =
    include_str!("../get_toml_for_new/default_hierarchy.toml");
pub(super) const DEFAULT_NEW_PROJECT_RULES: &str =
    include_str!("../get_toml_for_new/default_rules.toml");
pub(super) const DEFAULT_NEW_PROJECT_BUILD_GUARD: &str =
    include_str!("../get_toml_for_new/ant_delirium_build_guard.rs");
pub(super) const DEFAULT_GITHOOKS_README: &str = r#"Managed Ant-Delirium Git hooks.

These hooks are generated artifacts under AC_Ant-Delirium_props/githooks.
Do not treat them as hand-edited policy files.

Activation is external to the motor.
ant_delirium does not run git or shell commands for you.
Wire this directory into your hook system manually.

Current generated policies:
- pre-commit: requires local login.json so the workspace is reconciled first
- commit-msg: requires the current stable agent_id and an explicit submit_id in the commit message
- pre-merge-commit: requires explicit merge_request.json metadata plus local login.json
- pre-push: blocks pushes to refs/heads/main and requires explicit push_request.json metadata
"#;
pub(super) const DEFAULT_MERGE_REQUEST_TEMPLATE: &str = r#"{
  "agent_id": "HGMEO5LM",
  "workspace_path": "/abs/path/to/worktree",
  "branch": "ant-agent-1",
  "base_revision": "BASE1234",
  "workspace_revision": "WORK5678",
  "repository_revision": "ROOT9999",
  "submit_id": "SUBMIT123",
  "approved_by_agent_id": "83BAV05E",
  "approval_note": "root authorized local merge after reviewing submit evidence"
}
"#;
pub(super) const DEFAULT_PUSH_REQUEST_TEMPLATE: &str = r#"{
  "agent_id": "HGMEO5LM",
  "workspace_path": "/abs/path/to/worktree",
  "branch": "ant-agent-1",
  "base_revision": "BASE1234",
  "workspace_revision": "WORK5678",
  "repository_revision": "ROOT9999",
  "submit_id": "SUBMIT123",
  "push_target": "origin/ant-agent-1",
  "approved_by_agent_id": "83BAV05E",
  "approval_note": "root authorized external push after reviewing sync metadata"
}
"#;
pub(super) const SWARM_STATUS_BACKLOG_PREVIEW_LIMIT: usize = 12;

pub(super) fn discover_runtime_context(
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

pub(super) fn sync_agent_registration(
    plugin: &PluginContext,
) -> io::Result<Option<ConnectedAgent>> {
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
                assign_governance_task(&mut tasks, &[], next_id).unwrap_or_default()
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

pub(super) fn render_agents_report(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, tasks, manager_tasks, messages) = load_runtime_state(plugin)?;
        let feedback = load_feedback_entries(plugin)?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let governance_history = read_governance_history(&plugin.governance_history_path())?;

        if json_requested {
            return Ok(render_agents_report_json(
                plugin,
                &agents,
                &tasks,
                &manager_tasks,
                &messages,
                &feedback,
                &submit_queue,
                &governance_history,
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
            "Active agents: {}\nProjects: {}\nOpen governance tasks: {}\nOpen manager tasks: {}\nGovernance history events: {}\nReady submits: {}\nPending submits: {}\nBlocked submits: {}\nBlocked task-version submits: {}\nConsumed submits: {}\nOpen submit records: {}\nAccepted submit records: {}\nRejected submit records: {}\nClosed submit records: {}\nSuperseded submit records: {}\nStale submit records: {}\n\n",
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
            governance_history.len(),
            agents
                .iter()
                .filter(|agent| {
                    matches!(
                        resolve_agent_submit_state(
                            &agents,
                            agent,
                            &tasks,
                            &messages,
                            &feedback,
                            &submit_queue,
                            &governance_history
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
                            &tasks,
                            &messages,
                            &feedback,
                            &submit_queue,
                            &governance_history
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
                            &tasks,
                            &messages,
                            &feedback,
                            &submit_queue,
                            &governance_history
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
                            &tasks,
                            &messages,
                            &feedback,
                            &submit_queue,
                            &governance_history
                        )
                        .submit_state
                        .as_str(),
                        "blocked_task_version"
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
                            &tasks,
                            &messages,
                            &feedback,
                            &submit_queue,
                            &governance_history
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
            ,
            submit_queue
                .iter()
                .filter(|record| record.status == "stale")
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
            let active_task = if is_root_agent_context(plugin, agent) {
                None
            } else {
                owned.first().copied()
            };
            let owned_manager: Vec<&ManagerTask> = manager_tasks
                .iter()
                .filter(|task| task.owner_id == agent.internal_id && is_manager_task_active(task))
                .collect();
            let manager_override = !owned_manager.is_empty();
            let pending_messages = count_pending_messages_for_agent(agent.internal_id, &messages);
            let violations: i64 = owned.iter().map(|task| task.violation_count).sum();
            let sync = resolve_agent_sync_state(&agents, &agent);
            let submit = resolve_agent_submit_state(
                &agents,
                &agent,
                &tasks,
                &messages,
                &feedback,
                &submit_queue,
                &governance_history,
            );
            let (operational_feedback, pedagogical_feedback) =
                count_feedback_for_agent(&agent.agent_id, &feedback);
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
                "  submit_state={} submit_queue_status={} submit_task={} submit_task_version={} task_stale={} stale_reason={} decided_by={} decided_at={} submit_message={} submit_feedback={}\n",
                submit.submit_state,
                empty_or_dash(&submit.submit_queue_status),
                empty_or_dash(&submit.task_file_path),
                submit.task_version,
                if submit.task_stale { "yes" } else { "no" },
                empty_or_dash(&submit.stale_reason),
                empty_or_dash(&submit.decision_by_agent_id),
                submit.decided_at,
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
                "  active_task={} active_task_version={} active_task_id={}\n",
                empty_or_dash(active_task.map(|task| task.file_path.as_str()).unwrap_or("")),
                active_task.map(|task| task.task_version).unwrap_or_default(),
                empty_or_dash(active_task.map(|task| task.task_id.as_str()).unwrap_or("")),
            ));
            out.push_str(&format!(
                "  manager_override={} active_manager_tasks={} queued_violation_files={} queued_violations={}\n",
                if manager_override { "yes" } else { "no" },
                owned_manager.len(),
                owned.len(),
                violations
            ));
            out.push_str(&format!(
                "  operational_feedback={} pedagogical_feedback={}\n",
                operational_feedback, pedagogical_feedback
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
            let preview_len = owned.len().min(SWARM_STATUS_BACKLOG_PREVIEW_LIMIT);
            for task in owned.iter().take(preview_len) {
                out.push_str(&format!(
                    "    file={} violations={}\n",
                    task.file_path, task.violation_count
                ));
            }
            if owned.len() > SWARM_STATUS_BACKLOG_PREVIEW_LIMIT {
                let hidden = &owned[SWARM_STATUS_BACKLOG_PREVIEW_LIMIT..];
                let hidden_violations: i64 = hidden.iter().map(|task| task.violation_count).sum();
                out.push_str(&format!(
                    "    ... {} more file(s) hidden, {} more violation(s)\n",
                    hidden.len(),
                    hidden_violations
                ));
            }
        }

        Ok(out)
    })
}

pub(super) fn render_projects_report(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
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

pub(super) fn render_next_action(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, tasks, manager_tasks, messages) = load_runtime_state(plugin)?;
        let feedback = load_feedback_entries(plugin)?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let governance_history = read_governance_history(&plugin.governance_history_path())?;
        let current_agent = resolve_current_agent(plugin, &agents).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "current workspace is not a reconciled agent; login first",
            )
        })?;
        let sync = resolve_agent_sync_state(&agents, current_agent);
        let submit = resolve_agent_submit_state(
            &agents,
            current_agent,
            &tasks,
            &messages,
            &feedback,
            &submit_queue,
            &governance_history,
        );
        let owned = agent_governance_scope(plugin, current_agent, &tasks);
        let active_task = if is_root_agent_context(plugin, current_agent) {
            None
        } else {
            owned.first().copied()
        };
        let active_manager_tasks: Vec<&ManagerTask> = manager_tasks
            .iter()
            .filter(|task| {
                task.owner_id == current_agent.internal_id && is_manager_task_active(task)
            })
            .collect();

        let (action_code, action_text) = if is_root_agent_context(plugin, current_agent) {
            let active_submit_count = submit_queue
                .iter()
                .filter(|entry| entry.status == "open")
                .count();
            let blocked_task_submit_count = agents
                .iter()
                .filter(|agent| {
                    matches!(
                        resolve_agent_submit_state(
                            &agents,
                            agent,
                            &tasks,
                            &messages,
                            &feedback,
                            &submit_queue,
                            &governance_history
                        )
                        .submit_state
                        .as_str(),
                        "blocked_task_version"
                    )
                })
                .count();
            let findings = collect_root_orchestration_findings(plugin)?;
            let stale_task_only = !findings.is_empty()
                && findings
                    .iter()
                    .all(|finding| finding.code == "submit_task_stale");
            if stale_task_only {
                (
                    "review_stale_submit_tasks",
                    format!(
                        "root sees {} stale submit(s); inspect --submits or --submit-show, decide whether to close them, and wait for agents to refresh against the latest governance task",
                        blocked_task_submit_count.max(findings.len())
                    ),
                )
            } else if !findings.is_empty() {
                (
                    "review_root_violations",
                    format!(
                        "root has {} orchestration violation(s); run swarm status and resolve root findings before continuing",
                        findings.len()
                    ),
                )
            } else if blocked_task_submit_count > 0 {
                (
                    "review_stale_submit_tasks",
                    format!(
                        "root sees {} stale submit(s); inspect --submits or --submit-show, decide whether to close them, and wait for agents to refresh against the latest governance task",
                        blocked_task_submit_count
                    ),
                )
            } else if active_submit_count > 0 {
                (
                    "triage_active_submits",
                    format!(
                        "root should inspect {} active submit(s); use --submits-active and decide accept/reject/close",
                        active_submit_count
                    ),
                )
            } else if manager_tasks
                .iter()
                .any(|task| task.owner_id < 0 && is_manager_task_active(task))
            {
                (
                    "dispatch_manager_priority",
                    "root should review unassigned manager priority tasks and let the next agent claim them".to_string(),
                )
            } else if tasks.iter().any(|task| task.owner_id < 0) {
                (
                    "await_agent_claims",
                    "root has unassigned governance work; wait for agents to login or claim the backlog".to_string(),
                )
            } else {
                (
                    "root_idle",
                    "root is synchronized and has no active submit or backlog triage to process"
                        .to_string(),
                )
            }
        } else if !active_manager_tasks.is_empty() {
            (
                "complete_manager_task",
                format!(
                    "finish the active manager priority task `{}` before returning to governance backlog",
                    active_manager_tasks[0].title
                ),
            )
        } else if submit.submit_queue_status == "open" && submit.submit_state == "ready_for_root" {
            (
                "wait_root_triage",
                "submit is already open and waiting for root review; do not resend it yet, and monitor --inbox or --messages for root feedback".to_string(),
            )
        } else if submit.submit_state == "blocked_task_version" {
            (
                "refresh_governance_task",
                "submit is stale against the current governance task version; re-run the local audit for the assigned file, confirm the new task metadata, and only then submit again".to_string(),
            )
        } else if submit.submit_state == "blocked_rebase" {
            (
                "refresh_sync_metadata",
                "submit is blocked by rebase metadata; gather fresh branch/base/workspace/repository revisions and retry sync or submit".to_string(),
            )
        } else if submit.submit_queue_status == "rejected" {
            (
                "revise_rejected_submit",
                format!(
                    "root rejected the latest submit; revise `{}` and submit updated evidence",
                    if !current_agent.assigned_mission.trim().is_empty() {
                        current_agent.assigned_mission.as_str()
                    } else if !owned.is_empty() {
                        owned[0].file_path.as_str()
                    } else {
                        "the assigned work"
                    }
                ),
            )
        } else if !current_agent.assigned_mission.trim().is_empty() {
            (
                "continue_assigned_mission",
                format!(
                    "continue working on the assigned mission `{}` until local violations reach zero or a manager task overrides it",
                    current_agent.assigned_mission
                ),
            )
        } else if !owned.is_empty() {
            (
                "resume_owned_governance",
                "resume the currently owned governance backlog and reduce local violations before requesting submit".to_string(),
            )
        } else if matches!(submit.submit_queue_status.as_str(), "accepted" | "closed") {
            (
                "resume_backlog",
                "latest submit was already consumed by root; return to backlog or wait for a new assignment".to_string(),
            )
        } else {
            (
                "claim_next_work",
                "no mission is assigned right now; wait for the next backlog claim or manager override".to_string(),
            )
        };

        if json_requested {
            return Ok(format!(
                "{{\n  \"command\": \"next_action\",\n  \"agent_id\": \"{}\",\n  \"internal_id\": {},\n  \"root_scope\": {},\n  \"sync_state\": \"{}\",\n  \"submit_state\": \"{}\",\n  \"submit_queue_status\": \"{}\",\n  \"task_id\": \"{}\",\n  \"task_version\": {},\n  \"task_revision\": \"{}\",\n  \"task_file_path\": \"{}\",\n  \"active_task_id\": \"{}\",\n  \"active_task_version\": {},\n  \"active_task_revision\": \"{}\",\n  \"active_task_content_hash\": \"{}\",\n  \"active_task_file_path\": \"{}\",\n  \"task_stale\": {},\n  \"stale_reason\": \"{}\",\n  \"decision_by_agent_id\": \"{}\",\n  \"decision_note\": \"{}\",\n  \"action_code\": \"{}\",\n  \"action_text\": \"{}\"\n}}\n",
                json_escape(&current_agent.agent_id),
                current_agent.internal_id,
                if is_root_agent_context(plugin, current_agent) { "true" } else { "false" },
                json_escape(&sync.sync_state),
                json_escape(&submit.submit_state),
                json_escape(&submit.submit_queue_status),
                json_escape(&submit.task_id),
                submit.task_version,
                json_escape(&submit.task_revision),
                json_escape(&submit.task_file_path),
                json_escape(active_task.map(|task| task.task_id.as_str()).unwrap_or("")),
                active_task.map(|task| task.task_version).unwrap_or_default(),
                json_escape(
                    active_task
                        .map(|task| task.task_revision.as_str())
                        .unwrap_or("")
                ),
                json_escape(
                    active_task
                        .map(|task| task.task_content_hash.as_str())
                        .unwrap_or("")
                ),
                json_escape(active_task.map(|task| task.file_path.as_str()).unwrap_or("")),
                if submit.task_stale { "true" } else { "false" },
                json_escape(&submit.stale_reason),
                json_escape(&submit.decision_by_agent_id),
                json_escape(&submit.decision_note),
                json_escape(action_code),
                json_escape(&action_text)
            ));
        }

        Ok(format!(
            "Next Action\n\nAgent: {} (# {})\nAction: {}\n\n{}\n",
            current_agent.agent_id, current_agent.internal_id, action_code, action_text
        ))
    })
}

pub(super) fn render_manager_tasks(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
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

pub(super) fn render_submit_queue(
    plugin: &PluginContext,
    json_requested: bool,
    filter: Option<&str>,
    submit_id: Option<&str>,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let entries = read_submit_records(&plugin.submit_queue_path())?;
        let tasks = read_governance_tasks(&plugin.governance_path())?;
        let history = read_governance_history(&plugin.governance_history_path())?;
        let view = filter.unwrap_or("all");
        let filtered: Vec<SubmitRecord> =
            if let Some(id) = submit_id.map(str::trim).filter(|value| !value.is_empty()) {
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
                let task_stale = if submit_record_task_stale(entry, &tasks) {
                    "true"
                } else {
                    "false"
                };
                let stale_reason = submit_record_stale_reason(entry, &tasks, &history);
                out.push_str(&format!(
                    "    {{ \"id\": \"{}\", \"internal_id\": {}, \"agent_id\": \"{}\", \"task_id\": \"{}\", \"task_version\": {}, \"task_revision\": \"{}\", \"task_content_hash\": \"{}\", \"task_file_path\": \"{}\", \"task_stale\": {}, \"stale_reason\": \"{}\", \"title\": \"{}\", \"topic\": \"{}\", \"branch\": \"{}\", \"base_revision\": \"{}\", \"workspace_revision\": \"{}\", \"repository_revision\": \"{}\", \"status\": \"{}\", \"decision_by_internal_id\": {}, \"decision_by_agent_id\": \"{}\", \"decision_note\": \"{}\", \"decided_at\": {}, \"created_at\": {}, \"updated_at\": {} }}{}\n",
                    json_escape(&entry.id),
                    entry.internal_id,
                    json_escape(&entry.agent_id),
                    json_escape(&entry.task_id),
                    entry.task_version,
                    json_escape(&entry.task_revision),
                    json_escape(&entry.task_content_hash),
                    json_escape(&entry.task_file_path),
                    task_stale,
                    json_escape(&stale_reason),
                    json_escape(&entry.title),
                    json_escape(&entry.topic),
                    json_escape(&entry.branch),
                    json_escape(&entry.base_revision),
                    json_escape(&entry.workspace_revision),
                    json_escape(&entry.repository_revision),
                    json_escape(&entry.status),
                    entry.decision_by_internal_id,
                    json_escape(&entry.decision_by_agent_id),
                    json_escape(&entry.decision_note),
                    entry.decided_at,
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
            "View: {}\nSubmit ID: {}\nCount: {}\nOpen: {}\nAccepted: {}\nRejected: {}\nClosed: {}\nSuperseded: {}\nStale queue records: {}\nStale task submits: {}\n\n",
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
            entries.iter().filter(|entry| entry.status == "stale").count(),
            entries
                .iter()
                .filter(|entry| submit_record_task_stale(entry, &tasks))
                .count(),
        ));
        if filtered.is_empty() {
            out.push_str("- none\n");
            return Ok(out);
        }
        for entry in filtered {
            let task_stale = if submit_record_task_stale(&entry, &tasks) {
                "yes"
            } else {
                "no"
            };
            let stale_reason = submit_record_stale_reason(&entry, &tasks, &history);
            out.push_str(&format!(
                "- id={} agent={} (# {}) task={} v{} hash={} task_stale={} stale_reason={} status={} decided_by={} decided_at={} topic={} branch={} title={} note={}\n",
                entry.id,
                entry.agent_id,
                entry.internal_id,
                empty_or_dash(&entry.task_file_path),
                entry.task_version,
                empty_or_dash(&entry.task_content_hash),
                task_stale,
                empty_or_dash(&stale_reason),
                entry.status,
                empty_or_dash(&entry.decision_by_agent_id),
                entry.decided_at,
                empty_or_dash(&entry.topic),
                empty_or_dash(&entry.branch),
                entry.title,
                empty_or_dash(&entry.decision_note)
            ));
        }
        Ok(out)
    })
}

pub(super) fn render_governance_history(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let entries = read_governance_history(&plugin.governance_history_path())?;

        if json_requested {
            let mut out = String::new();
            out.push_str("{\n");
            out.push_str("  \"command\": \"governance_history\",\n");
            out.push_str(&format!("  \"count\": {},\n", entries.len()));
            out.push_str("  \"entries\": [\n");
            for (idx, entry) in entries.iter().enumerate() {
                let comma = if idx + 1 < entries.len() { "," } else { "" };
                out.push_str(&format!(
                    "    {{ \"id\": \"{}\", \"task_id\": \"{}\", \"file_path\": \"{}\", \"change_kind\": \"{}\", \"previous_version\": {}, \"current_version\": {}, \"previous_revision\": \"{}\", \"current_revision\": \"{}\", \"previous_content_hash\": \"{}\", \"current_content_hash\": \"{}\", \"previous_owner_id\": {}, \"current_owner_id\": {}, \"created_at\": {}, \"previous_event_hash\": \"{}\", \"event_hash\": \"{}\" }}{}\n",
                    json_escape(&entry.id),
                    json_escape(&entry.task_id),
                    json_escape(&entry.file_path),
                    json_escape(&entry.change_kind),
                    entry.previous_version,
                    entry.current_version,
                    json_escape(&entry.previous_revision),
                    json_escape(&entry.current_revision),
                    json_escape(&entry.previous_content_hash),
                    json_escape(&entry.current_content_hash),
                    entry.previous_owner_id,
                    entry.current_owner_id,
                    entry.created_at,
                    json_escape(&entry.previous_event_hash),
                    json_escape(&entry.event_hash),
                    comma
                ));
            }
            out.push_str("  ]\n}\n");
            return Ok(out);
        }

        let mut out = String::new();
        out.push_str("Governance History\n\n");
        out.push_str(&format!("Count: {}\n\n", entries.len()));
        if entries.is_empty() {
            out.push_str("- none\n");
            return Ok(out);
        }
        for entry in entries {
            out.push_str(&format!(
                "- id={} task={} file={} kind={} v{}->v{} prev_owner={} owner={} prev_hash={} hash={} at={}\n",
                entry.id,
                empty_or_dash(&entry.task_id),
                empty_or_dash(&entry.file_path),
                empty_or_dash(&entry.change_kind),
                entry.previous_version,
                entry.current_version,
                entry.previous_owner_id,
                entry.current_owner_id,
                empty_or_dash(&entry.previous_event_hash),
                empty_or_dash(&entry.event_hash),
                entry.created_at
            ));
        }
        Ok(out)
    })
}

pub(super) fn update_submit_status(
    plugin: &PluginContext,
    submit_id: &str,
    status: &str,
    note: Option<&str>,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        if !is_repository_root_context(plugin) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "submit status can only be updated from repository root",
            ));
        }
        let (mut agents, mut tasks, _manager_tasks, _messages) = load_runtime_state(plugin)?;
        let current_agent = resolve_current_agent(plugin, &agents)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "submit status update requires the repository root to be a reconciled agent",
                )
            })?
            .clone();
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
        let now = unix_timestamp();
        entry.status = status.to_string();
        entry.decision_by_internal_id = current_agent.internal_id;
        entry.decision_by_agent_id = current_agent.agent_id.clone();
        entry.decision_note = note.map(str::trim).unwrap_or("").to_string();
        entry.decided_at = now;
        entry.updated_at = now;

        if matches!(status, "accepted" | "closed") {
            release_submit_task_claim(plugin, &mut agents, &mut tasks, entry)?;
        }

        write_submit_records(&plugin.submit_queue_path(), &entries)?;
        Ok(format!(
            "submit {} marked {} by {}",
            submit_id.trim(),
            status,
            current_agent.agent_id
        ))
    })
}

pub(super) fn append_manager_task(
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

pub(super) fn close_manager_task(plugin: &PluginContext, task_id: &str) -> io::Result<String> {
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

pub(super) fn render_rebase_check(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
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

pub(super) fn render_root_orchestration_violations(
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

pub(super) fn render_current_agent_identity(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, tasks, manager_tasks, messages) = load_runtime_state(plugin)?;
        let feedback = load_feedback_entries(plugin)?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let governance_history = read_governance_history(&plugin.governance_history_path())?;
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
            &governance_history,
            json_requested,
        ))
    })
}

pub(super) fn render_agent_identity(
    plugin: &PluginContext,
    agent_id: i64,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, tasks, manager_tasks, messages) = load_runtime_state(plugin)?;
        let feedback = load_feedback_entries(plugin)?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let governance_history = read_governance_history(&plugin.governance_history_path())?;
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
            &governance_history,
            json_requested,
        ))
    })
}

pub(super) fn render_agent_identity_by_agent_id(
    plugin: &PluginContext,
    agent_uid: &str,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        let (agents, tasks, manager_tasks, messages) = load_runtime_state(plugin)?;
        let feedback = load_feedback_entries(plugin)?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let governance_history = read_governance_history(&plugin.governance_history_path())?;
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
            &governance_history,
            json_requested,
        ))
    })
}

pub(super) fn release_stale_agent_claims(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let mut agents = read_connected_agents(&plugin.connected_path())?;
        let mut tasks = read_governance_tasks(&plugin.governance_path())?;
        let mut manager_tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let now = unix_timestamp();

        normalize_agent_topology(plugin, &mut agents, &mut tasks, &mut manager_tasks, now);
        reconcile_agents_with_login_paths(
            plugin,
            &mut agents,
            &mut tasks,
            &mut manager_tasks,
            now,
        )?;

        let current_agent = resolve_current_agent(plugin, &agents).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "current workspace is not a reconciled agent; login first",
            )
        })?;
        if !is_root_agent_context(plugin, current_agent) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "--release-stale-agent-claims is root-only; run it from the repository root",
            ));
        }

        let mut released_agents = Vec::new();
        let mut skipped_agents = Vec::new();
        let mut released_governance_tasks = 0usize;
        let mut released_manager_tasks = 0usize;

        for idx in 0..agents.len() {
            if agents[idx].internal_id == 0 || now - agents[idx].last_ping <= 3600 {
                continue;
            }

            let owns_governance = tasks
                .iter()
                .any(|task| task.owner_id == agents[idx].internal_id);
            let owns_manager = manager_tasks.iter().any(|task| {
                task.owner_id == agents[idx].internal_id && is_manager_task_active(task)
            });
            let has_mission = !agents[idx].assigned_mission.trim().is_empty();
            if !has_mission && !owns_governance && !owns_manager {
                continue;
            }

            if latest_active_submit_record_for_agent(&submit_queue, &agents[idx].agent_id).is_some()
            {
                skipped_agents.push(agents[idx].agent_id.clone());
                continue;
            }

            let agent_internal_id = agents[idx].internal_id;
            let previous_mission = agents[idx].assigned_mission.clone();
            for task in tasks
                .iter_mut()
                .filter(|task| task.owner_id == agent_internal_id)
            {
                task.owner_id = -1;
                released_governance_tasks += 1;
            }
            for task in manager_tasks
                .iter_mut()
                .filter(|task| task.owner_id == agent_internal_id && is_manager_task_active(task))
            {
                task.owner_id = -1;
                released_manager_tasks += 1;
            }
            agents[idx].assigned_mission.clear();
            write_workspace_login_for_agent(plugin, &agents[idx])?;
            released_agents.push((agents[idx].agent_id.clone(), previous_mission));
        }

        write_governance_tasks(&plugin.governance_path(), &tasks)?;
        write_manager_tasks(&plugin.manager_tasks_path(), &manager_tasks)?;
        write_connected_agents(&plugin.connected_path(), &agents)?;

        if json_requested {
            let mut out = String::new();
            out.push_str("{\n");
            out.push_str("  \"command\": \"release_stale_agent_claims\",\n");
            out.push_str(&format!(
                "  \"released_agents\": {},\n",
                released_agents.len()
            ));
            out.push_str(&format!(
                "  \"released_governance_tasks\": {},\n",
                released_governance_tasks
            ));
            out.push_str(&format!(
                "  \"released_manager_tasks\": {},\n",
                released_manager_tasks
            ));
            out.push_str(&format!(
                "  \"skipped_agents\": {},\n",
                skipped_agents.len()
            ));
            out.push_str("  \"agents\": [\n");
            for (idx, (agent_id, mission)) in released_agents.iter().enumerate() {
                let comma = if idx + 1 < released_agents.len() {
                    ","
                } else {
                    ""
                };
                out.push_str(&format!(
                    "    {{ \"agent_id\": \"{}\", \"previous_mission\": \"{}\" }}{}\n",
                    json_escape(agent_id),
                    json_escape(mission),
                    comma
                ));
            }
            out.push_str("  ],\n");
            out.push_str("  \"skipped\": [\n");
            for (idx, agent_id) in skipped_agents.iter().enumerate() {
                let comma = if idx + 1 < skipped_agents.len() {
                    ","
                } else {
                    ""
                };
                out.push_str(&format!("    \"{}\"{}\n", json_escape(agent_id), comma));
            }
            out.push_str("  ]\n");
            out.push_str("}\n");
            return Ok(out);
        }

        let mut out = String::new();
        out.push_str("Release Stale Agent Claims\n\n");
        out.push_str(&format!("Released agents: {}\n", released_agents.len()));
        out.push_str(&format!(
            "Released governance tasks: {}\n",
            released_governance_tasks
        ));
        out.push_str(&format!(
            "Released manager tasks: {}\n",
            released_manager_tasks
        ));
        out.push_str(&format!(
            "Skipped agents with open submit: {}\n",
            skipped_agents.len()
        ));
        for (agent_id, mission) in released_agents {
            out.push_str(&format!(
                "- released agent={} previous_mission={}\n",
                agent_id,
                empty_or_dash(&mission)
            ));
        }
        for agent_id in skipped_agents {
            out.push_str(&format!(
                "- skipped agent={} reason=open_submit\n",
                agent_id
            ));
        }
        Ok(out)
    })
}

pub(super) fn prune_stale_agents(
    plugin: &PluginContext,
    json_requested: bool,
) -> io::Result<String> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let mut agents = read_connected_agents(&plugin.connected_path())?;
        let mut tasks = read_governance_tasks(&plugin.governance_path())?;
        let mut manager_tasks = read_manager_tasks(&plugin.manager_tasks_path())?;
        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let now = unix_timestamp();

        normalize_agent_topology(plugin, &mut agents, &mut tasks, &mut manager_tasks, now);
        reconcile_agents_with_login_paths(
            plugin,
            &mut agents,
            &mut tasks,
            &mut manager_tasks,
            now,
        )?;

        let current_agent = resolve_current_agent(plugin, &agents).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "current workspace is not a reconciled agent; login first",
            )
        })?;
        if !is_root_agent_context(plugin, current_agent) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "--prune-stale-agents is root-only; run it from the repository root",
            ));
        }

        let active_submit_agents: HashSet<String> = submit_queue
            .iter()
            .filter(|entry| matches!(entry.status.as_str(), "open" | "stale"))
            .map(|entry| entry.agent_id.clone())
            .collect();
        let task_owner_ids: HashSet<i64> = tasks
            .iter()
            .filter(|task| task.owner_id >= 0)
            .map(|task| task.owner_id)
            .collect();
        let manager_owner_ids: HashSet<i64> = manager_tasks
            .iter()
            .filter(|task| task.owner_id >= 0 && is_manager_task_active(task))
            .map(|task| task.owner_id)
            .collect();
        let before_len = agents.len();
        let mut pruned = Vec::new();

        agents.retain(|agent| {
            if agent.internal_id == 0 || now - agent.last_ping <= 3600 {
                return true;
            }
            if !agent.assigned_mission.trim().is_empty() {
                return true;
            }
            if task_owner_ids.contains(&agent.internal_id)
                || manager_owner_ids.contains(&agent.internal_id)
                || active_submit_agents.contains(&agent.agent_id)
            {
                return true;
            }
            pruned.push(agent.agent_id.clone());
            false
        });

        if agents.len() != before_len {
            normalize_agent_topology(plugin, &mut agents, &mut tasks, &mut manager_tasks, now);
            write_connected_agents(&plugin.connected_path(), &agents)?;
            write_governance_tasks(&plugin.governance_path(), &tasks)?;
            write_manager_tasks(&plugin.manager_tasks_path(), &manager_tasks)?;
        }

        if json_requested {
            let mut out = String::new();
            out.push_str("{\n");
            out.push_str("  \"command\": \"prune_stale_agents\",\n");
            out.push_str(&format!("  \"pruned_agents\": {},\n", pruned.len()));
            out.push_str("  \"agents\": [\n");
            for (idx, agent_id) in pruned.iter().enumerate() {
                let comma = if idx + 1 < pruned.len() { "," } else { "" };
                out.push_str(&format!("    \"{}\"{}\n", json_escape(agent_id), comma));
            }
            out.push_str("  ]\n");
            out.push_str("}\n");
            return Ok(out);
        }

        let mut out = String::new();
        out.push_str("Prune Stale Agents\n\n");
        out.push_str(&format!("Pruned agents: {}\n", pruned.len()));
        for agent_id in pruned {
            out.push_str(&format!("- pruned agent={}\n", agent_id));
        }
        Ok(out)
    })
}

pub(super) fn refresh_governance_from_audit(
    plugin: &PluginContext,
    audit_result: &AuditResult,
) -> io::Result<Vec<GovernanceTask>> {
    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let mut agents = read_connected_agents(&plugin.connected_path())?;
        let previous = read_governance_tasks(&plugin.governance_path())?;
        let mut governance_history = read_governance_history(&plugin.governance_history_path())?;
        let previous_by_path: BTreeMap<String, GovernanceTask> = previous
            .into_iter()
            .map(|task| (normalize_task_path(&task.file_path), task))
            .collect();
        let governance_revision = current_governance_revision(plugin);

        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        for violation in &audit_result.violations {
            if is_super_block_violation(&violation.rule_id) {
                continue;
            }
            let key = normalize_task_path(&violation.file_path);
            *counts.entry(key).or_insert(0) += 1;
        }

        let mut invalidated_claim_paths = Vec::new();
        let mut tasks: Vec<GovernanceTask> = counts
            .into_iter()
            .map(|(file_path, violation_count)| {
                let previous = previous_by_path.get(&file_path);
                let content_hash = task_content_hash_for_path(plugin, &file_path);
                let claim_changed = previous
                    .map(|task| {
                        task.violation_count != violation_count
                            || task.task_content_hash != content_hash
                    })
                    .unwrap_or(false);
                if claim_changed {
                    if let Some(task) = previous.filter(|task| task.owner_id >= 0) {
                        invalidated_claim_paths.push(task.file_path.clone());
                    }
                }
                GovernanceTask {
                    task_id: previous
                        .map(|task| task.task_id.clone())
                        .filter(|id| !id.trim().is_empty())
                        .unwrap_or_else(|| governance_task_id_for_path(&file_path)),
                    task_version: previous
                        .map(|task| {
                            if task.violation_count == violation_count
                                && task.task_content_hash == content_hash
                            {
                                task.task_version
                            } else {
                                task.task_version + 1
                            }
                        })
                        .unwrap_or(1),
                    task_revision: governance_revision.clone(),
                    task_content_hash: content_hash,
                    owner_id: if claim_changed {
                        -1
                    } else {
                        previous.map(|task| task.owner_id).unwrap_or(-1)
                    },
                    file_path,
                    violation_count,
                }
            })
            .collect();

        for task in previous_by_path.values() {
            if task.owner_id >= 0 && !tasks.iter().any(|current| current.task_id == task.task_id) {
                invalidated_claim_paths.push(task.file_path.clone());
            }
        }

        tasks.sort_by(|left, right| {
            right
                .violation_count
                .cmp(&left.violation_count)
                .then_with(|| left.file_path.cmp(&right.file_path))
        });
        let history_events = build_governance_history_entries(
            &governance_history,
            &previous_by_path,
            &tasks,
            unix_timestamp(),
        );
        governance_history.extend(history_events);

        let mut submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let submit_queue_changed =
            mark_open_submit_records_stale(&mut submit_queue, &tasks, unix_timestamp());

        let mut touched_agents: Vec<ConnectedAgent> = Vec::new();
        if !invalidated_claim_paths.is_empty() {
            let invalidated: BTreeSet<String> = invalidated_claim_paths
                .into_iter()
                .map(|path| normalize_task_path(&path))
                .collect();
            for agent in agents.iter_mut() {
                if agent.internal_id == 0 {
                    continue;
                }
                let mission = normalize_task_path(&agent.assigned_mission);
                if !mission.is_empty() && invalidated.contains(&mission) {
                    agent.assigned_mission.clear();
                    touched_agents.push(agent.clone());
                }
            }
        }

        write_governance_tasks(&plugin.governance_path(), &tasks)?;
        if !governance_history.is_empty() {
            write_governance_history(&plugin.governance_history_path(), &governance_history)?;
        }
        if submit_queue_changed {
            write_submit_records(&plugin.submit_queue_path(), &submit_queue)?;
        }
        if !touched_agents.is_empty() {
            write_connected_agents(&plugin.connected_path(), &agents)?;
            for agent in &touched_agents {
                write_workspace_login_for_agent(plugin, agent)?;
            }
        }
        Ok(tasks)
    })
}

fn is_super_block_violation(rule_id: &str) -> bool {
    let normalized = rule_id.trim();
    normalized.contains(".super_block.enforced.") || normalized.contains(".super_block.watch.")
}

pub(super) fn assign_task_to_current_agent_if_missing(
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

        let submit_queue = read_submit_records(&plugin.submit_queue_path())?;
        let Some(assigned) = assign_governance_task(&mut tasks, &submit_queue, agent_id) else {
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

pub(super) fn isolate_audit_to_current_agent(
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

pub(super) fn append_message(
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
        let (mut agents, tasks, _manager_tasks, mut entries) = load_runtime_state(plugin)?;
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
        let active_task = if operation_kind == "submit" {
            validate_submit_task_freshness(plugin, "message", &sender_agent, &tasks)?
        } else {
            governance_task_for_agent(&sender_agent, &tasks)
                .map(|task| hydrate_governance_task_metadata(plugin, task))
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
        append_task_metadata_fields(&mut fields, &operation_kind, active_task.as_ref());
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

pub(super) fn append_feedback(
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
        let (mut agents, tasks, _manager_tasks, _messages) = load_runtime_state(plugin)?;
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
        let active_task = if operation_kind == "submit" {
            validate_submit_task_freshness(plugin, "feedback", &current_agent, &tasks)?
        } else {
            governance_task_for_agent(&current_agent, &tasks)
                .map(|task| hydrate_governance_task_metadata(plugin, task))
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
        append_task_metadata_fields(&mut fields, &operation_kind, active_task.as_ref());
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

pub(super) fn append_submit(
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
    let submit_metadata = sync_metadata_from_cli(
        branch,
        base_revision,
        workspace_revision,
        repository_revision,
    );
    validate_operation_metadata("submit", "submit", &submit_metadata)?;

    with_runtime_lock(plugin, || {
        ensure_support_files(plugin)?;
        let (agents, tasks, _manager_tasks, _messages) = load_runtime_state(plugin)?;
        let current_agent = resolve_current_agent(plugin, &agents).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "submit requires the current workspace to be a reconciled agent",
            )
        })?;
        validate_submit_task_freshness(plugin, "submit", current_agent, &tasks)?;
        Ok(())
    })?;

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

pub(super) fn render_current_inbox(plugin: &PluginContext) -> io::Result<String> {
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

pub(super) fn render_messages_feed(
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

pub(super) fn render_plugin_help(plugin: Option<&PluginContext>, binary_name: &str) -> String {
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
    out.push_str(&format!("- {} config hooks init\n", binary_name));
    out.push_str(&format!("- {} config hooks verify merge\n", binary_name));
    out.push_str(&format!("- {} config hooks verify push\n", binary_name));
    out.push_str(&format!("- {} config dump\n", binary_name));
    out.push_str(&format!("- {} swarm status\n", binary_name));
    out.push_str(&format!("- {} --getprojects\n", binary_name));
    out.push_str(&format!("- {} --whoami\n", binary_name));
    out.push_str(&format!("- {} --next-action\n", binary_name));
    out.push_str(&format!("- {} --root-checks\n", binary_name));
    out.push_str(&format!("- {} --release-stale-agent-claims\n", binary_name));
    out.push_str(&format!("- {} --prune-stale-agents\n", binary_name));
    out.push_str(&format!("- {} --rebase-check\n", binary_name));
    out.push_str(&format!("- {} --show-agent <id>\n", binary_name));
    out.push_str(&format!("- {} --show-agent-id <agent_id>\n", binary_name));
    out.push_str(&format!("- {} --help.feedback\n", binary_name));
    out.push_str(&format!("- {} --manager-tasks\n", binary_name));
    out.push_str(&format!("- {} --submits\n", binary_name));
    out.push_str(&format!("- {} --submits-active\n", binary_name));
    out.push_str(&format!("- {} --submits-history\n", binary_name));
    out.push_str(&format!("- {} --submit-show <submit_id>\n", binary_name));
    out.push_str(&format!("- {} --governance-history\n", binary_name));
    out.push_str(&format!(
        "- {} --submit-accept <submit_id> --submit-note \"...\"\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --submit-reject <submit_id> --submit-note \"...\"\n",
        binary_name
    ));
    out.push_str(&format!(
        "- {} --submit-close <submit_id> --submit-note \"...\"\n",
        binary_name
    ));
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
    out.push_str(&format!("- {} config hooks init\n", binary_name));
    out.push_str(&format!("- {} config hooks verify merge\n", binary_name));
    out.push_str(&format!("- {} config hooks verify push\n", binary_name));
    out.push_str(&format!("- {} config dump\n", binary_name));
    out.push_str(&format!("- {} swarm status\n", binary_name));
    out.push_str(&format!("- {} --getprojects\n", binary_name));
    out.push_str(&format!("- {} --whoami\n", binary_name));
    out.push_str(&format!("- {} --next-action\n", binary_name));
    out.push_str(&format!("- {} --root-checks\n", binary_name));
    out.push_str(&format!("- {} --release-stale-agent-claims\n", binary_name));
    out.push_str(&format!("- {} --prune-stale-agents\n", binary_name));
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
    out.push_str("- `--root-checks` refreshes governance from the current audit before rendering root-only orchestration findings, so stale sovereign backlog state can self-heal\n");
    out.push_str("- `--whoami` and `--next-action --json` expose `active_task_*` so agents can distinguish current governed work from the last consumed submit\n");
    out.push_str("- rejected submits return `revise_rejected_submit`; accepted or closed submits only fall back to `resume_backlog` when no new active task was assigned\n");
    out.push_str("- `--release-stale-agent-claims` is root-only and returns stale inactive missions to backlog without deleting agents\n");
    out.push_str("- `--prune-stale-agents` is root-only and removes stale idle agents from connected.json without deleting login files\n");
    out.push_str("- sync and submit operations must pass `--branch`, `--base-revision`, `--workspace-revision`, and `--repository-revision`\n");
    out.push_str("- `--submit` is the canonical integration signal and stores both submit evidence artifacts for the current agent\n");
    out.push_str("- `submit_queue.json` is the canonical root-side submit lifecycle queue\n");
    out.push_str("- stale submit diagnostics expose `stale_reason` in `--submits`, `--submit-show`, `swarm status`, and `--whoami`\n");
    out.push_str("- `manager_tasks.json` is the priority backlog that can override normal violation work until marked done\n");
    out.push('\n');
    out.push_str("Canonical submit lifecycle:\n");
    out.push_str("1. Agent runs `--whoami` and confirms assigned mission, agent_id, branch and task metadata.\n");
    out.push_str(
        "2. Agent sends `--submit` with branch/base/workspace/repository revision parameters.\n",
    );
    out.push_str("3. Root inspects `--submits-active` or `--submit-show <submit_id>`.\n");
    out.push_str(
        "4. Root decides with `--submit-accept`, `--submit-reject` or `--submit-close`.\n",
    );
    out.push_str(
        "5. Agent reruns `--whoami` or `--next-action` and either waits, revises a rejected submit, refreshes stale work, continues active task work, or resumes backlog.\n",
    );
    out.push('\n');
    out.push_str("Canonical multi-agent bootstrap:\n");
    out.push_str("1. Place the plugin binary at `plugins/<project>/ac_ant_delirium`.\n");
    out.push_str("2. Run `config init` and review `ant_delirium_config/default_hierarchy.toml`.\n");
    out.push_str("3. Install the generated build guard into every enforced `build.rs` before creating worktrees.\n");
    out.push_str("4. Keep generated login files under `AC_Ant-Delirium_props/login/` out of manual edits and per-agent copies.\n");
    out.push_str("5. Create worktrees only after the sovereign files exist in main.\n");
    out.push_str(
        "6. Run `cargo check` inside each worktree; that is the login event for each agent.\n",
    );
    out.push_str("7. Validate with `swarm status`, `--getprojects`, and `--whoami`.\n");
    out.push_str("8. If an older `governance.json` still contains sovereign files, rerun `--root-checks`; it refreshes governance before reporting root-only findings.\n");
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

pub(super) fn render_plugin_feedback_help(
    plugin: Option<&PluginContext>,
    binary_name: &str,
) -> String {
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

pub(super) fn render_plugin_installation(
    plugin: Option<&PluginContext>,
    binary_name: &str,
) -> String {
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
    out.push_str("Managed local Git hooks:\n");
    out.push_str("1. Run `config hooks init` to generate `AC_Ant-Delirium_props/githooks/` and `AC_Ant-Delirium_props/orchestration/*.template.json`.\n");
    out.push_str("2. This only materializes managed files; the motor does not activate hooks or run external commands for you.\n");
    out.push_str("3. Hook activation is manual and external to ant_delirium.\n");
    out.push_str("4. Current generated policies require local `login.json`, require `agent_id` plus `submit_id` in commit messages, and require explicit merge/push metadata files filled by the human or orchestrator.\n");
    out.push_str("5. Validate those files with `config hooks verify merge` or `config hooks verify push` before using the external hook layer.\n\n");
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
    out.push_str(&format!("- {} config hooks init\n", binary_name));
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

pub(super) fn materialize_project_local_plugin_package(
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

pub(super) fn materialize_managed_git_hooks(
    project_root: &Path,
    explicit_project_root: Option<&str>,
    plugin: Option<&PluginContext>,
    binary_name: &str,
) -> io::Result<String> {
    let current_exe = env::current_exe()?;
    let effective_project_root = match plugin {
        Some(context) => context.repository_path.clone(),
        None => infer_bootstrap_context(project_root, explicit_project_root, &current_exe).0,
    };
    let props_dir = effective_project_root.join("AC_Ant-Delirium_props");
    let hooks_dir = props_dir.join("githooks");
    let orchestration_dir = props_dir.join("orchestration");

    fs::create_dir_all(&hooks_dir)?;
    fs::create_dir_all(&orchestration_dir)?;

    let readme_path = hooks_dir.join("README.md");
    write_generated_file(&readme_path, DEFAULT_GITHOOKS_README)?;

    let merge_template_path = orchestration_dir.join("merge_request.template.json");
    write_generated_file(&merge_template_path, DEFAULT_MERGE_REQUEST_TEMPLATE)?;

    let push_template_path = orchestration_dir.join("push_request.template.json");
    write_generated_file(&push_template_path, DEFAULT_PUSH_REQUEST_TEMPLATE)?;

    let pre_commit_path = hooks_dir.join("pre-commit");
    write_generated_file(&pre_commit_path, &render_pre_commit_hook())?;
    set_executable_if_supported(&pre_commit_path)?;

    let commit_msg_path = hooks_dir.join("commit-msg");
    write_generated_file(&commit_msg_path, &render_commit_msg_hook())?;
    set_executable_if_supported(&commit_msg_path)?;

    let pre_merge_commit_path = hooks_dir.join("pre-merge-commit");
    write_generated_file(&pre_merge_commit_path, &render_pre_merge_commit_hook())?;
    set_executable_if_supported(&pre_merge_commit_path)?;

    let pre_push_path = hooks_dir.join("pre-push");
    write_generated_file(&pre_push_path, &render_pre_push_hook())?;
    set_executable_if_supported(&pre_push_path)?;

    let mut out = String::new();
    out.push_str("Managed Git hooks ensured.\n\n");
    out.push_str(&format!(
        "Project root: {}\nHooks dir: {}\n\n",
        effective_project_root.to_string_lossy(),
        hooks_dir.to_string_lossy()
    ));
    out.push_str("Generated hooks:\n");
    out.push_str("- pre-commit: blocks commits when this workspace has no local login.json\n");
    out.push_str(
        "- commit-msg: requires the current stable agent_id and an explicit submit_id inside the commit message\n",
    );
    out.push_str(
        "- pre-merge-commit: requires AC_Ant-Delirium_props/orchestration/merge_request.json with explicit branch/revision/workspace metadata\n",
    );
    out.push_str(
        "- pre-push: blocks pushes to refs/heads/main and requires AC_Ant-Delirium_props/orchestration/push_request.json\n",
    );
    out.push_str("\nGenerated templates:\n");
    out.push_str(
        "- AC_Ant-Delirium_props/orchestration/merge_request.template.json\n",
    );
    out.push_str(
        "- AC_Ant-Delirium_props/orchestration/push_request.template.json\n",
    );
    out.push_str("\nNext steps:\n");
    out.push_str("- activate AC_Ant-Delirium_props/githooks in your hook system manually\n");
    out.push_str(
        "- copy each *.template.json to the matching *.json only when a human or orchestrator approves the operation\n",
    );
    out.push_str(
        "- fill agent_id, workspace_path, branch, base_revision, workspace_revision and repository_revision explicitly; the motor will not discover them for you\n",
    );
    out.push_str("- validate merge metadata with `config hooks verify merge`\n");
    out.push_str("- validate push metadata with `config hooks verify push`\n");
    out.push_str(&format!("- {} --whoami\n", binary_name));
    out.push_str(
        "- keep these files generated; rerun `config hooks init` when the motor updates the templates\n",
    );
    Ok(out)
}

pub(super) fn validate_managed_orchestration_request(
    project_root: &Path,
    explicit_project_root: Option<&str>,
    plugin: Option<&PluginContext>,
    request_kind: &str,
    json_requested: bool,
) -> io::Result<String> {
    let current_exe = env::current_exe()?;
    let effective_project_root = match plugin {
        Some(context) => context.repository_path.clone(),
        None => infer_bootstrap_context(project_root, explicit_project_root, &current_exe).0,
    };
    let props_dir = effective_project_root.join("AC_Ant-Delirium_props");
    let orchestration_dir = props_dir.join("orchestration");
    let (request_name, template_name, extra_key) = match request_kind {
        "merge" => (
            "merge_request.json",
            "merge_request.template.json",
            None,
        ),
        "push" => (
            "push_request.json",
            "push_request.template.json",
            Some("push_target"),
        ),
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported orchestration request kind: {}", other),
            ))
        }
    };
    let request_path = orchestration_dir.join(request_name);
    if !request_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "missing {}. Copy {} and fill the explicit workspace metadata before retrying",
                request_path.to_string_lossy(),
                orchestration_dir.join(template_name).to_string_lossy()
            ),
        ));
    }

    let raw = fs::read_to_string(&request_path)?;
    let mut fields = BTreeMap::new();
    for key in [
        "agent_id",
        "workspace_path",
        "branch",
        "base_revision",
        "workspace_revision",
        "repository_revision",
        "submit_id",
        "approved_by_agent_id",
        "approval_note",
    ] {
        if let Some(value) = extract_string_field(&raw, key) {
            fields.insert(key.to_string(), value);
        }
    }
    if let Some(key) = extra_key {
        if let Some(value) = extract_string_field(&raw, key) {
            fields.insert(key.to_string(), value);
        }
    }

    let mut missing = Vec::new();
    for key in [
        "agent_id",
        "workspace_path",
        "branch",
        "base_revision",
        "workspace_revision",
        "repository_revision",
        "submit_id",
        "approved_by_agent_id",
    ] {
        if fields.get(key).map(|value| value.trim().is_empty()).unwrap_or(true) {
            missing.push(key.to_string());
        }
    }
    if let Some(key) = extra_key {
        if fields.get(key).map(|value| value.trim().is_empty()).unwrap_or(true) {
            missing.push(key.to_string());
        }
    }
    if !missing.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is missing required non-empty field(s): {}. The motor does not discover Git/VCS state; fill the metadata explicitly from the current workspace context",
                request_path.to_string_lossy(),
                missing.join(", ")
            ),
        ));
    }

    let login_path = plugin.map(|context| context.login_path());
    let workspace_path = plugin.map(|context| context.workspace_path.clone());
    let repository_path = plugin.map(|context| context.repository_path.clone());
    let mut findings = Vec::new();
    if let Some(path) = &workspace_path {
        if normalize_path_str(fields["workspace_path"].as_str()) != normalize_path(path.as_path()) {
            findings.push(format!(
                "workspace_path mismatch: request={}, current={}",
                fields["workspace_path"],
                path.to_string_lossy()
            ));
        }
    }
    if let Some(path) = &login_path {
        if path.is_file() {
            if let Some(snapshot) = read_workspace_login_snapshot(path.as_path()) {
                if snapshot.agent_id != fields["agent_id"] {
                    findings.push(format!(
                        "agent_id mismatch: request={}, login={}",
                        fields["agent_id"], snapshot.agent_id
                    ));
                }
                if !snapshot.branch.trim().is_empty() && snapshot.branch != fields["branch"] {
                    findings.push(format!(
                        "branch mismatch: request={}, login={}",
                        fields["branch"], snapshot.branch
                    ));
                }
            }
        }
    }
    if let Some(path) = &repository_path {
        if normalize_path_str(&effective_project_root.to_string_lossy())
            != normalize_path(path.as_path())
        {
            findings.push(format!(
                "repository root mismatch: request context resolved {}, runtime plugin resolved {}",
                effective_project_root.to_string_lossy(),
                path.to_string_lossy()
            ));
        }
    }
    if !findings.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} validation failed: {}",
                request_name,
                findings.join("; ")
            ),
        ));
    }

    if json_requested {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str("  \"command\": \"hooks_verify\",\n");
        out.push_str(&format!(
            "  \"request_kind\": \"{}\",\n",
            json_escape(request_kind)
        ));
        out.push_str(&format!(
            "  \"request_path\": \"{}\",\n",
            json_escape(&request_path.to_string_lossy())
        ));
        out.push_str("  \"valid\": true,\n");
        out.push_str("  \"fields\": {\n");
        let keys: Vec<String> = fields.keys().cloned().collect();
        for (idx, key) in keys.iter().enumerate() {
            let comma = if idx + 1 < keys.len() { "," } else { "" };
            out.push_str(&format!(
                "    \"{}\": \"{}\"{}\n",
                json_escape(key),
                json_escape(fields.get(key).map(String::as_str).unwrap_or("")),
                comma
            ));
        }
        out.push_str("  }\n");
        out.push_str("}\n");
        return Ok(out);
    }

    let mut out = String::new();
    out.push_str("Managed orchestration request is valid.\n\n");
    out.push_str(&format!("Kind: {}\n", request_kind));
    out.push_str(&format!("Path: {}\n", request_path.to_string_lossy()));
    out.push_str(&format!("Agent ID: {}\n", fields["agent_id"]));
    out.push_str(&format!("Workspace: {}\n", fields["workspace_path"]));
    out.push_str(&format!("Branch: {}\n", fields["branch"]));
    out.push_str(&format!("Base revision: {}\n", fields["base_revision"]));
    out.push_str(&format!(
        "Workspace revision: {}\n",
        fields["workspace_revision"]
    ));
    out.push_str(&format!(
        "Repository revision: {}\n",
        fields["repository_revision"]
    ));
    out.push_str(&format!("Submit ID: {}\n", fields["submit_id"]));
    if let Some(key) = extra_key {
        out.push_str(&format!(
            "{}: {}\n",
            if key == "push_target" {
                "Push target"
            } else {
                key
            },
            fields[key]
        ));
    }
    out.push_str(&format!(
        "Approved by: {}\n",
        fields["approved_by_agent_id"]
    ));
    Ok(out)
}

pub(super) fn build_api_overrides(
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
    installation.push_str("Managed local Git hooks:\n");
    installation.push_str("1. Run `config hooks init` to generate `AC_Ant-Delirium_props/githooks/` and orchestration metadata templates.\n");
    installation.push_str("2. The motor writes templates only; it does not activate hooks or call third-party commands itself.\n");
    installation.push_str("3. Current generated hooks enforce local login presence, require `agent_id` plus `submit_id` in commit messages, require explicit `merge_request.json` and `push_request.json`, and still block pushes to `main`.\n");
    installation.push_str("4. Validate those explicit orchestration files with `config hooks verify merge` and `config hooks verify push` before relying on the external hook layer.\n");
    installation.push_str("5. Treat those hooks as an auxiliary external discipline layer; the JSON artifacts remain the source of truth.\n\n");
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
    installation.push_str(&format!("- {} config hooks init\n", binary_name));
    installation.push_str(&format!("- {} swarm status\n", binary_name));
    installation.push_str(&format!("- {} --next-action\n", binary_name));
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
    installation.push_str(&format!("- {} --governance-history\n", binary_name));
    installation.push_str(&format!(
        "- {} --submit-accept <submit_id> --submit-note \"...\"\n",
        binary_name
    ));
    installation.push_str(&format!(
        "- {} --submit-reject <submit_id> --submit-note \"...\"\n",
        binary_name
    ));
    installation.push_str(&format!(
        "- {} --submit-close <submit_id> --submit-note \"...\"\n",
        binary_name
    ));
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

pub(super) fn discover_plugin_context(
    project_root: &Path,
    argv0: Option<&str>,
) -> Option<PluginContext> {
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

pub(super) fn plugin_context_from_binary_parent(
    project_root: &Path,
    argv0: Option<&str>,
) -> Option<PluginContext> {
    let bin = argv0?;
    let parent = Path::new(bin).parent()?;
    let config_path = resolve_plugin_config_path(parent)?;
    build_plugin_context_from_config(project_root, &config_path)
}

pub(super) fn plugin_context_from_current_exe(project_root: &Path) -> Option<PluginContext> {
    let current_exe = env::current_exe().ok()?;
    let parent = current_exe.parent()?;
    let config_path = resolve_plugin_config_path(parent)?;
    build_plugin_context_from_config(project_root, &config_path)
}

pub(super) fn build_plugin_context_from_config(
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

pub(super) fn resolve_workspace_path(
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

pub(super) fn resolve_plugin_config_path(plugin_dir: &Path) -> Option<PathBuf> {
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

pub(super) fn default_plugin_dir_for_project(project_root: &Path) -> PathBuf {
    let project_name = project_root
        .file_name()
        .unwrap_or_else(|| panic!("project root should have a terminal folder name"));
    project_root
        .parent()
        .unwrap_or(project_root)
        .join("plugins")
        .join(project_name)
}

pub(super) fn infer_bootstrap_context(
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

pub(super) fn looks_like_project_plugin_dir(path: &Path) -> bool {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|value| value.to_str())
        == Some("plugins")
}

pub(super) fn infer_repository_root_from_plugin_dir(plugin_dir: &Path) -> Option<PathBuf> {
    let project_name = plugin_dir.file_name()?;
    let repository_root = plugin_dir.parent()?.parent()?.join(project_name);
    Some(repository_root)
}

pub(super) fn render_new_project_hierarchy(project_root: &Path) -> String {
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

pub(super) fn copy_binary_if_needed(source: &Path, target: &Path) -> io::Result<bool> {
    if paths_resolve_to_same_file(source, target)? {
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, target)?;
    Ok(true)
}

pub(super) fn paths_resolve_to_same_file(left: &Path, right: &Path) -> io::Result<bool> {
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

pub(super) fn write_default_template_if_missing(path: &Path, content: &str) -> io::Result<bool> {
    if path.is_file() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(true)
}

pub(super) fn shell_escape(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    if raw.contains(char::is_whitespace) {
        format!("\"{}\"", raw.replace('"', "\\\""))
    } else {
        raw
    }
}

fn write_generated_file(path: &Path, content: &str) -> io::Result<()> {
    fs::write(path, content)
}

#[cfg(unix)]
fn set_executable_if_supported(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_executable_if_supported(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn render_pre_commit_hook() -> String {
    concat!(
        "#!/bin/sh\n",
        "set -eu\n\n",
        "login_path=\"AC_Ant-Delirium_props/login/login.json\"\n",
        "if [ ! -f \"$login_path\" ]; then\n",
        "  echo \"Ant-Delirium policy:\" >&2\n",
        "  echo \"current workspace is not a registered agent.\" >&2\n",
        "  echo \"Run cargo check first so this workspace generates login.json.\" >&2\n",
        "  exit 1\n",
        "fi\n",
    )
    .to_string()
}

fn render_commit_msg_hook() -> String {
    concat!(
        "#!/bin/sh\n",
        "set -eu\n\n",
        "msg_file=\"$1\"\n",
        "login_path=\"AC_Ant-Delirium_props/login/login.json\"\n",
        "if [ ! -f \"$login_path\" ]; then\n",
        "  echo \"Commit message rejected:\" >&2\n",
        "  echo \"missing local login.json. Run cargo check first.\" >&2\n",
        "  exit 1\n",
        "fi\n",
        "agent_id=$(sed -n 's/.*\"agent_id\": \"\\([^\"]*\\)\".*/\\1/p' \"$login_path\" | head -n 1)\n",
        "if [ -z \"$agent_id\" ]; then\n",
        "  echo \"Commit message rejected:\" >&2\n",
        "  echo \"login.json has no stable agent_id.\" >&2\n",
        "  exit 1\n",
        "fi\n",
        "if ! grep -Fq \"$agent_id\" \"$msg_file\"; then\n",
        "  echo \"Commit message rejected:\" >&2\n",
        "  echo \"missing stable agent identity $agent_id.\" >&2\n",
        "  echo \"Include the current agent_id in the commit message.\" >&2\n",
        "  exit 1\n",
        "fi\n",
        "submit_id=$(sed -n 's/.*\"submit_id\": \"\\([^\"]*\\)\".*/\\1/p' AC_Ant-Delirium_props/orchestration/merge_request.json 2>/dev/null | head -n 1)\n",
        "if [ -z \"$submit_id\" ]; then\n",
        "  submit_id=$(sed -n 's/.*\"submit_id\": \"\\([^\"]*\\)\".*/\\1/p' AC_Ant-Delirium_props/orchestration/push_request.json 2>/dev/null | head -n 1)\n",
        "fi\n",
        "if [ -n \"$submit_id\" ]; then\n",
        "  if ! grep -Fq \"$submit_id\" \"$msg_file\"; then\n",
        "    echo \"Commit message rejected:\" >&2\n",
        "    echo \"missing explicit submit_id $submit_id.\" >&2\n",
        "    echo \"Include the current submit_id in the commit message so orchestration stays traceable.\" >&2\n",
        "    exit 1\n",
        "  fi\n",
        "else\n",
        "  if ! grep -Eq 'submit[_ -]?id[=: ]+[A-Z0-9]{6,}' \"$msg_file\"; then\n",
        "    echo \"Commit message rejected:\" >&2\n",
        "    echo \"missing explicit submit_id marker.\" >&2\n",
        "    echo \"Include submit_id=<ID> in the commit message. The motor does not infer it for you.\" >&2\n",
        "    exit 1\n",
        "  fi\n",
        "fi\n",
    )
    .to_string()
}

fn render_pre_merge_commit_hook() -> String {
    concat!(
        "#!/bin/sh\n",
        "set -eu\n\n",
        "login_path=\"AC_Ant-Delirium_props/login/login.json\"\n",
        "if [ ! -f \"$login_path\" ]; then\n",
        "  echo \"Ant-Delirium policy:\" >&2\n",
        "  echo \"merge commit denied because this workspace has no local login.json.\" >&2\n",
        "  echo \"Run cargo check first or inspect runtime state with --whoami.\" >&2\n",
        "  exit 1\n",
        "fi\n",
        "request_path=\"AC_Ant-Delirium_props/orchestration/merge_request.json\"\n",
        "if [ ! -f \"$request_path\" ]; then\n",
        "  echo \"Merge commit denied:\" >&2\n",
        "  echo \"missing orchestration/merge_request.json with explicit workspace metadata.\" >&2\n",
        "  echo \"Copy merge_request.template.json, fill branch/base_revision/workspace_revision/repository_revision/submit_id/approved_by_agent_id, and retry.\" >&2\n",
        "  exit 1\n",
        "fi\n",
        "for key in agent_id workspace_path branch base_revision workspace_revision repository_revision submit_id approved_by_agent_id; do\n",
        "  if ! grep -Eq '\"'\"$key\"'\"[[:space:]]*:[[:space:]]*\"[^\"]+\"' \"$request_path\"; then\n",
        "    echo \"Merge commit denied:\" >&2\n",
        "    echo \"merge_request.json is missing a non-empty $key field.\" >&2\n",
        "    echo \"The motor does not discover Git state; provide the metadata explicitly.\" >&2\n",
        "    exit 1\n",
        "  fi\n",
        "done\n",
    )
    .to_string()
}

fn render_pre_push_hook() -> String {
    concat!(
        "#!/bin/sh\n",
        "set -eu\n\n",
        "remote_name=${1:-}\n",
        "remote_url=${2:-}\n",
        "request_path=\"AC_Ant-Delirium_props/orchestration/push_request.json\"\n",
        "while read -r local_ref local_sha remote_ref remote_sha; do\n",
        "  if [ \"$remote_ref\" = \"refs/heads/main\" ]; then\n",
        "    echo \"Push denied:\" >&2\n",
        "    echo \"agent workspaces must not push directly to main.\" >&2\n",
        "    echo \"Remote: $remote_name $remote_url\" >&2\n",
        "    exit 1\n",
        "  fi\n",
        "done\n",
        "if [ ! -f \"$request_path\" ]; then\n",
        "  echo \"Push denied:\" >&2\n",
        "  echo \"missing orchestration/push_request.json with explicit workspace metadata.\" >&2\n",
        "  echo \"Copy push_request.template.json, fill branch/base_revision/workspace_revision/repository_revision/push_target/approved_by_agent_id, and retry.\" >&2\n",
        "  exit 1\n",
        "fi\n",
        "for key in agent_id workspace_path branch base_revision workspace_revision repository_revision submit_id push_target approved_by_agent_id; do\n",
        "  if ! grep -Eq '\"'\"$key\"'\"[[:space:]]*:[[:space:]]*\"[^\"]+\"' \"$request_path\"; then\n",
        "    echo \"Push denied:\" >&2\n",
        "    echo \"push_request.json is missing a non-empty $key field.\" >&2\n",
        "    echo \"The motor does not discover Git state; provide the metadata explicitly.\" >&2\n",
        "    exit 1\n",
        "  fi\n",
        "done\n",
    )
    .to_string()
}

pub(super) fn plugin_dir_from_config_path(config_path: &Path) -> Option<PathBuf> {
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

pub(super) fn candidate_plugin_roots(project_root: &Path, argv0: Option<&str>) -> Vec<PathBuf> {
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

pub(super) fn api_root(hierarchy: &HierarchyConfig) -> String {
    for preferred_root in ["hierarchy", "rules", "regras"] {
        if has_semantic_root(hierarchy, preferred_root) {
            return preferred_root.to_string();
        }
    }
    "hierarchy".to_string()
}

pub(super) fn match_plugin_context(
    project_root: &Path,
    plugin_dir: &Path,
    repository_path: &Path,
) -> usize {
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

pub(super) fn extract_repository_path(config_path: &Path) -> Option<PathBuf> {
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

pub(super) fn render_agents_report_json(
    plugin: &PluginContext,
    agents: &[ConnectedAgent],
    tasks: &[GovernanceTask],
    manager_tasks: &[ManagerTask],
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    submit_queue: &[SubmitRecord],
    governance_history: &[GovernanceHistoryEntry],
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
        "    \"governance_history_events\": {},\n",
        governance_history.len()
    ));
    out.push_str(&format!(
        "    \"ready_submits\": {},\n",
        agents
            .iter()
            .filter(|agent| {
                matches!(
                    resolve_agent_submit_state(
                        agents,
                        agent,
                        tasks,
                        messages,
                        feedback,
                        submit_queue,
                        governance_history,
                    )
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
                    resolve_agent_submit_state(
                        agents,
                        agent,
                        tasks,
                        messages,
                        feedback,
                        submit_queue,
                        governance_history,
                    )
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
                    resolve_agent_submit_state(
                        agents,
                        agent,
                        tasks,
                        messages,
                        feedback,
                        submit_queue,
                        governance_history,
                    )
                    .submit_state
                    .as_str(),
                    "blocked_rebase"
                )
            })
            .count()
    ));
    out.push_str(&format!(
        "    \"blocked_task_version_submits\": {},\n",
        agents
            .iter()
            .filter(|agent| {
                matches!(
                    resolve_agent_submit_state(
                        agents,
                        agent,
                        tasks,
                        messages,
                        feedback,
                        submit_queue,
                        governance_history,
                    )
                    .submit_state
                    .as_str(),
                    "blocked_task_version"
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
                    resolve_agent_submit_state(
                        agents,
                        agent,
                        tasks,
                        messages,
                        feedback,
                        submit_queue,
                        governance_history,
                    )
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
        "    \"superseded_submit_records\": {},\n",
        submit_queue
            .iter()
            .filter(|record| record.status == "superseded")
            .count()
    ));
    out.push_str(&format!(
        "    \"stale_submit_records\": {}\n",
        submit_queue
            .iter()
            .filter(|record| record.status == "stale")
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
        let active_task = if is_root_agent_context(plugin, agent) {
            None
        } else {
            owned.first().copied()
        };
        let manager_override = !owned_manager.is_empty();
        let pending_messages = count_pending_messages_for_agent(agent.internal_id, messages);
        let violations: i64 = owned.iter().map(|task| task.violation_count).sum();
        let sync = resolve_agent_sync_state(agents, agent);
        let submit = resolve_agent_submit_state(
            agents,
            agent,
            tasks,
            messages,
            feedback,
            submit_queue,
            governance_history,
        );
        let (operational_feedback, pedagogical_feedback) =
            count_feedback_for_agent(&agent.agent_id, feedback);
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
            "      \"task_id\": \"{}\",\n",
            json_escape(&submit.task_id)
        ));
        out.push_str(&format!(
            "      \"task_version\": {},\n",
            submit.task_version
        ));
        out.push_str(&format!(
            "      \"task_revision\": \"{}\",\n",
            json_escape(&submit.task_revision)
        ));
        out.push_str(&format!(
            "      \"task_file_path\": \"{}\",\n",
            json_escape(&submit.task_file_path)
        ));
        out.push_str(&format!(
            "      \"active_task_id\": \"{}\",\n",
            json_escape(active_task.map(|task| task.task_id.as_str()).unwrap_or(""))
        ));
        out.push_str(&format!(
            "      \"active_task_version\": {},\n",
            active_task
                .map(|task| task.task_version)
                .unwrap_or_default()
        ));
        out.push_str(&format!(
            "      \"active_task_revision\": \"{}\",\n",
            json_escape(
                active_task
                    .map(|task| task.task_revision.as_str())
                    .unwrap_or("")
            )
        ));
        out.push_str(&format!(
            "      \"active_task_content_hash\": \"{}\",\n",
            json_escape(
                active_task
                    .map(|task| task.task_content_hash.as_str())
                    .unwrap_or("")
            )
        ));
        out.push_str(&format!(
            "      \"active_task_file_path\": \"{}\",\n",
            json_escape(
                active_task
                    .map(|task| task.file_path.as_str())
                    .unwrap_or("")
            )
        ));
        out.push_str(&format!(
            "      \"task_stale\": {},\n",
            if submit.task_stale { "true" } else { "false" }
        ));
        out.push_str(&format!(
            "      \"stale_reason\": \"{}\",\n",
            json_escape(&submit.stale_reason)
        ));
        out.push_str(&format!(
            "      \"decision_by_agent_id\": \"{}\",\n",
            json_escape(&submit.decision_by_agent_id)
        ));
        out.push_str(&format!(
            "      \"decision_note\": \"{}\",\n",
            json_escape(&submit.decision_note)
        ));
        out.push_str(&format!("      \"decided_at\": {},\n", submit.decided_at));
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
            "      \"operational_feedback\": {},\n",
            operational_feedback
        ));
        out.push_str(&format!(
            "      \"pedagogical_feedback\": {},\n",
            pedagogical_feedback
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

pub(super) fn render_single_agent_identity(
    plugin: &PluginContext,
    agents: &[ConnectedAgent],
    agent: &ConnectedAgent,
    tasks: &[GovernanceTask],
    manager_tasks: &[ManagerTask],
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    submit_queue: &[SubmitRecord],
    governance_history: &[GovernanceHistoryEntry],
    json_requested: bool,
) -> String {
    let owned = agent_governance_scope(plugin, agent, tasks);
    let owned_manager: Vec<&ManagerTask> = manager_tasks
        .iter()
        .filter(|task| task.owner_id == agent.internal_id && is_manager_task_active(task))
        .collect();
    let manager_override = !owned_manager.is_empty();
    let active_task = if is_root_agent_context(plugin, agent) {
        None
    } else {
        owned.first().copied()
    };
    let pending_messages = count_pending_messages_for_agent(agent.internal_id, messages);
    let violations: i64 = owned.iter().map(|task| task.violation_count).sum();
    let repository_path = resolve_repository_path_for_agent(agent);
    let sync = resolve_agent_sync_state(agents, agent);
    let submit = resolve_agent_submit_state(
        agents,
        agent,
        tasks,
        messages,
        feedback,
        submit_queue,
        governance_history,
    );
    let (action_code, action_text) = agent_identity_action_hint(
        plugin,
        agent,
        tasks,
        manager_tasks,
        &owned,
        &owned_manager,
        &submit,
        submit_queue,
    );
    let (operational_feedback, pedagogical_feedback) =
        count_feedback_for_agent(&agent.agent_id, feedback);
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
            "  \"action_code\": \"{}\",\n",
            json_escape(action_code)
        ));
        out.push_str(&format!(
            "  \"action_text\": \"{}\",\n",
            json_escape(&action_text)
        ));
        out.push_str(&format!(
            "  \"task_id\": \"{}\",\n",
            json_escape(&submit.task_id)
        ));
        out.push_str(&format!("  \"task_version\": {},\n", submit.task_version));
        out.push_str(&format!(
            "  \"task_revision\": \"{}\",\n",
            json_escape(&submit.task_revision)
        ));
        out.push_str(&format!(
            "  \"task_file_path\": \"{}\",\n",
            json_escape(&submit.task_file_path)
        ));
        out.push_str(&format!(
            "  \"active_task_id\": \"{}\",\n",
            json_escape(active_task.map(|task| task.task_id.as_str()).unwrap_or(""))
        ));
        out.push_str(&format!(
            "  \"active_task_version\": {},\n",
            active_task
                .map(|task| task.task_version)
                .unwrap_or_default()
        ));
        out.push_str(&format!(
            "  \"active_task_revision\": \"{}\",\n",
            json_escape(
                active_task
                    .map(|task| task.task_revision.as_str())
                    .unwrap_or("")
            )
        ));
        out.push_str(&format!(
            "  \"active_task_content_hash\": \"{}\",\n",
            json_escape(
                active_task
                    .map(|task| task.task_content_hash.as_str())
                    .unwrap_or("")
            )
        ));
        out.push_str(&format!(
            "  \"active_task_file_path\": \"{}\",\n",
            json_escape(
                active_task
                    .map(|task| task.file_path.as_str())
                    .unwrap_or("")
            )
        ));
        out.push_str(&format!(
            "  \"task_stale\": {},\n",
            if submit.task_stale { "true" } else { "false" }
        ));
        out.push_str(&format!(
            "  \"stale_reason\": \"{}\",\n",
            json_escape(&submit.stale_reason)
        ));
        out.push_str(&format!(
            "  \"decision_by_agent_id\": \"{}\",\n",
            json_escape(&submit.decision_by_agent_id)
        ));
        out.push_str(&format!(
            "  \"decision_note\": \"{}\",\n",
            json_escape(&submit.decision_note)
        ));
        out.push_str(&format!("  \"decided_at\": {},\n", submit.decided_at));
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
            "  \"operational_feedback\": {},\n",
            operational_feedback
        ));
        out.push_str(&format!(
            "  \"pedagogical_feedback\": {},\n",
            pedagogical_feedback
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
    let state_summary = agent_identity_state_summary(
        plugin,
        agent,
        &sync,
        &submit,
        action_code,
        manager_override,
        active_task,
        &owned,
        &owned_manager,
    );
    out.push_str(&format!(
        "Agent {} ({})\n\nState summary: {}\n\n",
        agent.internal_id, agent.agent_id, state_summary
    ));
    out.push_str(&format!(
        "Agent ID: {}\nStatus: {}\nRoot scope: {}\nBranch: {}\nSync state: {}\nRebase required: {}\nSubmit state: {}\nSubmit queue status: {}\nNext action: {} - {}\nSubmit task ID: {}\nSubmit task version: {}\nSubmit task revision: {}\nSubmit task file: {}\nActive task ID: {}\nActive task version: {}\nActive task revision: {}\nActive task content hash: {}\nActive task file: {}\nTask stale: {}\nStale reason: {}\nDecision by: {}\nDecision at: {}\nDecision note: {}\nSubmit message: {}\nSubmit feedback: {}\nOperational feedback: {}\nPedagogical feedback: {}\nManager override: {}\nActive manager tasks: {}\nQueued violation files: {}\nQueued violations: {}\nRepository revision: {}\nWorkspace revision: {}\nBase revision: {}\nRepository: {}\nWorkspace: {}\nLogin: {}\nExecution context: {}\nMission: {}\nPending messages: {}\nViolations: {}\n",
        agent.agent_id,
        status,
        if is_root_agent_context(plugin, agent) { "yes" } else { "no" },
        empty_or_dash(&agent.branch),
        sync.sync_state,
        if sync.rebase_required { "yes" } else { "no" },
        submit.submit_state,
        empty_or_dash(&submit.submit_queue_status),
        action_code,
        action_text,
        empty_or_dash(&submit.task_id),
        submit.task_version,
        empty_or_dash(&submit.task_revision),
        empty_or_dash(&submit.task_file_path),
        empty_or_dash(active_task.map(|task| task.task_id.as_str()).unwrap_or("")),
        active_task.map(|task| task.task_version).unwrap_or_default(),
        empty_or_dash(active_task.map(|task| task.task_revision.as_str()).unwrap_or("")),
        empty_or_dash(
            active_task
                .map(|task| task.task_content_hash.as_str())
                .unwrap_or("")
        ),
        empty_or_dash(active_task.map(|task| task.file_path.as_str()).unwrap_or("")),
        if submit.task_stale { "yes" } else { "no" },
        empty_or_dash(&submit.stale_reason),
        empty_or_dash(&submit.decision_by_agent_id),
        submit.decided_at,
        empty_or_dash(&submit.decision_note),
        if submit.has_submit_message { "yes" } else { "no" },
        if submit.has_submit_feedback { "yes" } else { "no" },
        operational_feedback,
        pedagogical_feedback,
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

fn agent_identity_state_summary(
    plugin: &PluginContext,
    agent: &ConnectedAgent,
    sync: &AgentSyncState,
    submit: &AgentSubmitState,
    action_code: &str,
    manager_override: bool,
    active_task: Option<&GovernanceTask>,
    owned: &[&GovernanceTask],
    owned_manager: &[&ManagerTask],
) -> String {
    if is_root_agent_context(plugin, agent) {
        if action_code == "triage_active_submits" {
            return "root has open submit evidence waiting for triage and should decide accept/reject/close".to_string();
        }
        if action_code == "review_stale_submit_tasks" {
            return "root detected stale submit records and should inspect or consume them before normal backlog flow".to_string();
        }
        if !owned.is_empty() {
            return format!(
                "root is coordinating {} queued governance file(s) and waiting for agent claims",
                owned.len()
            );
        }
        return "root has no queued governance work right now".to_string();
    }

    if manager_override {
        return format!(
            "manager priority work is overriding ordinary governance backlog ({} active manager task(s))",
            owned_manager.len()
        );
    }

    if action_code == "wait_root_triage" {
        return "latest submit is open and this workspace must wait for root triage instead of resending it".to_string();
    }

    if action_code == "revise_rejected_submit" || submit.submit_queue_status == "rejected" {
        return "root rejected the latest submit but the claim is still active, so this workspace should revise the mission and resubmit".to_string();
    }

    if sync.rebase_required {
        return "sync metadata says this workspace must refresh branch/revision context before submit".to_string();
    }

    if submit.task_stale || action_code == "refresh_governance_task" {
        return "latest submit metadata is stale and this workspace must refresh governance state before resubmitting".to_string();
    }

    if let Some(task) = active_task {
        return format!(
            "active governed task is `{}` at version {}",
            task.file_path, task.task_version
        );
    }

    if submit.submit_state == "consumed" && !submit.submit_queue_status.trim().is_empty() {
        return format!(
            "latest submit was consumed as `{}` and no active task is assigned right now",
            submit.submit_queue_status
        );
    }

    if !owned.is_empty() {
        return format!(
            "this workspace owns {} governance file(s) and should continue local backlog work",
            owned.len()
        );
    }

    "no active task, submit, or manager override is assigned right now".to_string()
}

pub(super) fn agent_identity_action_hint(
    plugin: &PluginContext,
    agent: &ConnectedAgent,
    tasks: &[GovernanceTask],
    manager_tasks: &[ManagerTask],
    owned: &[&GovernanceTask],
    owned_manager: &[&ManagerTask],
    submit: &AgentSubmitState,
    submit_queue: &[SubmitRecord],
) -> (&'static str, String) {
    if is_root_agent_context(plugin, agent) {
        let active_submit_count = submit_queue
            .iter()
            .filter(|entry| entry.status == "open")
            .count();
        let open_manager_tasks = manager_tasks
            .iter()
            .filter(|task| task.owner_id < 0 && is_manager_task_active(task))
            .count();
        let open_governance_tasks = tasks.iter().filter(|task| task.owner_id < 0).count();
        if active_submit_count > 0 {
            return (
                "triage_active_submits",
                format!(
                    "inspect {} open submit(s) with --submits-active and decide accept/reject/close",
                    active_submit_count
                ),
            );
        }
        if open_manager_tasks > 0 {
            return (
                "dispatch_manager_priority",
                format!(
                    "{} manager priority task(s) are waiting for agents",
                    open_manager_tasks
                ),
            );
        }
        if open_governance_tasks > 0 {
            return (
                "await_agent_claims",
                format!(
                    "{} governance task(s) are unassigned",
                    open_governance_tasks
                ),
            );
        }
        return (
            "root_idle",
            "root has no active submit or unassigned backlog to triage".to_string(),
        );
    }

    if !owned_manager.is_empty() {
        return (
            "complete_manager_task",
            format!(
                "finish manager priority task `{}` before normal governance backlog",
                owned_manager[0].title
            ),
        );
    }
    if submit.submit_queue_status == "open" && submit.submit_state == "ready_for_root" {
        return (
            "wait_root_triage",
            "submit is already open and waiting for root review; do not resend it yet, and monitor --messages/--inbox".to_string(),
        );
    }
    if submit.submit_state == "blocked_task_version" {
        return (
            "refresh_governance_task",
            "submit is stale; re-run the local audit for the assigned file, confirm the new task metadata, and only then submit again".to_string(),
        );
    }
    if submit.submit_state == "blocked_rebase" {
        return (
            "refresh_sync_metadata",
            "collect fresh branch/base/workspace/repository revisions and retry sync or submit"
                .to_string(),
        );
    }
    if submit.submit_queue_status == "rejected" {
        return (
            "revise_rejected_submit",
            format!(
                "root rejected the latest submit; revise `{}` and submit updated evidence",
                if !agent.assigned_mission.trim().is_empty() {
                    agent.assigned_mission.as_str()
                } else if !owned.is_empty() {
                    owned[0].file_path.as_str()
                } else {
                    "the assigned work"
                }
            ),
        );
    }
    if !agent.assigned_mission.trim().is_empty() {
        return (
            "continue_assigned_mission",
            format!("continue assigned mission `{}`", agent.assigned_mission),
        );
    }
    if !owned.is_empty() {
        return (
            "resume_owned_governance",
            "resume owned governance backlog and reduce local violations".to_string(),
        );
    }
    if matches!(submit.submit_queue_status.as_str(), "accepted" | "closed") {
        return (
            "resume_backlog",
            "latest submit was consumed by root; check current active_task_* state and either resume backlog or wait for a new assignment"
                .to_string(),
        );
    }
    (
        "claim_next_work",
        "no mission is assigned; wait for next backlog claim or manager override".to_string(),
    )
}

pub(super) fn render_message_entry_text(entry: &JsonEntry, agents: &[ConnectedAgent]) -> String {
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
    let task_id = extract_entry_string(entry, "task_id").unwrap_or_default();
    let task_version = extract_entry_number(entry, "task_version").unwrap_or_default();
    let task_revision = extract_entry_string(entry, "task_revision").unwrap_or_default();
    let task_content_hash = extract_entry_string(entry, "task_content_hash").unwrap_or_default();
    let task_file_path = extract_entry_string(entry, "task_file_path").unwrap_or_default();
    let operation_kind = extract_entry_string(entry, "operation_kind").unwrap_or("general");
    let execution_context = extract_entry_string(entry, "execution_context").unwrap_or_default();
    let assigned_mission = extract_entry_string(entry, "assigned_mission").unwrap_or_default();
    let created_at = extract_entry_number(entry, "created_at").unwrap_or_default();
    let sender_status = resolve_agent_by_agent_id(agents, sender_agent_id)
        .map(|_| "known")
        .unwrap_or("unknown");

    format!(
        "- from_agent={} (# {}) [{}] to_agent={} (# {}) topic={} kind={} task={} task_id={} task_version={} task_revision={} task_hash={} visibility={} context={} repository={} branch={} base_revision={} workspace_revision={} repository_revision={} workspace={} login={} mission={} at={} body={}\n",
        empty_or_dash(&sender_agent_id),
        sender_internal_id,
        sender_status,
        empty_or_dash(&target_agent_id),
        target_internal_id,
        empty_or_dash(&topic),
        empty_or_dash(&operation_kind),
        empty_or_dash(&task_file_path),
        empty_or_dash(&task_id),
        task_version,
        empty_or_dash(&task_revision),
        empty_or_dash(&task_content_hash),
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

pub(super) fn render_message_entry_json(entry: &JsonEntry, comma: &str) -> String {
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

pub(super) fn load_runtime_state(
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

pub(super) fn load_feedback_entries(plugin: &PluginContext) -> io::Result<Vec<JsonEntry>> {
    let mut feedback = read_message_entries(&plugin.feedback_path(), "entries")?;
    if prune_feedback_entries(&mut feedback) {
        write_entry_collection(&plugin.feedback_path(), "entries", &feedback)?;
    }
    Ok(feedback)
}

pub(super) fn reconcile_agents_with_login_paths(
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

pub(super) fn collect_root_orchestration_findings(
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
    let governance_history = read_governance_history(&plugin.governance_history_path())?;

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
        &tasks,
        &messages,
        &feedback,
        &submit_queue,
        &governance_history,
    ));
    findings.extend(check_stale_active_agents(&agents));

    Ok(findings)
}

pub(super) fn check_duplicate_agent_identities(
    agents: &[ConnectedAgent],
) -> Vec<RootOrchestrationFinding> {
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

pub(super) fn check_governance_ownership_integrity(
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

pub(super) fn check_manager_task_ownership_integrity(
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

pub(super) fn check_message_integrity(
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

pub(super) fn check_feedback_integrity(feedback: &[JsonEntry]) -> Vec<RootOrchestrationFinding> {
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

pub(super) fn check_submit_evidence(
    agents: &[ConnectedAgent],
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    submit_queue: &[SubmitRecord],
) -> Vec<RootOrchestrationFinding> {
    let mut findings = Vec::new();

    for agent in agents.iter().filter(|agent| agent.internal_id != 0) {
        let has_any_submit_record =
            latest_submit_record_for_agent(submit_queue, &agent.agent_id).is_some();
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
            } else {
                continue;
            }
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

pub(super) fn check_submit_sync_conflicts(
    agents: &[ConnectedAgent],
    tasks: &[GovernanceTask],
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    submit_queue: &[SubmitRecord],
    governance_history: &[GovernanceHistoryEntry],
) -> Vec<RootOrchestrationFinding> {
    let mut findings = Vec::new();

    for agent in agents.iter().filter(|agent| agent.internal_id != 0) {
        if latest_active_submit_record_for_agent(submit_queue, &agent.agent_id).is_none() {
            continue;
        }
        let submit = resolve_agent_submit_state(
            agents,
            agent,
            tasks,
            messages,
            feedback,
            submit_queue,
            governance_history,
        );
        if matches!(
            submit.submit_state.as_str(),
            "blocked_rebase" | "blocked_task_version"
        ) {
            findings.push(RootOrchestrationFinding {
                code: if submit.submit_state == "blocked_task_version" {
                    "submit_task_stale".to_string()
                } else {
                    "submit_requires_rebase".to_string()
                },
                message: format!(
                    "{}",
                    if submit.submit_state == "blocked_task_version" {
                        format!(
                            "agent {} requested submit on stale governance task {} version {}",
                            agent.agent_id,
                            empty_or_dash(&submit.task_file_path),
                            submit.task_version
                        )
                    } else {
                        format!(
                            "agent {} requested submit but persisted metadata says rebase is still required",
                            agent.agent_id
                        )
                    }
                ),
            });
        }
    }

    findings
}

pub(super) fn check_submit_queue_integrity(
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

pub(super) fn check_stale_active_agents(
    agents: &[ConnectedAgent],
) -> Vec<RootOrchestrationFinding> {
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

pub(super) fn summarize_projects(agents: &[ConnectedAgent]) -> Vec<ProjectSummary> {
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

pub(super) fn read_workspace_login_snapshot(path: &Path) -> Option<WorkspaceLoginSnapshot> {
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
        task_id: extract_string_field(&raw, "task_id").unwrap_or_default(),
        task_version: extract_number_field(&raw, "task_version").unwrap_or_default(),
        task_revision: extract_string_field(&raw, "task_revision").unwrap_or_default(),
        task_content_hash: extract_string_field(&raw, "task_content_hash").unwrap_or_default(),
        task_file_path: extract_string_field(&raw, "task_file_path").unwrap_or_default(),
        repository_path: extract_string_field(&raw, "repository_path")?,
        workspace_path,
    })
}

pub(super) fn prune_message_entries(
    agents: &[ConnectedAgent],
    entries: &mut Vec<JsonEntry>,
) -> bool {
    let original_len = entries.len();
    entries.retain(|entry| is_valid_message_entry(agents, entry));
    entries.len() != original_len
}

pub(super) fn prune_feedback_entries(entries: &mut Vec<JsonEntry>) -> bool {
    let original_len = entries.len();
    entries.retain(is_valid_feedback_entry);
    entries.len() != original_len
}

pub(super) fn is_valid_message_entry(agents: &[ConnectedAgent], entry: &JsonEntry) -> bool {
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

pub(super) fn is_valid_feedback_entry(entry: &JsonEntry) -> bool {
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

pub(super) fn resolve_repository_path_for_agent(agent: &ConnectedAgent) -> String {
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

pub(super) fn extract_repository_name_from_workspace_name(workspace_name: &str) -> Option<&str> {
    workspace_name
        .split_once("_")
        .map(|(_, repository_name)| repository_name)
}

pub(super) fn ensure_support_files(plugin: &PluginContext) -> io::Result<()> {
    fs::create_dir_all(plugin.login_dir())?;
    if !plugin.connected_path().exists() {
        write_connected_agents(&plugin.connected_path(), &[])?;
    }
    if !plugin.governance_path().exists() {
        write_governance_tasks(&plugin.governance_path(), &[])?;
    }
    if !plugin.governance_history_path().exists() {
        write_governance_history(&plugin.governance_history_path(), &[])?;
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

pub(super) fn write_context_snapshot(plugin: &PluginContext) -> io::Result<()> {
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

pub(super) fn read_connected_agents(path: &Path) -> io::Result<Vec<ConnectedAgent>> {
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

pub(super) fn write_connected_agents(path: &Path, agents: &[ConnectedAgent]) -> io::Result<()> {
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

pub(super) fn read_governance_tasks(path: &Path) -> io::Result<Vec<GovernanceTask>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, "tasks");
    Ok(objects
        .into_iter()
        .map(|object| {
            let file_path = extract_string_field(&object, "file_path").unwrap_or_default();
            GovernanceTask {
                task_id: extract_string_field(&object, "task_id")
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| governance_task_id_for_path(&file_path)),
                task_version: extract_number_field(&object, "task_version").unwrap_or(1),
                task_revision: extract_string_field(&object, "task_revision").unwrap_or_default(),
                task_content_hash: extract_string_field(&object, "task_content_hash")
                    .unwrap_or_default(),
                file_path,
                violation_count: extract_number_field(&object, "violation_count")
                    .unwrap_or_default(),
                owner_id: extract_number_field(&object, "owner_id").unwrap_or(-1),
            }
        })
        .collect())
}

pub(super) fn read_governance_history(path: &Path) -> io::Result<Vec<GovernanceHistoryEntry>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, "entries");
    Ok(objects
        .into_iter()
        .map(|object| GovernanceHistoryEntry {
            id: extract_string_field(&object, "id").unwrap_or_default(),
            task_id: extract_string_field(&object, "task_id").unwrap_or_default(),
            file_path: extract_string_field(&object, "file_path").unwrap_or_default(),
            change_kind: extract_string_field(&object, "change_kind").unwrap_or_default(),
            previous_version: extract_number_field(&object, "previous_version").unwrap_or_default(),
            current_version: extract_number_field(&object, "current_version").unwrap_or_default(),
            previous_revision: extract_string_field(&object, "previous_revision")
                .unwrap_or_default(),
            current_revision: extract_string_field(&object, "current_revision").unwrap_or_default(),
            previous_content_hash: extract_string_field(&object, "previous_content_hash")
                .unwrap_or_default(),
            current_content_hash: extract_string_field(&object, "current_content_hash")
                .unwrap_or_default(),
            previous_owner_id: extract_number_field(&object, "previous_owner_id").unwrap_or(-1),
            current_owner_id: extract_number_field(&object, "current_owner_id").unwrap_or(-1),
            created_at: extract_number_field(&object, "created_at").unwrap_or_default(),
            previous_event_hash: extract_string_field(&object, "previous_event_hash")
                .unwrap_or_default(),
            event_hash: extract_string_field(&object, "event_hash").unwrap_or_default(),
        })
        .collect())
}

pub(super) fn write_governance_tasks(path: &Path, tasks: &[GovernanceTask]) -> io::Result<()> {
    let mut out = String::new();
    out.push_str("{\n  \"tasks\": [\n");
    for (idx, task) in tasks.iter().enumerate() {
        let comma = if idx + 1 < tasks.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{\n      \"task_id\": \"{}\",\n      \"task_version\": {},\n      \"task_revision\": \"{}\",\n      \"task_content_hash\": \"{}\",\n      \"file_path\": \"{}\",\n      \"violation_count\": {},\n      \"owner_id\": {}\n    }}{}\n",
            json_escape(&task.task_id),
            task.task_version,
            json_escape(&task.task_revision),
            json_escape(&task.task_content_hash),
            json_escape(&task.file_path),
            task.violation_count,
            task.owner_id,
            comma
        ));
    }
    out.push_str("  ]\n}\n");
    fs::write(path, out)
}

pub(super) fn write_governance_history(
    path: &Path,
    entries: &[GovernanceHistoryEntry],
) -> io::Result<()> {
    let mut out = String::new();
    out.push_str("{\n  \"entries\": [\n");
    for (idx, entry) in entries.iter().enumerate() {
        let comma = if idx + 1 < entries.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{\n      \"id\": \"{}\",\n      \"task_id\": \"{}\",\n      \"file_path\": \"{}\",\n      \"change_kind\": \"{}\",\n      \"previous_version\": {},\n      \"current_version\": {},\n      \"previous_revision\": \"{}\",\n      \"current_revision\": \"{}\",\n      \"previous_content_hash\": \"{}\",\n      \"current_content_hash\": \"{}\",\n      \"previous_owner_id\": {},\n      \"current_owner_id\": {},\n      \"created_at\": {},\n      \"previous_event_hash\": \"{}\",\n      \"event_hash\": \"{}\"\n    }}{}\n",
            json_escape(&entry.id),
            json_escape(&entry.task_id),
            json_escape(&entry.file_path),
            json_escape(&entry.change_kind),
            entry.previous_version,
            entry.current_version,
            json_escape(&entry.previous_revision),
            json_escape(&entry.current_revision),
            json_escape(&entry.previous_content_hash),
            json_escape(&entry.current_content_hash),
            entry.previous_owner_id,
            entry.current_owner_id,
            entry.created_at,
            json_escape(&entry.previous_event_hash),
            json_escape(&entry.event_hash),
            comma
        ));
    }
    out.push_str("  ]\n}\n");
    fs::write(path, out)
}

pub(super) fn read_manager_tasks(path: &Path) -> io::Result<Vec<ManagerTask>> {
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

pub(super) fn write_manager_tasks(path: &Path, tasks: &[ManagerTask]) -> io::Result<()> {
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

pub(super) fn read_submit_records(path: &Path) -> io::Result<Vec<SubmitRecord>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, "entries");
    Ok(objects
        .into_iter()
        .map(|object| SubmitRecord {
            id: extract_string_field(&object, "id").unwrap_or_default(),
            internal_id: extract_number_field(&object, "internal_id").unwrap_or(-1),
            agent_id: extract_string_field(&object, "agent_id").unwrap_or_default(),
            task_id: extract_string_field(&object, "task_id").unwrap_or_default(),
            task_version: extract_number_field(&object, "task_version").unwrap_or_default(),
            task_revision: extract_string_field(&object, "task_revision").unwrap_or_default(),
            task_content_hash: extract_string_field(&object, "task_content_hash")
                .unwrap_or_default(),
            task_file_path: extract_string_field(&object, "task_file_path").unwrap_or_default(),
            title: extract_string_field(&object, "title").unwrap_or_default(),
            topic: extract_string_field(&object, "topic").unwrap_or_default(),
            branch: extract_string_field(&object, "branch").unwrap_or_default(),
            base_revision: extract_string_field(&object, "base_revision").unwrap_or_default(),
            workspace_revision: extract_string_field(&object, "workspace_revision")
                .unwrap_or_default(),
            repository_revision: extract_string_field(&object, "repository_revision")
                .unwrap_or_default(),
            status: extract_string_field(&object, "status").unwrap_or_else(|| "open".to_string()),
            decision_by_internal_id: extract_number_field(&object, "decision_by_internal_id")
                .unwrap_or(-1),
            decision_by_agent_id: extract_string_field(&object, "decision_by_agent_id")
                .unwrap_or_default(),
            decision_note: extract_string_field(&object, "decision_note").unwrap_or_default(),
            decided_at: extract_number_field(&object, "decided_at").unwrap_or_default(),
            created_at: extract_number_field(&object, "created_at").unwrap_or_default(),
            updated_at: extract_number_field(&object, "updated_at").unwrap_or_default(),
        })
        .collect())
}

pub(super) fn write_submit_records(path: &Path, entries: &[SubmitRecord]) -> io::Result<()> {
    let mut out = String::new();
    out.push_str("{\n  \"entries\": [\n");
    for (idx, entry) in entries.iter().enumerate() {
        let comma = if idx + 1 < entries.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{\n      \"id\": \"{}\",\n      \"internal_id\": {},\n      \"agent_id\": \"{}\",\n      \"task_id\": \"{}\",\n      \"task_version\": {},\n      \"task_revision\": \"{}\",\n      \"task_content_hash\": \"{}\",\n      \"task_file_path\": \"{}\",\n      \"title\": \"{}\",\n      \"topic\": \"{}\",\n      \"branch\": \"{}\",\n      \"base_revision\": \"{}\",\n      \"workspace_revision\": \"{}\",\n      \"repository_revision\": \"{}\",\n      \"status\": \"{}\",\n      \"decision_by_internal_id\": {},\n      \"decision_by_agent_id\": \"{}\",\n      \"decision_note\": \"{}\",\n      \"decided_at\": {},\n      \"created_at\": {},\n      \"updated_at\": {}\n    }}{}\n",
            json_escape(&entry.id),
            entry.internal_id,
            json_escape(&entry.agent_id),
            json_escape(&entry.task_id),
            entry.task_version,
            json_escape(&entry.task_revision),
            json_escape(&entry.task_content_hash),
            json_escape(&entry.task_file_path),
            json_escape(&entry.title),
            json_escape(&entry.topic),
            json_escape(&entry.branch),
            json_escape(&entry.base_revision),
            json_escape(&entry.workspace_revision),
            json_escape(&entry.repository_revision),
            json_escape(&entry.status),
            entry.decision_by_internal_id,
            json_escape(&entry.decision_by_agent_id),
            json_escape(&entry.decision_note),
            entry.decided_at,
            entry.created_at,
            entry.updated_at,
            comma
        ));
    }
    out.push_str("  ]\n}\n");
    fs::write(path, out)
}

pub(super) fn assign_governance_task(
    tasks: &mut [GovernanceTask],
    submit_queue: &[SubmitRecord],
    owner_id: i64,
) -> Option<String> {
    let suppressed_task_ids = suppressed_governance_task_ids(tasks, submit_queue);
    let mut best_idx = None;
    let mut best_count = i64::MIN;
    for (idx, task) in tasks.iter().enumerate() {
        if task.owner_id >= 0 {
            continue;
        }
        if suppressed_task_ids.contains(&task.task_id) {
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

pub(super) fn claim_manager_task_for_agent(
    tasks: &mut [ManagerTask],
    owner_id: i64,
) -> Option<ManagerTask> {
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

pub(super) fn manager_task_mission(task: &ManagerTask) -> String {
    if task.file_path.trim().is_empty() {
        format!("manager:{}", task.title)
    } else {
        format!("manager:{} [{}]", task.title, task.file_path)
    }
}

pub(super) fn is_manager_task_active(task: &ManagerTask) -> bool {
    !matches!(task.status.trim(), "done" | "closed" | "resolved")
}

pub(super) fn normalize_task_path(path: &str) -> String {
    path.replace('\\', "/")
}

pub(super) fn normalize_agent_topology(
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

pub(super) fn is_valid_registered_workspace_path(path: &str) -> bool {
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

pub(super) fn expected_login_path_from_workspace_path(workspace_path: &Path) -> PathBuf {
    workspace_path
        .join("AC_Ant-Delirium_props")
        .join("login")
        .join("login.json")
}

pub(super) fn ensure_agent_ids(agents: &mut [ConnectedAgent]) {
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

pub(super) fn ensure_agent_branches(agents: &mut [ConnectedAgent]) {
    for agent in agents {
        if agent.branch.trim().is_empty() {
            agent.branch = read_workspace_login_snapshot(Path::new(&agent.login_path))
                .map(|snapshot| snapshot.branch)
                .unwrap_or_default();
        }
    }
}

pub(super) fn ensure_agent_revisions(plugin: &PluginContext, agents: &mut [ConnectedAgent]) {
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

pub(super) fn agent_id_for_workspace_path(workspace_path: &Path) -> String {
    let normalized = normalize_path(workspace_path);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    to_base36_upper(hasher.finish(), 8)
}

pub(super) fn governance_task_id_for_path(file_path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    normalize_task_path(file_path).hash(&mut hasher);
    to_base36_upper(hasher.finish(), 8)
}

pub(super) fn stable_content_hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016X}", hash)
}

pub(super) fn resolve_task_source_path(plugin: &PluginContext, file_path: &str) -> PathBuf {
    let candidate = Path::new(file_path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        plugin.repository_path.join(candidate)
    }
}

pub(super) fn task_content_hash_for_path(plugin: &PluginContext, file_path: &str) -> String {
    let source_path = resolve_task_source_path(plugin, file_path);
    match fs::read(&source_path) {
        Ok(bytes) => stable_content_hash(&bytes),
        Err(_) => String::new(),
    }
}

pub(super) fn current_governance_revision(plugin: &PluginContext) -> String {
    read_workspace_login_snapshot(plugin.login_path().as_path())
        .map(|snapshot| snapshot.repository_revision)
        .unwrap_or_default()
}

pub(super) fn governance_history_entry_id(next_index: usize) -> String {
    format!("GH{:08}", next_index)
}

pub(super) fn governance_history_event_hash(entry: &GovernanceHistoryEntry) -> String {
    stable_content_hash(
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            entry.id,
            entry.task_id,
            entry.file_path,
            entry.change_kind,
            entry.previous_version,
            entry.current_version,
            entry.previous_revision,
            entry.current_revision,
            entry.previous_content_hash,
            entry.current_content_hash,
            entry.previous_owner_id,
            entry.current_owner_id,
            entry.created_at,
            entry.previous_event_hash,
            entry.event_hash
        )
        .as_bytes(),
    )
}

pub(super) fn build_governance_history_entries(
    existing: &[GovernanceHistoryEntry],
    previous_by_path: &BTreeMap<String, GovernanceTask>,
    tasks: &[GovernanceTask],
    created_at: i64,
) -> Vec<GovernanceHistoryEntry> {
    let mut events = Vec::new();
    let mut previous_event_hash = existing
        .last()
        .map(|entry| entry.event_hash.clone())
        .unwrap_or_default();
    let mut next_index = existing.len() + 1;

    for task in tasks {
        let previous = previous_by_path.get(&normalize_task_path(&task.file_path));
        let change_kind = match previous {
            None => Some("created"),
            Some(previous)
                if previous.task_version != task.task_version
                    || previous.task_revision != task.task_revision
                    || previous.task_content_hash != task.task_content_hash
                    || previous.owner_id != task.owner_id =>
            {
                Some(if previous.owner_id >= 0 && task.owner_id < 0 {
                    "released"
                } else if previous.owner_id != task.owner_id && task.owner_id >= 0 {
                    "reassigned"
                } else {
                    "updated"
                })
            }
            _ => None,
        };
        let Some(change_kind) = change_kind else {
            continue;
        };
        let mut entry = GovernanceHistoryEntry {
            id: governance_history_entry_id(next_index),
            task_id: task.task_id.clone(),
            file_path: task.file_path.clone(),
            change_kind: change_kind.to_string(),
            previous_version: previous.map(|value| value.task_version).unwrap_or_default(),
            current_version: task.task_version,
            previous_revision: previous
                .map(|value| value.task_revision.clone())
                .unwrap_or_default(),
            current_revision: task.task_revision.clone(),
            previous_content_hash: previous
                .map(|value| value.task_content_hash.clone())
                .unwrap_or_default(),
            current_content_hash: task.task_content_hash.clone(),
            previous_owner_id: previous.map(|value| value.owner_id).unwrap_or(-1),
            current_owner_id: task.owner_id,
            created_at,
            previous_event_hash: previous_event_hash.clone(),
            event_hash: String::new(),
        };
        entry.event_hash = governance_history_event_hash(&entry);
        previous_event_hash = entry.event_hash.clone();
        next_index += 1;
        events.push(entry);
    }

    for previous in previous_by_path.values() {
        if tasks.iter().any(|task| task.task_id == previous.task_id) {
            continue;
        }
        let mut entry = GovernanceHistoryEntry {
            id: governance_history_entry_id(next_index),
            task_id: previous.task_id.clone(),
            file_path: previous.file_path.clone(),
            change_kind: "removed".to_string(),
            previous_version: previous.task_version,
            current_version: 0,
            previous_revision: previous.task_revision.clone(),
            current_revision: String::new(),
            previous_content_hash: previous.task_content_hash.clone(),
            current_content_hash: String::new(),
            previous_owner_id: previous.owner_id,
            current_owner_id: -1,
            created_at,
            previous_event_hash: previous_event_hash.clone(),
            event_hash: String::new(),
        };
        entry.event_hash = governance_history_event_hash(&entry);
        previous_event_hash = entry.event_hash.clone();
        next_index += 1;
        events.push(entry);
    }

    events
}

pub(super) fn to_base36_upper(mut value: u64, width: usize) -> String {
    const ALPHABET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut chars = vec!['0'; width];
    for idx in (0..width).rev() {
        let digit = (value % 36) as usize;
        chars[idx] = ALPHABET[digit] as char;
        value /= 36;
    }
    chars.into_iter().collect()
}

pub(super) fn sync_metadata_from_snapshot(
    snapshot: Option<WorkspaceLoginSnapshot>,
) -> SyncMetadata {
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

pub(super) fn sync_metadata_from_agent(agent: &ConnectedAgent) -> SyncMetadata {
    SyncMetadata {
        branch: agent.branch.clone(),
        base_revision: agent.base_revision.clone(),
        workspace_revision: agent.workspace_revision.clone(),
        repository_revision: agent.repository_revision.clone(),
    }
}

pub(super) fn sync_metadata_from_cli(
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

pub(super) fn sync_metadata_is_empty(metadata: &SyncMetadata) -> bool {
    metadata.branch.trim().is_empty()
        && metadata.base_revision.trim().is_empty()
        && metadata.workspace_revision.trim().is_empty()
        && metadata.repository_revision.trim().is_empty()
}

pub(super) fn apply_sync_metadata(agent: &mut ConnectedAgent, metadata: &SyncMetadata) {
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

pub(super) fn normalize_operation_kind(raw_kind: Option<&str>) -> String {
    raw_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("general")
        .to_ascii_lowercase()
}

pub(super) fn missing_sync_metadata_fields(metadata: &SyncMetadata) -> Vec<&'static str> {
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

pub(super) fn validate_operation_metadata(
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
            "{} metadata missing for {} operation: provide {}. The motor does not run Git commands; collect these values from your workspace/repository context and retry. Field meanings: --branch is the current work branch name; --base-revision is the revision the workspace was based on; --workspace-revision is the current workspace revision/commit/content marker; --repository-revision is the current root/repository revision the agent must synchronize against. Example: --branch <branch> --base-revision <base> --workspace-revision <workspace> --repository-revision <root>",
            command_name,
            operation_kind,
            missing.join(", ")
        ),
    ))
}

pub(super) fn governance_task_for_agent(
    agent: &ConnectedAgent,
    tasks: &[GovernanceTask],
) -> Option<GovernanceTask> {
    tasks
        .iter()
        .find(|task| {
            task.owner_id == agent.internal_id
                && (agent.assigned_mission.trim().is_empty()
                    || task.file_path == agent.assigned_mission)
        })
        .cloned()
}

pub(super) fn hydrate_governance_task_metadata(
    plugin: &PluginContext,
    mut task: GovernanceTask,
) -> GovernanceTask {
    if task.task_id.trim().is_empty() {
        task.task_id = governance_task_id_for_path(&task.file_path);
    }
    if task.task_version <= 0 {
        task.task_version = 1;
    }
    if task.task_revision.trim().is_empty() {
        task.task_revision = current_governance_revision(plugin);
    }
    if task.task_content_hash.trim().is_empty() {
        task.task_content_hash = task_content_hash_for_path(plugin, &task.file_path);
    }
    task
}

pub(super) fn governance_task_for_agent_from_disk(
    plugin: &PluginContext,
    agent: &ConnectedAgent,
) -> Option<GovernanceTask> {
    read_governance_tasks(&plugin.governance_path())
        .ok()
        .and_then(|tasks| governance_task_for_agent(agent, &tasks))
        .map(|task| hydrate_governance_task_metadata(plugin, task))
}

pub(super) fn revisions_match(left: &str, right: &str) -> bool {
    left.trim().is_empty() || right.trim().is_empty() || left == right
}

pub(super) fn validate_submit_task_freshness(
    plugin: &PluginContext,
    command_name: &str,
    agent: &ConnectedAgent,
    tasks: &[GovernanceTask],
) -> io::Result<Option<GovernanceTask>> {
    let active_task = governance_task_for_agent(agent, tasks)
        .map(|task| hydrate_governance_task_metadata(plugin, task));
    let Some(snapshot) = read_workspace_login_snapshot(Path::new(&agent.login_path)) else {
        return Ok(active_task);
    };
    if snapshot.task_id.trim().is_empty() {
        return Ok(active_task);
    }

    let current_task = tasks
        .iter()
        .find(|task| task.task_id == snapshot.task_id)
        .cloned()
        .map(|task| hydrate_governance_task_metadata(plugin, task));
    let claimed_file = if snapshot.task_file_path.trim().is_empty() {
        agent.assigned_mission.clone()
    } else {
        snapshot.task_file_path.clone()
    };

    let Some(current_task) = current_task else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} submit task metadata is stale: claimed task {} for {} no longer exists in governance; refresh the governance task and retry submit",
                command_name,
                snapshot.task_id,
                empty_or_dash(&claimed_file)
            ),
        ));
    };
    if current_task.owner_id != agent.internal_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} submit task metadata is stale: task {} is no longer owned by agent {}; refresh the governance task and retry submit",
                command_name,
                empty_or_dash(&current_task.file_path),
                agent.agent_id
            ),
        ));
    }
    if current_task.task_version != snapshot.task_version
        || !revisions_match(&current_task.task_revision, &snapshot.task_revision)
        || !revisions_match(&current_task.task_content_hash, &snapshot.task_content_hash)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} submit task metadata is stale for {}: claimed v{} / {} / {} but governance now expects v{} / {} / {}; refresh the governance task and retry submit",
                command_name,
                empty_or_dash(&current_task.file_path),
                snapshot.task_version,
                empty_or_dash(&snapshot.task_revision),
                empty_or_dash(&snapshot.task_content_hash),
                current_task.task_version,
                empty_or_dash(&current_task.task_revision),
                empty_or_dash(&current_task.task_content_hash)
            ),
        ));
    }
    Ok(Some(current_task))
}

pub(super) fn append_sync_metadata_fields(
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

pub(super) fn append_task_metadata_fields(
    fields: &mut Vec<(String, JsonValue)>,
    operation_kind: &str,
    task: Option<&GovernanceTask>,
) {
    if operation_kind != "submit" {
        return;
    }
    let task_id = task.map(|task| task.task_id.clone()).unwrap_or_default();
    let task_version = task.map(|task| task.task_version).unwrap_or_default();
    let task_revision = task
        .map(|task| task.task_revision.clone())
        .unwrap_or_default();
    let task_content_hash = task
        .map(|task| task.task_content_hash.clone())
        .unwrap_or_default();
    let task_file_path = task.map(|task| task.file_path.clone()).unwrap_or_default();

    fields.push(("task_id".to_string(), JsonValue::String(task_id)));
    fields.push(("task_version".to_string(), JsonValue::Number(task_version)));
    fields.push((
        "task_revision".to_string(),
        JsonValue::String(task_revision),
    ));
    fields.push((
        "task_content_hash".to_string(),
        JsonValue::String(task_content_hash),
    ));
    fields.push((
        "task_file_path".to_string(),
        JsonValue::String(task_file_path),
    ));
}

pub(super) fn persist_current_agent_sync_metadata(
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

pub(super) fn resolve_agent_sync_state(
    agents: &[ConnectedAgent],
    agent: &ConnectedAgent,
) -> AgentSyncState {
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

pub(super) fn resolve_agent_submit_state(
    agents: &[ConnectedAgent],
    agent: &ConnectedAgent,
    tasks: &[GovernanceTask],
    messages: &[JsonEntry],
    feedback: &[JsonEntry],
    queue: &[SubmitRecord],
    governance_history: &[GovernanceHistoryEntry],
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
    let latest_submit_record = latest_submit_record_for_agent(queue, &agent.agent_id);
    let latest_active_submit_record = latest_active_submit_record_for_agent(queue, &agent.agent_id);
    let submit_queue_status = latest_submit_record
        .map(|entry| entry.status.clone())
        .unwrap_or_default();
    let task_stale = latest_active_submit_record
        .map(|entry| submit_record_task_stale(entry, tasks))
        .or_else(|| {
            latest_submit_record
                .filter(|entry| entry.status == "stale")
                .map(|_| true)
        })
        .unwrap_or(false);
    let stale_reason = latest_active_submit_record
        .or(latest_submit_record.filter(|entry| entry.status == "stale"))
        .map(|entry| submit_record_stale_reason(entry, tasks, governance_history))
        .unwrap_or_default();
    let submit_state = if matches!(
        submit_queue_status.as_str(),
        "accepted" | "rejected" | "closed"
    ) {
        "consumed"
    } else if submit_queue_status == "stale" {
        "blocked_task_version"
    } else if !has_submit_message && !has_submit_feedback {
        "none"
    } else if has_submit_message && !has_submit_feedback {
        "message_only"
    } else if !has_submit_message && has_submit_feedback {
        "feedback_only"
    } else if task_stale {
        "blocked_task_version"
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
        task_id: latest_submit_record
            .map(|entry| entry.task_id.clone())
            .unwrap_or_default(),
        task_version: latest_submit_record
            .map(|entry| entry.task_version)
            .unwrap_or_default(),
        task_revision: latest_submit_record
            .map(|entry| entry.task_revision.clone())
            .unwrap_or_default(),
        task_file_path: latest_submit_record
            .map(|entry| entry.task_file_path.clone())
            .unwrap_or_default(),
        task_stale,
        stale_reason,
        decision_by_agent_id: latest_submit_record
            .map(|entry| entry.decision_by_agent_id.clone())
            .unwrap_or_default(),
        decision_note: latest_submit_record
            .map(|entry| entry.decision_note.clone())
            .unwrap_or_default(),
        decided_at: latest_submit_record
            .map(|entry| entry.decided_at)
            .unwrap_or_default(),
        latest_submit_status: submit_queue_status,
    }
}

pub(super) fn latest_submit_record_for_agent<'a>(
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

pub(super) fn latest_active_submit_record_for_agent<'a>(
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

pub(super) fn mark_open_submit_records_stale(
    submit_queue: &mut [SubmitRecord],
    tasks: &[GovernanceTask],
    now: i64,
) -> bool {
    let mut changed = false;
    for entry in submit_queue
        .iter_mut()
        .filter(|entry| entry.status == "open")
    {
        if submit_record_task_stale(entry, tasks) {
            entry.status = "stale".to_string();
            entry.updated_at = now;
            changed = true;
        }
    }
    changed
}

pub(super) fn submit_record_task_stale(entry: &SubmitRecord, tasks: &[GovernanceTask]) -> bool {
    if entry.task_id.trim().is_empty() {
        return false;
    }
    let Some(task) = tasks.iter().find(|task| task.task_id == entry.task_id) else {
        return true;
    };
    if entry.status == "open" && task.owner_id != entry.internal_id {
        return true;
    }
    entry.task_version != task.task_version
        || !revisions_match(&entry.task_revision, &task.task_revision)
        || !revisions_match(&entry.task_content_hash, &task.task_content_hash)
}

pub(super) fn submit_record_stale_reason(
    entry: &SubmitRecord,
    tasks: &[GovernanceTask],
    history: &[GovernanceHistoryEntry],
) -> String {
    if entry.task_id.trim().is_empty() {
        return String::new();
    }
    let Some(task) = tasks.iter().find(|task| task.task_id == entry.task_id) else {
        return latest_governance_history_hint(&entry.task_id, history, "task_removed");
    };

    let mut reasons = Vec::new();
    if entry.status == "open" && task.owner_id != entry.internal_id {
        reasons.push(format!(
            "owner_changed:{}->{}",
            entry.internal_id, task.owner_id
        ));
    }
    if entry.task_version != task.task_version {
        reasons.push(format!(
            "version_changed:{}->{}",
            entry.task_version, task.task_version
        ));
    }
    if !revisions_match(&entry.task_revision, &task.task_revision) {
        reasons.push(format!(
            "revision_changed:{}->{}",
            empty_or_dash(&entry.task_revision),
            empty_or_dash(&task.task_revision)
        ));
    }
    if !revisions_match(&entry.task_content_hash, &task.task_content_hash) {
        reasons.push(format!(
            "content_hash_changed:{}->{}",
            empty_or_dash(&entry.task_content_hash),
            empty_or_dash(&task.task_content_hash)
        ));
    }

    if reasons.is_empty() {
        return String::new();
    }

    let hint = latest_governance_history_hint(&entry.task_id, history, "");
    if hint.is_empty() {
        reasons.join(",")
    } else {
        format!("{};{}", reasons.join(","), hint)
    }
}

pub(super) fn latest_governance_history_hint(
    task_id: &str,
    history: &[GovernanceHistoryEntry],
    fallback: &str,
) -> String {
    history
        .iter()
        .rev()
        .find(|entry| entry.task_id == task_id)
        .map(|entry| {
            format!(
                "last_history={}@{}:{}",
                empty_or_dash(&entry.change_kind),
                empty_or_dash(&entry.id),
                empty_or_dash(&entry.event_hash)
            )
        })
        .unwrap_or_else(|| fallback.to_string())
}

pub(super) fn suppressed_governance_task_ids(
    tasks: &[GovernanceTask],
    submit_queue: &[SubmitRecord],
) -> BTreeSet<String> {
    let mut latest_by_task_id: BTreeMap<String, &SubmitRecord> = BTreeMap::new();

    for entry in submit_queue
        .iter()
        .filter(|entry| !entry.task_id.trim().is_empty())
    {
        match latest_by_task_id.get(entry.task_id.as_str()) {
            Some(existing)
                if existing.updated_at > entry.updated_at
                    || (existing.updated_at == entry.updated_at
                        && existing.created_at >= entry.created_at) => {}
            _ => {
                latest_by_task_id.insert(entry.task_id.clone(), entry);
            }
        }
    }

    tasks
        .iter()
        .filter_map(|task| {
            let entry = latest_by_task_id.get(task.task_id.as_str())?;
            if !matches!(entry.status.as_str(), "accepted" | "closed") {
                return None;
            }
            if entry.task_version != task.task_version {
                return None;
            }
            if !entry.task_revision.trim().is_empty()
                && !task.task_revision.trim().is_empty()
                && entry.task_revision != task.task_revision
            {
                return None;
            }
            if !entry.task_content_hash.trim().is_empty()
                && !task.task_content_hash.trim().is_empty()
                && entry.task_content_hash != task.task_content_hash
            {
                return None;
            }
            Some(task.task_id.clone())
        })
        .collect()
}

pub(super) fn release_submit_task_claim(
    plugin: &PluginContext,
    agents: &mut [ConnectedAgent],
    tasks: &mut [GovernanceTask],
    entry: &SubmitRecord,
) -> io::Result<()> {
    if !entry.task_id.trim().is_empty() {
        if let Some(task) = tasks.iter_mut().find(|task| {
            task.task_id == entry.task_id
                && task.owner_id == entry.internal_id
                && task.task_version == entry.task_version
                && (entry.task_revision.trim().is_empty()
                    || task.task_revision.trim().is_empty()
                    || task.task_revision == entry.task_revision)
        }) {
            task.owner_id = -1;
        }
    }

    if let Some(agent) = agents
        .iter_mut()
        .find(|agent| agent.internal_id == entry.internal_id && agent.agent_id == entry.agent_id)
    {
        if !entry.task_file_path.trim().is_empty() && agent.assigned_mission == entry.task_file_path
        {
            agent.assigned_mission.clear();
            agent.last_ping = unix_timestamp();
        }
        write_workspace_login_for_agent(plugin, agent)?;
    }

    write_governance_tasks(&plugin.governance_path(), tasks)?;
    write_connected_agents(&plugin.connected_path(), agents)?;
    Ok(())
}

pub(super) fn resolve_persisted_agent_id(
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

pub(super) fn generate_unique_agent_id(
    existing_agents: &[ConnectedAgent],
    workspace_path: &Path,
) -> String {
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

pub(super) fn generate_manager_task_id(existing_tasks: &[ManagerTask], title: &str) -> String {
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

pub(super) fn generate_submit_id(existing_entries: &[SubmitRecord], title: &str) -> String {
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

pub(super) fn append_submit_record(
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
        let (agents, tasks, _manager_tasks, _messages) = load_runtime_state(plugin)?;
        let current_agent = resolve_current_agent(plugin, &agents).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "submit requires the current workspace to be a reconciled agent",
            )
        })?;
        let active_task = validate_submit_task_freshness(plugin, "submit", current_agent, &tasks)?;
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
            task_id: active_task
                .as_ref()
                .map(|task| task.task_id.clone())
                .unwrap_or_default(),
            task_version: active_task
                .as_ref()
                .map(|task| task.task_version)
                .unwrap_or_default(),
            task_revision: active_task
                .as_ref()
                .map(|task| task.task_revision.clone())
                .unwrap_or_default(),
            task_content_hash: active_task
                .as_ref()
                .map(|task| task.task_content_hash.clone())
                .unwrap_or_default(),
            task_file_path: active_task
                .as_ref()
                .map(|task| task.file_path.clone())
                .unwrap_or_default(),
            title: body.trim().to_string(),
            topic: topic.unwrap_or("submit").trim().to_string(),
            branch: branch.unwrap_or("").trim().to_string(),
            base_revision: base_revision.unwrap_or("").trim().to_string(),
            workspace_revision: workspace_revision.unwrap_or("").trim().to_string(),
            repository_revision: repository_revision.unwrap_or("").trim().to_string(),
            status: "open".to_string(),
            decision_by_internal_id: -1,
            decision_by_agent_id: String::new(),
            decision_note: String::new(),
            decided_at: 0,
            created_at: now,
            updated_at: now,
        });
        write_submit_records(&plugin.submit_queue_path(), &entries)
    })
}

pub(super) fn write_workspace_login(
    plugin: &PluginContext,
    agent: &ConnectedAgent,
) -> io::Result<()> {
    fs::create_dir_all(plugin.login_dir())?;
    let claimed_task = governance_task_for_agent_from_disk(plugin, agent);
    let output = format!(
        "{{\n  \"internal_id\": {},\n  \"agent_id\": \"{}\",\n  \"branch\": \"{}\",\n  \"base_revision\": \"{}\",\n  \"workspace_revision\": \"{}\",\n  \"repository_revision\": \"{}\",\n  \"task_id\": \"{}\",\n  \"task_version\": {},\n  \"task_revision\": \"{}\",\n  \"task_content_hash\": \"{}\",\n  \"task_file_path\": \"{}\",\n  \"repository_path\": \"{}\",\n  \"workspace_path\": \"{}\",\n  \"login_path\": \"{}\",\n  \"plugin_path\": \"{}\",\n  \"execution_context\": \"{}\",\n  \"assigned_mission\": \"{}\",\n  \"last_ping\": {}\n}}\n",
        agent.internal_id,
        json_escape(&agent.agent_id),
        json_escape(&agent.branch),
        json_escape(&agent.base_revision),
        json_escape(&agent.workspace_revision),
        json_escape(&agent.repository_revision),
        json_escape(
            &claimed_task
                .as_ref()
                .map(|task| task.task_id.clone())
                .unwrap_or_default(),
        ),
        claimed_task
            .as_ref()
            .map(|task| task.task_version)
            .unwrap_or_default(),
        json_escape(
            &claimed_task
                .as_ref()
                .map(|task| task.task_revision.clone())
                .unwrap_or_default(),
        ),
        json_escape(
            &claimed_task
                .as_ref()
                .map(|task| task.task_content_hash.clone())
                .unwrap_or_default(),
        ),
        json_escape(
            &claimed_task
                .as_ref()
                .map(|task| task.file_path.clone())
                .unwrap_or_default(),
        ),
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

pub(super) fn write_workspace_login_for_agent(
    plugin: &PluginContext,
    agent: &ConnectedAgent,
) -> io::Result<()> {
    let login_path = PathBuf::from(&agent.login_path);
    let workspace_path = PathBuf::from(&agent.worktree_path);
    let claimed_task = governance_task_for_agent_from_disk(plugin, agent);
    let login_dir = login_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workspace_path.join("AC_Ant-Delirium_props/login"));
    fs::create_dir_all(&login_dir)?;
    let output = format!(
        "{{\n  \"internal_id\": {},\n  \"agent_id\": \"{}\",\n  \"branch\": \"{}\",\n  \"base_revision\": \"{}\",\n  \"workspace_revision\": \"{}\",\n  \"repository_revision\": \"{}\",\n  \"task_id\": \"{}\",\n  \"task_version\": {},\n  \"task_revision\": \"{}\",\n  \"task_content_hash\": \"{}\",\n  \"task_file_path\": \"{}\",\n  \"repository_path\": \"{}\",\n  \"workspace_path\": \"{}\",\n  \"login_path\": \"{}\",\n  \"plugin_path\": \"{}\",\n  \"execution_context\": \"{}\",\n  \"assigned_mission\": \"{}\",\n  \"last_ping\": {}\n}}\n",
        agent.internal_id,
        json_escape(&agent.agent_id),
        json_escape(&agent.branch),
        json_escape(&agent.base_revision),
        json_escape(&agent.workspace_revision),
        json_escape(&agent.repository_revision),
        json_escape(
            &claimed_task
                .as_ref()
                .map(|task| task.task_id.clone())
                .unwrap_or_default(),
        ),
        claimed_task
            .as_ref()
            .map(|task| task.task_version)
            .unwrap_or_default(),
        json_escape(
            &claimed_task
                .as_ref()
                .map(|task| task.task_revision.clone())
                .unwrap_or_default(),
        ),
        json_escape(
            &claimed_task
                .as_ref()
                .map(|task| task.task_content_hash.clone())
                .unwrap_or_default(),
        ),
        json_escape(
            &claimed_task
                .as_ref()
                .map(|task| task.file_path.clone())
                .unwrap_or_default(),
        ),
        json_escape(&plugin.repository_path.to_string_lossy()),
        json_escape(&workspace_path.to_string_lossy()),
        json_escape(&login_path.to_string_lossy()),
        json_escape(&plugin.plugin_dir.to_string_lossy()),
        if normalize_path(workspace_path.as_path()) == normalize_path(plugin.repository_path.as_path()) {
            "repository root"
        } else {
            "git worktree"
        },
        json_escape(&agent.assigned_mission),
        agent.last_ping
    );
    fs::write(login_path, output)
}

pub(super) fn agent_execution_context(
    plugin: &PluginContext,
    agent: &ConnectedAgent,
) -> &'static str {
    if agent.internal_id == 0
        || normalize_path_str(&agent.worktree_path)
            == normalize_path(plugin.repository_path.as_path())
    {
        "repository root"
    } else {
        "git worktree"
    }
}

pub(super) fn plugin_execution_context(plugin: &PluginContext) -> &'static str {
    if is_repository_root_context(plugin) {
        "repository root"
    } else {
        "git worktree"
    }
}

pub(super) fn is_repository_root_context(plugin: &PluginContext) -> bool {
    normalize_path(plugin.workspace_path.as_path())
        == normalize_path(plugin.repository_path.as_path())
}

pub(super) fn is_root_agent_context(plugin: &PluginContext, agent: &ConnectedAgent) -> bool {
    agent.internal_id == 0
        && normalize_path_str(&agent.worktree_path)
            == normalize_path(plugin.repository_path.as_path())
}

pub(super) fn agent_governance_scope<'a>(
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

pub(super) fn resolve_current_agent<'a>(
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

pub(super) fn resolve_agent_by_id(
    agents: &[ConnectedAgent],
    agent_id: i64,
) -> Option<&ConnectedAgent> {
    agents.iter().find(|agent| agent.internal_id == agent_id)
}

pub(super) fn resolve_agent_by_agent_id<'a>(
    agents: &'a [ConnectedAgent],
    agent_id: &str,
) -> Option<&'a ConnectedAgent> {
    agents.iter().find(|agent| agent.agent_id == agent_id)
}

pub(super) fn count_pending_messages_for_agent(agent_id: i64, messages: &[JsonEntry]) -> usize {
    messages
        .iter()
        .filter(|entry| {
            extract_entry_number(entry, "target_internal_id")
                .map(|target| target == agent_id || target < 0)
                .unwrap_or(false)
        })
        .count()
}

pub(super) fn count_feedback_for_agent(agent_id: &str, entries: &[JsonEntry]) -> (usize, usize) {
    let mut operational = 0;
    let mut pedagogical = 0;

    for entry in entries.iter().filter(|entry| {
        extract_entry_string(entry, "sender_agent_id")
            .map(|sender| sender == agent_id)
            .unwrap_or(false)
    }) {
        match extract_entry_string(entry, "operation_kind").unwrap_or("general") {
            "sync" | "submit" => operational += 1,
            _ => pedagogical += 1,
        }
    }

    (operational, pedagogical)
}

pub(super) fn read_message_entries(path: &Path, key: &str) -> io::Result<Vec<JsonEntry>> {
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

pub(super) fn extract_entry_string<'a>(entry: &'a JsonEntry, field: &str) -> Option<&'a str> {
    entry.fields.iter().find_map(|(name, value)| match value {
        JsonValue::String(value) if name == field => Some(value.as_str()),
        _ => None,
    })
}

pub(super) fn extract_entry_number(entry: &JsonEntry, field: &str) -> Option<i64> {
    entry.fields.iter().find_map(|(name, value)| match value {
        JsonValue::Number(value) if name == field => Some(*value),
        _ => None,
    })
}

pub(super) fn write_entry_collection(
    path: &Path,
    key: &str,
    entries: &[JsonEntry],
) -> io::Result<()> {
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

pub(super) fn parse_object_array(raw: &str, key: &str) -> Vec<String> {
    let Some(array_content) = extract_named_array(raw, key) else {
        return Vec::new();
    };
    split_top_level_objects(&array_content)
}

pub(super) fn extract_named_array(raw: &str, key: &str) -> Option<String> {
    let key_pattern = format!("\"{}\"", key);
    let start = raw.find(&key_pattern)?;
    let bracket_idx = raw[start..].find('[')? + start;
    let end = find_matching_delimiter(raw, bracket_idx, '[', ']')?;
    Some(raw[bracket_idx + 1..end].to_string())
}

pub(super) fn split_top_level_objects(raw: &str) -> Vec<String> {
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

pub(super) fn extract_string_field(raw: &str, field: &str) -> Option<String> {
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

pub(super) fn extract_number_field(raw: &str, field: &str) -> Option<i64> {
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

pub(super) fn find_matching_delimiter(
    raw: &str,
    start: usize,
    open: char,
    close: char,
) -> Option<usize> {
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

pub(super) fn read_text_or_empty(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err),
    }
}

pub(super) fn resolve_cli_path(project_root: &Path, raw_path: &str) -> PathBuf {
    let candidate = Path::new(raw_path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    }
}

pub(super) fn load_hierarchy_from_path(path: &Path) -> HierarchyConfig {
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

pub(super) fn select_runtime_profile_folder(path: &Path) -> PathBuf {
    let type_1 = path.join("type_1");
    if type_1.is_dir() {
        type_1
    } else {
        path.to_path_buf()
    }
}

pub(super) fn hierarchy_hint_path(project_root: &Path, source: &Path) -> String {
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

pub(super) fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

pub(super) fn with_runtime_lock<T, F>(plugin: &PluginContext, operation: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T>,
{
    let _guard = RuntimeLockGuard::acquire(&plugin.plugin_dir)?;
    operation()
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

pub(super) fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
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

pub(super) fn has_semantic_root(hierarchy: &HierarchyConfig, root: &str) -> bool {
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

pub(super) fn normalize_path(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

pub(super) fn normalize_path_str(path: &str) -> String {
    normalize_path(Path::new(path))
}

pub(super) fn json_escape(input: &str) -> String {
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

pub(super) fn unescape_json_string(input: &str) -> String {
    input
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

pub(super) fn empty_or_dash(value: &str) -> &str {
    if value.trim().is_empty() {
        "-"
    } else {
        value
    }
}
