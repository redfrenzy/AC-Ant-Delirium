use std::env;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ac_ant_delirium::api::default_rules_path;
use ac_ant_delirium::api::InteractiveApi;
use ac_ant_delirium::engine::Manifest_Engine_unit::ManifestEngine;
use ac_ant_delirium::engine::Swarm_Engine_unit::SwarmEngine;
use ac_ant_delirium::Logs::GovernanceLogs;
use ac_ant_delirium::restricted_tokens::out::elite_out;
use ac_ant_delirium::structs::{
    AuditConfig, CliArgs, GovernanceMode, HierarchyConfig, PluginContext,
};

use super::agent_runtime;

pub(super) fn run_main() {
    let args: Vec<String> = env::args().collect();
    let binary_name = runtime_binary_display(&args);
    let cli = parse_cli_args(&args[1..]);
    let project_root = detect_project_root(cli.project_root.as_deref());
    let mode = resolve_mode(&cli);

    let runtime_context = agent_runtime::discover_runtime_context(
        &project_root,
        cli.config_path.as_deref(),
        args.first().map(String::as_str),
    );
    let plugin_context = runtime_context.plugin.clone();

    if args.len() == 1 || cli.help_requested {
        let output = agent_runtime::render_plugin_help(plugin_context.as_ref(), &binary_name);
        elite_out(&wrap_text_output("help", &output, cli.json_requested));
        return;
    }

    if cli.version_requested {
        elite_out(&render_version_output(&binary_name, cli.json_requested));
        return;
    }

    if let Some(plugin) = &plugin_context {
        if let Err(err) = agent_runtime::sync_agent_registration(plugin) {
            elite_out(&format!("[!] failed to sync agent registration: {}", err));
        }
    }

    if let Some(query) = cli.plugin_info_query.as_deref() {
        let output = match query {
            "help" => agent_runtime::render_plugin_help(plugin_context.as_ref(), &binary_name),
            "installation" => {
                agent_runtime::render_plugin_installation(plugin_context.as_ref(), &binary_name)
            }
            "feedback_help" => {
                agent_runtime::render_plugin_feedback_help(plugin_context.as_ref(), &binary_name)
            }
            _ => agent_runtime::render_plugin_help(plugin_context.as_ref(), &binary_name),
        };
        elite_out(&wrap_text_output(query, &output, cli.json_requested));
        return;
    }

    if cli.release_stale_agent_claims {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::release_stale_agent_claims(
                plugin,
                cli.json_requested,
            ) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "release_stale_agent_claims",
                    &format!("[!] failed to release stale agent claims: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "release_stale_agent_claims",
                "[!] no plugin context detected for --release-stale-agent-claims",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.prune_stale_agents {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::prune_stale_agents(plugin, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "prune_stale_agents",
                    &format!("[!] failed to prune stale agents: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "prune_stale_agents",
                "[!] no plugin context detected for --prune-stale-agents",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.next_action {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_next_action(plugin, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "next_action",
                    &format!("[!] failed to determine next action: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "next_action",
                "[!] no plugin context detected for --next-action",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.governance_history {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_governance_history(plugin, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "governance_history",
                    &format!("[!] failed to read governance history: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "governance_history",
                "[!] no plugin context detected for --governance-history",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.submits || cli.submits_active || cli.submits_history || cli.submit_show.is_some() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_submit_queue(
                plugin,
                cli.json_requested,
                if cli.submits_active {
                    Some("active")
                } else if cli.submits_history {
                    Some("history")
                } else {
                    None
                },
                cli.submit_show.as_deref(),
            ) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "submits",
                    &format!("[!] failed to read submit queue: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "submits",
                "[!] no plugin context detected for --submits",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(submit_id) = cli.submit_accept.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::update_submit_status(
                plugin,
                submit_id,
                "accepted",
                cli.submit_note.as_deref(),
            ) {
                Ok(message) => elite_out(&wrap_text_output("submit_accept", &message, cli.json_requested)),
                Err(err) => elite_out(&wrap_text_output(
                    "submit_accept",
                    &format!("[!] failed to accept submit: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "submit_accept",
                "[!] no plugin context detected for --submit-accept",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(submit_id) = cli.submit_reject.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::update_submit_status(
                plugin,
                submit_id,
                "rejected",
                cli.submit_note.as_deref(),
            ) {
                Ok(message) => elite_out(&wrap_text_output("submit_reject", &message, cli.json_requested)),
                Err(err) => elite_out(&wrap_text_output(
                    "submit_reject",
                    &format!("[!] failed to reject submit: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "submit_reject",
                "[!] no plugin context detected for --submit-reject",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(submit_id) = cli.submit_close.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::update_submit_status(
                plugin,
                submit_id,
                "closed",
                cli.submit_note.as_deref(),
            ) {
                Ok(message) => elite_out(&wrap_text_output("submit_close", &message, cli.json_requested)),
                Err(err) => elite_out(&wrap_text_output(
                    "submit_close",
                    &format!("[!] failed to close submit: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "submit_close",
                "[!] no plugin context detected for --submit-close",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.manager_tasks {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_manager_tasks(plugin, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "manager_tasks",
                    &format!("[!] failed to read manager tasks: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "manager_tasks",
                "[!] no plugin context detected for --manager-tasks",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(title) = cli.manager_task_add.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::append_manager_task(
                plugin,
                title,
                cli.manager_task_body.as_deref(),
                cli.manager_task_file.as_deref(),
                cli.manager_task_priority.unwrap_or(100),
            ) {
                Ok(message) => elite_out(&wrap_text_output(
                    "manager_task_add",
                    &message,
                    cli.json_requested,
                )),
                Err(err) => elite_out(&wrap_text_output(
                    "manager_task_add",
                    &format!("[!] failed to store manager task: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "manager_task_add",
                "[!] no plugin context detected for --manager-task-add",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(task_id) = cli.manager_task_close.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::close_manager_task(plugin, task_id) {
                Ok(message) => elite_out(&wrap_text_output(
                    "manager_task_close",
                    &message,
                    cli.json_requested,
                )),
                Err(err) => elite_out(&wrap_text_output(
                    "manager_task_close",
                    &format!("[!] failed to close manager task: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "manager_task_close",
                "[!] no plugin context detected for --manager-task-done",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(body) = cli.submit_body.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::append_submit(
                plugin,
                body,
                cli.message_topic.as_deref(),
                cli.feedback_context.as_deref(),
                cli.branch.as_deref(),
                cli.base_revision.as_deref(),
                cli.workspace_revision.as_deref(),
                cli.repository_revision.as_deref(),
                &binary_name,
            ) {
                Ok(message) => elite_out(&wrap_text_output("submit", &message, cli.json_requested)),
                Err(err) => elite_out(&wrap_text_output(
                    "submit",
                    &format!("[!] failed to store submit evidence: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "submit",
                "[!] no plugin context detected for --submit",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(body) = cli.message_body.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::append_message(
                plugin,
                cli.message_target_agent_id,
                cli.message_target_agent_uid.as_deref(),
                cli.message_sender_agent_id,
                cli.message_sender_agent_uid.as_deref(),
                cli.message_topic.as_deref(),
                cli.message_kind.as_deref(),
                cli.branch.as_deref(),
                cli.base_revision.as_deref(),
                cli.workspace_revision.as_deref(),
                cli.repository_revision.as_deref(),
                body,
            ) {
                Ok(message) => elite_out(&wrap_text_output("message", &message, cli.json_requested)),
                Err(err) => elite_out(&wrap_text_output(
                    "message",
                    &format!("[!] failed to store message: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "message",
                "[!] no plugin context detected for --message",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.messages_feed {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_messages_feed(plugin, None, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "messages",
                    &format!("[!] failed to read messages: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "messages",
                "[!] no plugin context detected for --messages",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(topic) = cli.thread_topic.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_messages_feed(plugin, Some(topic), cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "thread",
                    &format!("[!] failed to read thread {}: {}", topic, err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "thread",
                "[!] no plugin context detected for --thread",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.inbox {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_current_inbox(plugin) {
                Ok(message) => elite_out(&wrap_text_output("inbox", &message, cli.json_requested)),
                Err(err) => elite_out(&wrap_text_output(
                    "inbox",
                    &format!("[!] failed to read inbox: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "inbox",
                "[!] no plugin context detected for --inbox",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.whoami {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_current_agent_identity(plugin, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "whoami",
                    &format!("[!] failed to resolve current agent: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "whoami",
                "[!] no plugin context detected for --whoami",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.rebase_check {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_rebase_check(plugin, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "rebase_check",
                    &format!("[!] failed to evaluate rebase state: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "rebase_check",
                "[!] no plugin context detected for --rebase-check",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(agent_uid) = cli.show_agent_uid.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_agent_identity_by_agent_id(
                plugin,
                agent_uid,
                cli.json_requested,
            ) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "agent",
                    &format!("[!] failed to resolve agent {}: {}", agent_uid, err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "agent",
                "[!] no plugin context detected for --show-agent-id",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(agent_id) = cli.show_agent_id {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_agent_identity(plugin, agent_id, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "agent",
                    &format!("[!] failed to resolve agent {}: {}", agent_id, err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "agent",
                "[!] no plugin context detected for --show-agent",
                cli.json_requested,
            )),
        }
        return;
    }

    if let Some(body) = cli.feedback.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::append_feedback(
                plugin,
                body,
                cli.feedback_context.as_deref(),
                cli.feedback_kind.as_deref(),
                cli.branch.as_deref(),
                cli.base_revision.as_deref(),
                cli.workspace_revision.as_deref(),
                cli.repository_revision.as_deref(),
                &binary_name,
            ) {
                Ok(message) => elite_out(&wrap_text_output(
                    "feedback",
                    &message,
                    cli.json_requested,
                )),
                Err(err) => elite_out(&wrap_text_output(
                    "feedback",
                    &format!("[!] failed to store feedback: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "feedback",
                "[!] no plugin context detected for --feedback",
                cli.json_requested,
            )),
        }
        return;
    }

    let (mut hierarchy, hierarchy_hint_path) = match (
        runtime_context.hierarchy,
        runtime_context.hierarchy_hint_path,
    ) {
        (Some(hierarchy), Some(hint)) => (hierarchy, hint),
        _ => load_runtime_hierarchy(&project_root, cli.config_path.as_deref()),
    };
    hierarchy.attach_runtime_project_root(&project_root.to_string_lossy());

    if cli.config_dump {
        let output = render_config_dump(
            &project_root,
            &hierarchy,
            &hierarchy_hint_path,
            plugin_context.as_ref(),
            &binary_name,
            cli.json_requested,
        );
        elite_out(&output);
        return;
    }

    if cli.config_init {
        let output = match agent_runtime::materialize_project_local_plugin_package(
            &project_root,
            cli.project_root.as_deref(),
            plugin_context.as_ref(),
            &binary_name,
        ) {
            Ok(report) => wrap_text_output("config_init", &report, cli.json_requested),
            Err(err) => wrap_text_output(
                "config_init",
                &format!("[!] failed to initialize project-local package: {}", err),
                cli.json_requested,
            ),
        };
        elite_out(&output);
        return;
    }

    if cli.hooks_init {
        let output = match agent_runtime::materialize_managed_git_hooks(
            &project_root,
            cli.project_root.as_deref(),
            plugin_context.as_ref(),
            &binary_name,
        ) {
            Ok(report) => wrap_text_output("hooks_init", &report, cli.json_requested),
            Err(err) => wrap_text_output(
                "hooks_init",
                &format!("[!] failed to initialize managed git hooks: {}", err),
                cli.json_requested,
            ),
        };
        elite_out(&output);
        return;
    }

    if cli.hooks_verify_merge {
        let output = match agent_runtime::validate_managed_orchestration_request(
            &project_root,
            cli.project_root.as_deref(),
            plugin_context.as_ref(),
            "merge",
            cli.json_requested,
        ) {
            Ok(report) => report,
            Err(err) => wrap_text_output(
                "hooks_verify_merge",
                &format!("[!] failed to validate merge_request.json: {}", err),
                cli.json_requested,
            ),
        };
        elite_out(&output);
        return;
    }

    if cli.hooks_verify_push {
        let output = match agent_runtime::validate_managed_orchestration_request(
            &project_root,
            cli.project_root.as_deref(),
            plugin_context.as_ref(),
            "push",
            cli.json_requested,
        ) {
            Ok(report) => report,
            Err(err) => wrap_text_output(
                "hooks_verify_push",
                &format!("[!] failed to validate push_request.json: {}", err),
                cli.json_requested,
            ),
        };
        elite_out(&output);
        return;
    }

    if cli.api_requested {
        let info_overrides = plugin_context
            .as_ref()
            .and_then(|plugin| {
                agent_runtime::build_api_overrides(plugin, &hierarchy, &binary_name)
                    .ok()
            })
            .unwrap_or_default();
        let api = InteractiveApi::new(
            hierarchy,
            project_root,
            hierarchy_hint_path,
            binary_name,
        )
        .with_info_overrides(info_overrides);
        let rendered = api.render(cli.api_query.as_deref());
        elite_out(&wrap_text_output("api", &rendered, cli.json_requested));
        return;
    }

    if let Some(path) = cli.search_path.as_deref() {
        if let Some(result) = hierarchy.search_params(path) {
            elite_out(&render_search_output(path, &result, cli.json_requested));
        } else {
            elite_out(&render_search_not_found(path, cli.json_requested));
        }
        return;
    }

    let config = AuditConfig {
        hierarchy,
        runtime: ac_ant_delirium::structs::RuntimeConfig {
            project_root: project_root.to_string_lossy().to_string(),
            mode,
        },
        ..Default::default()
    };

    if cli.root_checks {
        match plugin_context.as_ref() {
            Some(plugin) => {
                if is_repository_root_plugin(plugin) {
                    if let Err(err) = refresh_runtime_governance_state(plugin, &config) {
                        elite_out(&wrap_text_output(
                            "root_orchestration_checks",
                            &format!("[!] failed to refresh governance before root checks: {}", err),
                            cli.json_requested,
                        ));
                        return;
                    }
                }
                match agent_runtime::render_root_orchestration_violations(plugin, cli.json_requested) {
                    Ok(Some(report)) => elite_out(&report),
                    Ok(None) => elite_out(&wrap_text_output(
                        "root_orchestration_checks",
                        "Root orchestration checks passed",
                        cli.json_requested,
                    )),
                    Err(err) => elite_out(&wrap_text_output(
                        "root_orchestration_checks",
                        &format!("[!] failed to evaluate root orchestration checks: {}", err),
                        cli.json_requested,
                    )),
                }
            }
            None => elite_out(&wrap_text_output(
                "root_orchestration_checks",
                "[!] no plugin context detected for --root-checks",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.get_projects {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_projects_report(plugin, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "projects",
                    &format!("[!] failed to read projects: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "projects",
                "[!] no plugin context detected for project status",
                cli.json_requested,
            )),
        }
        return;
    }

    if cli.get_agents || cli.swarm_status {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_agents_report(plugin, cli.json_requested) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&wrap_text_output(
                    "swarm_status",
                    &format!("[!] failed to read agents: {}", err),
                    cli.json_requested,
                )),
            },
            None => elite_out(&wrap_text_output(
                "swarm_status",
                "[!] no plugin context detected for swarm status",
                cli.json_requested,
            )),
        }
        return;
    }

    let full_audit_result = SwarmEngine::audit(&config);
    if let Some(plugin) = plugin_context.as_ref() {
        if let Err(err) = agent_runtime::refresh_governance_from_audit(plugin, &full_audit_result) {
            elite_out(&format!("[!] failed to refresh governance tasks: {}", err));
        } else if let Err(err) = agent_runtime::assign_task_to_current_agent_if_missing(plugin) {
            elite_out(&format!("[!] failed to assign governance task: {}", err));
        }
        match agent_runtime::render_root_orchestration_violations(plugin, cli.json_requested) {
            Ok(Some(report)) => {
                elite_out(&report);
                return;
            }
            Ok(None) => {}
            Err(err) => {
                elite_out(&wrap_text_output(
                    "root_orchestration_checks",
                    &format!("[!] failed to evaluate root orchestration checks: {}", err),
                    cli.json_requested,
                ));
                return;
            }
        }
    }

    let audit_result = match plugin_context.as_ref() {
        Some(plugin) => match agent_runtime::isolate_audit_to_current_agent(plugin, &full_audit_result)
        {
            Ok(filtered) => filtered,
            Err(err) => {
                elite_out(&format!("[!] failed to isolate agent audit: {}", err));
                full_audit_result.clone()
            }
        },
        None => full_audit_result.clone(),
    };
    let _ = ManifestEngine::generate(&config);
    let artifact_paths = GovernanceLogs::write_artifacts(
        &audit_result,
        &config,
        GovernanceLogs::DEFAULT_OUTPUT_ROOT,
    ).ok();

    if cli.json_requested {
        elite_out(&GovernanceLogs::to_violation_manifest(
            &audit_result,
            &config,
            artifact_paths.as_ref(),
        ));
    } else {
        GovernanceLogs::emit_terminal_report(&audit_result, &config, artifact_paths.as_ref());
    }

    if !audit_result.violations.is_empty() {
        std::process::exit(1);
    }
}

pub(super) fn parse_cli_args(args: &[String]) -> CliArgs {
    let mut parsed = CliArgs::default();
    let mut idx = 0;

    while idx < args.len() {
        match args[idx].as_str() {
            "--help" | "-h" => {
                parsed.help_requested = true;
                idx += 1;
                continue;
            }
            "--version" | "-v" => {
                parsed.version_requested = true;
                idx += 1;
                continue;
            }
            "--json" => {
                parsed.json_requested = true;
                idx += 1;
                continue;
            }
            "--plugin.help" => {
                parsed.plugin_info_query = Some("help".to_string());
                idx += 1;
                continue;
            }
            "--help.feedback" | "--plugin.help.feedback" => {
                parsed.plugin_info_query = Some("feedback_help".to_string());
                idx += 1;
                continue;
            }
            "--installation" | "--plugin.installation" => {
                parsed.plugin_info_query = Some("installation".to_string());
                idx += 1;
                continue;
            }
            "audit" => {
                idx += 1;
                continue;
            }
            "config" => {
                match args.get(idx + 1).map(String::as_str) {
                    Some("dump") => {
                        parsed.config_dump = true;
                        idx += 2;
                        continue;
                    }
                    Some("init") => {
                        parsed.config_init = true;
                        idx += 2;
                        continue;
                    }
                    Some("hooks")
                        if matches!(args.get(idx + 2).map(String::as_str), Some("init")) =>
                    {
                        parsed.hooks_init = true;
                        idx += 3;
                        continue;
                    }
                    Some("hooks")
                        if matches!(args.get(idx + 2).map(String::as_str), Some("verify"))
                            && matches!(args.get(idx + 3).map(String::as_str), Some("merge")) =>
                    {
                        parsed.hooks_verify_merge = true;
                        idx += 4;
                        continue;
                    }
                    Some("hooks")
                        if matches!(args.get(idx + 2).map(String::as_str), Some("verify"))
                            && matches!(args.get(idx + 3).map(String::as_str), Some("push")) =>
                    {
                        parsed.hooks_verify_push = true;
                        idx += 4;
                        continue;
                    }
                    _ => {}
                }
            }
            "--init" => {
                parsed.config_init = true;
                idx += 1;
                continue;
            }
            "swarm" => {
                if matches!(args.get(idx + 1).map(String::as_str), Some("status")) {
                    parsed.swarm_status = true;
                    idx += 2;
                    continue;
                }
            }
            "--project-root" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.project_root = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--api" => {
                parsed.api_requested = true;
                if let Some(value) = args.get(idx + 1) {
                    if !value.starts_with("--") {
                        parsed.api_query = Some(value.clone());
                        idx += 2;
                        continue;
                    }
                }
                idx += 1;
                continue;
            }
            "--config" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.config_path = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--search" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.search_path = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--getagents" => {
                parsed.get_agents = true;
                parsed.swarm_status = true;
                idx += 1;
                continue;
            }
            "--getprojects" => {
                parsed.get_projects = true;
                idx += 1;
                continue;
            }
            "--whoami" => {
                parsed.whoami = true;
                idx += 1;
                continue;
            }
            "--next-action" => {
                parsed.next_action = true;
                idx += 1;
                continue;
            }
            "--root-checks" => {
                parsed.root_checks = true;
                idx += 1;
                continue;
            }
            "--release-stale-agent-claims" => {
                parsed.release_stale_agent_claims = true;
                idx += 1;
                continue;
            }
            "--prune-stale-agents" => {
                parsed.prune_stale_agents = true;
                idx += 1;
                continue;
            }
            "--rebase-check" => {
                parsed.rebase_check = true;
                idx += 1;
                continue;
            }
            "--governance-history" => {
                parsed.governance_history = true;
                idx += 1;
                continue;
            }
            "--messages" => {
                parsed.messages_feed = true;
                idx += 1;
                continue;
            }
            "--thread" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.thread_topic = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--show-agent" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.show_agent_id = Some(value.parse::<i64>().unwrap_or(-1));
                    idx += 2;
                    continue;
                }
            }
            "--show-agent-id" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.show_agent_uid = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--inbox" => {
                parsed.inbox = true;
                idx += 1;
                continue;
            }
            "--submits" => {
                parsed.submits = true;
                idx += 1;
                continue;
            }
            "--submits-active" => {
                parsed.submits_active = true;
                idx += 1;
                continue;
            }
            "--submits-history" => {
                parsed.submits_history = true;
                idx += 1;
                continue;
            }
            "--submit-show" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.submit_show = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--submit-note" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.submit_note = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--submit-accept" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.submit_accept = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--submit-reject" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.submit_reject = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--submit-close" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.submit_close = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--manager-tasks" => {
                parsed.manager_tasks = true;
                idx += 1;
                continue;
            }
            "--manager-task-add" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.manager_task_add = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--manager-task-done" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.manager_task_close = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--manager-task-body" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.manager_task_body = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--manager-task-file" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.manager_task_file = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--manager-task-priority" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.manager_task_priority = Some(value.parse::<i64>().unwrap_or(100));
                    idx += 2;
                    continue;
                }
            }
            "--agent" => {
                if let Some(value) = args.get(idx + 1) {
                    let agent_id = value.parse::<i64>().unwrap_or(-1);
                    parsed.message_target_agent_id = Some(agent_id);
                    idx += 2;
                    continue;
                }
            }
            "--to-agent" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.message_target_agent_id = Some(value.parse::<i64>().unwrap_or(-1));
                    idx += 2;
                    continue;
                }
            }
            "--to-agent-id" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.message_target_agent_uid = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--from-agent" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.message_sender_agent_id = Some(value.parse::<i64>().unwrap_or(-1));
                    idx += 2;
                    continue;
                }
            }
            "--from-agent-id" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.message_sender_agent_uid = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--message" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.message_body = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--submit" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.submit_body = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--message-kind" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.message_kind = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--topic" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.message_topic = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--broadcast" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.message_target_agent_id = Some(-1);
                    parsed.message_body = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--feedback" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.feedback = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--feedback-kind" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.feedback_kind = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--context" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.feedback_context = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--branch" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.branch = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--base-revision" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.base_revision = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--workspace-revision" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.workspace_revision = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            "--repository-revision" => {
                if let Some(value) = args.get(idx + 1) {
                    parsed.repository_revision = Some(value.clone());
                    idx += 2;
                    continue;
                }
            }
            value if value.starts_with("--") => {
                idx += 1;
                continue;
            }
            value => {
                if parsed.project_root.is_none() {
                    parsed.project_root = Some(value.to_string());
                }
                idx += 1;
                continue;
            }
        }

        idx += 1;
    }

    parsed
}

pub(super) fn resolve_mode(_cli: &CliArgs) -> GovernanceMode {
    GovernanceMode::Strict
}

pub(super) fn detect_project_root(cli_root: Option<&str>) -> PathBuf {
    let start = cli_root
        .and_then(resolve_cli_root_candidate)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let start = fs::canonicalize(&start).unwrap_or(start);

    discover_root_with_marker(&start).unwrap_or(start)
}

pub(super) fn resolve_cli_root_candidate(raw_root: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(raw_root);
    let resolved = if candidate.is_absolute() {
        candidate
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(candidate)
    };

    match fs::metadata(&resolved) {
        Ok(meta) if meta.is_dir() => Some(resolved),
        _ => None,
    }
}

pub(super) fn discover_root_with_marker(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);

    while let Some(path) = current {
        if path.join("ant_delirium_config/type_1").is_dir()
            || path
                .join("ant_delirium_config/default_hierarchy.toml")
                .is_file()
            || path.join("delirium.toml").is_file()
        {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }

    None
}

pub(super) fn load_runtime_hierarchy(
    project_root: &Path,
    explicit_config_path: Option<&str>,
) -> (HierarchyConfig, String) {
    if let Some(config_path) = explicit_config_path {
        let resolved = resolve_cli_path(project_root, config_path);
        return (
            load_hierarchy_from_path(&resolved),
            hierarchy_hint_path(project_root, &resolved),
        );
    }

    let type_1 = project_root.join("ant_delirium_config/type_1");
    if type_1.is_dir() {
        return (
            HierarchyConfig::load_from_folder(&type_1.to_string_lossy()),
            hierarchy_hint_path(project_root, &type_1),
        );
    }

    let default_hierarchy = project_root.join("ant_delirium_config/default_hierarchy.toml");
    if default_hierarchy.is_file() {
        let config_dir = project_root.join("ant_delirium_config");
        return (
            load_hierarchy_from_path(&config_dir),
            hierarchy_hint_path(project_root, &config_dir),
        );
    }

    let delirium_toml = project_root.join("delirium.toml");
    if delirium_toml.is_file() {
        return (
            load_hierarchy_from_path(&delirium_toml),
            hierarchy_hint_path(project_root, &delirium_toml),
        );
    }

    let default_rules = default_rules_path();
    (
        HierarchyConfig::load_static(),
        hierarchy_hint_path(project_root, &default_rules),
    )
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
    let is_dir = fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false);
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

pub(super) fn refresh_runtime_governance_state(
    plugin: &PluginContext,
    config: &AuditConfig,
) -> io::Result<()> {
    let audit_result = SwarmEngine::audit(config);
    agent_runtime::refresh_governance_from_audit(plugin, &audit_result)?;
    let _ = agent_runtime::assign_task_to_current_agent_if_missing(plugin)?;
    Ok(())
}

pub(super) fn is_repository_root_plugin(plugin: &PluginContext) -> bool {
    canonical_or_original(&plugin.workspace_path) == canonical_or_original(&plugin.repository_path)
}

pub(super) fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
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

pub(super) fn runtime_binary_display(args: &[String]) -> String {
    let raw = env::var("ANT_DELIRIUM_BINARY_DISPLAY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| args.first().cloned())
        .unwrap_or_else(|| "ac_ant_delirium".to_string());
    shell_quote_command(raw)
}

pub(super) fn shell_quote_command(raw: String) -> String {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.contains(char::is_whitespace) {
        format!("\"{}\"", normalized.replace('"', "\\\""))
    } else {
        normalized
    }
}

pub(super) fn render_version_output(binary_name: &str, json_requested: bool) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let git_hash = option_env!("ANT_DELIRIUM_BUILD_GIT_HASH").unwrap_or("unknown");
    let build_unix = option_env!("ANT_DELIRIUM_BUILD_UNIX").unwrap_or("unknown");

    if json_requested {
        return format!(
            "{{\n  \"command\": \"version\",\n  \"binary\": \"{}\",\n  \"version\": \"{}\",\n  \"git_hash\": \"{}\",\n  \"build_unix\": \"{}\"\n}}\n",
            json_escape(binary_name),
            json_escape(version),
            json_escape(git_hash),
            json_escape(build_unix),
        );
    }

    format!(
        "AC Ant-Delirium\n\nBinary: {}\nVersion: {}\nGit Hash: {}\nBuild Unix: {}\n",
        binary_name, version, git_hash, build_unix
    )
}

pub(super) fn render_config_dump(
    project_root: &Path,
    hierarchy: &HierarchyConfig,
    hierarchy_hint_path: &str,
    plugin_context: Option<&PluginContext>,
    binary_name: &str,
    json_requested: bool,
) -> String {
    let api_root = detect_api_root(hierarchy);
    let execution_context = plugin_context
        .map(|plugin| {
            if plugin.is_worktree {
                "git worktree"
            } else {
                "repository root"
            }
        })
        .unwrap_or("standalone");
    let node_count = hierarchy.nodes.len();

    if json_requested {
        let plugin_json = match plugin_context {
            Some(plugin) => format!(
                "  \"plugin\": {{\n    \"detected\": true,\n    \"plugin_dir\": \"{}\",\n    \"config_path\": \"{}\",\n    \"repository_path\": \"{}\",\n    \"workspace_path\": \"{}\",\n    \"execution_context\": \"{}\"\n  }},\n",
                json_escape(&plugin.plugin_dir.to_string_lossy()),
                json_escape(&plugin.config_path.to_string_lossy()),
                json_escape(&plugin.repository_path.to_string_lossy()),
                json_escape(&plugin.workspace_path.to_string_lossy()),
                json_escape(execution_context),
            ),
            None => "  \"plugin\": {\n    \"detected\": false\n  },\n".to_string(),
        };
        return format!(
            "{{\n  \"command\": \"config_dump\",\n  \"binary\": \"{}\",\n  \"project_root\": \"{}\",\n  \"hierarchy_hint_path\": \"{}\",\n  \"api_root\": \"{}\",\n  \"node_count\": {},\n{}  \"help_command\": \"{} --api {}.help\"\n}}\n",
            json_escape(binary_name),
            json_escape(&project_root.to_string_lossy()),
            json_escape(hierarchy_hint_path),
            json_escape(&api_root),
            node_count,
            plugin_json,
            json_escape(binary_name),
            json_escape(&api_root),
        );
    }

    let mut out = String::new();
    out.push_str("Configuration Dump\n\n");
    let _ = writeln!(out, "Project root: {}", project_root.to_string_lossy());
    let _ = writeln!(out, "Hierarchy hint: {}", hierarchy_hint_path);
    let _ = writeln!(out, "API root: {}", api_root);
    let _ = writeln!(out, "Loaded nodes: {}", node_count);
    if let Some(plugin) = plugin_context {
        out.push('\n');
        out.push_str("Plugin context:\n");
        let _ = writeln!(out, "Plugin dir: {}", plugin.plugin_dir.to_string_lossy());
        let _ = writeln!(out, "Config path: {}", plugin.config_path.to_string_lossy());
        let _ = writeln!(out, "Repository path: {}", plugin.repository_path.to_string_lossy());
        let _ = writeln!(out, "Workspace path: {}", plugin.workspace_path.to_string_lossy());
        let _ = writeln!(out, "Execution context: {}", execution_context);
    }
    out.push('\n');
    let _ = writeln!(out, "Useful commands:");
    let _ = writeln!(out, "- {} --api {}", binary_name, api_root);
    let _ = writeln!(out, "- {} --api {}.help", binary_name, api_root);
    let _ = writeln!(out, "- {} swarm status", binary_name);
    out
}

pub(super) fn render_search_output(path: &str, body: &str, json_requested: bool) -> String {
    if json_requested {
        format!(
            "{{\n  \"command\": \"search\",\n  \"path\": \"{}\",\n  \"found\": true,\n  \"value\": \"{}\"\n}}\n",
            json_escape(path),
            json_escape(body.trim()),
        )
    } else {
        format!(
            "\n[AQ - SEARCH RESULT]\nPath: {}\n---------------------------------------------\n{}\n---------------------------------------------",
            path, body
        )
    }
}

pub(super) fn render_search_not_found(path: &str, json_requested: bool) -> String {
    if json_requested {
        format!(
            "{{\n  \"command\": \"search\",\n  \"path\": \"{}\",\n  \"found\": false,\n  \"message\": \"Path or Parameter not found\"\n}}\n",
            json_escape(path),
        )
    } else {
        format!("\n[!] Path or Parameter not found: {}", path)
    }
}

pub(super) fn wrap_text_output(command: &str, body: &str, json_requested: bool) -> String {
    if json_requested {
        format!(
            "{{\n  \"command\": \"{}\",\n  \"body\": \"{}\"\n}}\n",
            json_escape(command),
            json_escape(body.trim()),
        )
    } else {
        body.to_string()
    }
}

pub(super) fn detect_api_root(hierarchy: &HierarchyConfig) -> String {
    for preferred_root in ["hierarchy", "rules", "regras"] {
        let prefix = format!("{}.", preferred_root);
        if hierarchy
            .nodes
            .keys()
            .any(|key| key == preferred_root || key.starts_with(&prefix))
        {
            return preferred_root.to_string();
        }
    }

    hierarchy
        .nodes
        .keys()
        .filter(|key| !key.is_empty())
        .filter_map(|key| key.split('.').next())
        .next()
        .unwrap_or("hierarchy")
        .to_string()
}

pub(super) fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
