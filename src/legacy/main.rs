use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use ac_ant_delirium::api::{default_rules_path, InteractiveApi};
use ac_ant_delirium::Logs::GovernanceLogs;
use ac_ant_delirium::restricted_tokens::out::elite_out;
use ac_ant_delirium::structs::{AuditConfig, GovernanceMode, HierarchyConfig};
use ac_ant_delirium::engine::Manifest_Engine_unit::ManifestEngine;
use ac_ant_delirium::engine::Swarm_Engine_unit::SwarmEngine;

/// [WHAT] Entry point focado na revelação ativa de um manifesto e auditoria de governança.
fn main() {
    let args: Vec<String> = env::args().collect();
    let cli = parse_cli_args(&args[1..]);
    let project_root = detect_project_root(cli.project_root.as_deref());
    let mode = resolve_mode(&cli);

    let runtime_context = agent_runtime::discover_runtime_context(
        &project_root,
        cli.config_path.as_deref(),
        args.first().map(String::as_str),
    );
    let plugin_context = runtime_context.plugin.clone();

    if let Some(plugin) = &plugin_context {
        if let Err(err) = agent_runtime::sync_agent_registration(plugin) {
            elite_out(&format!("[!] failed to sync agent registration: {}", err));
        }
    }

    if cli.get_agents {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::render_agents_report(plugin) {
                Ok(report) => elite_out(&report),
                Err(err) => elite_out(&format!("[!] failed to read agents: {}", err)),
            },
            None => elite_out("[!] no plugin context detected for --getagents"),
        }
        return;
    }

    if let Some((agent_id, body)) = cli.message.as_ref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::append_message(plugin, *agent_id, body) {
                Ok(message) => elite_out(&message),
                Err(err) => elite_out(&format!("[!] failed to store message: {}", err)),
            },
            None => elite_out("[!] no plugin context detected for --message"),
        }
        return;
    }

    if let Some(body) = cli.feedback.as_deref() {
        match plugin_context.as_ref() {
            Some(plugin) => match agent_runtime::append_feedback(plugin, body) {
                Ok(message) => elite_out(&message),
                Err(err) => elite_out(&format!("[!] failed to store feedback: {}", err)),
            },
            None => elite_out("[!] no plugin context detected for --feedback"),
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

    if cli.api_requested {
        let info_overrides = plugin_context
            .as_ref()
            .and_then(|plugin| {
                agent_runtime::build_api_overrides(plugin, &hierarchy, &runtime_binary_display(&args))
                    .ok()
            })
            .unwrap_or_default();
        let api = InteractiveApi::new(
            hierarchy,
            project_root,
            hierarchy_hint_path,
            runtime_binary_display(&args),
        )
        .with_info_overrides(info_overrides);
        elite_out(&api.render(cli.api_query.as_deref()));
        return;
    }

    // [COMMAND: --search]
    if let Some(path) = cli.search_path.as_deref() {
        if let Some(result) = hierarchy.search_params(path) {
            elite_out(&format!("\n[AQ - SEARCH RESULT]\nPath: {}\n---------------------------------------------\n{}\n---------------------------------------------", path, result));
        } else {
            elite_out(&format!("\n[!] Path or Parameter not found: {}", path));
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

    let audit_result = SwarmEngine::audit(&config);
    let _ = ManifestEngine::generate(&config);
    let artifact_paths = GovernanceLogs::write_artifacts(
        &audit_result,
        &config,
        GovernanceLogs::DEFAULT_OUTPUT_ROOT,
    ).ok();

    GovernanceLogs::emit_terminal_report(&audit_result, &config, artifact_paths.as_ref());

    if !audit_result.violations.is_empty() {
        std::process::exit(1);
    }
}

#[derive(Default)]
struct CliArgs {
    project_root: Option<String>,
    config_path: Option<String>,
    api_query: Option<String>,
    api_requested: bool,
    search_path: Option<String>,
    get_agents: bool,
    message: Option<(i64, String)>,
    feedback: Option<String>,
}

fn parse_cli_args(args: &[String]) -> CliArgs {
    let mut parsed = CliArgs::default();
    let mut idx = 0;

    while idx < args.len() {
        match args[idx].as_str() {
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
                idx += 1;
                continue;
            }
            "--agent" => {
                if let Some(value) = args.get(idx + 1) {
                    let agent_id = value.parse::<i64>().unwrap_or(-1);
                    if let Some((_, body)) = parsed.message.take() {
                        parsed.message = Some((agent_id, body));
                    } else {
                        parsed.message = Some((agent_id, String::new()));
                    }
                    idx += 2;
                    continue;
                }
            }
            "--message" => {
                if let Some(value) = args.get(idx + 1) {
                    let agent_id = parsed.message.as_ref().map(|(id, _)| *id).unwrap_or(-1);
                    parsed.message = Some((agent_id, value.clone()));
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

fn resolve_mode(cli: &CliArgs) -> GovernanceMode {
    let _ = cli;
    GovernanceMode::Strict
}

fn detect_project_root(cli_root: Option<&str>) -> PathBuf {
    let start = cli_root
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let start = fs::canonicalize(&start).unwrap_or(start);

    discover_root_with_marker(&start)
        .unwrap_or(start)
}

fn discover_root_with_marker(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);

    while let Some(path) = current {
        if path.join("ant_delirium_config/type_1").is_dir() || path.join("delirium.toml").is_file() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }

    None
}

fn load_runtime_hierarchy(project_root: &Path, explicit_config_path: Option<&str>) -> (HierarchyConfig, String) {
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

fn resolve_cli_path(project_root: &Path, raw_path: &str) -> PathBuf {
    let candidate = Path::new(raw_path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    }
}

fn load_hierarchy_from_path(path: &Path) -> HierarchyConfig {
    let is_dir = fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false);
    if is_dir {
        HierarchyConfig::load_from_folder(&path.to_string_lossy())
    } else {
        let input = fs::read_to_string(path).unwrap_or_default();
        HierarchyConfig::parse_brute(&input)
    }
}

fn hierarchy_hint_path(project_root: &Path, source: &Path) -> String {
    let hint = if source.is_dir() {
        let default_hierarchy = source.join("default_hierarchy.toml");
        if default_hierarchy.is_file() {
            default_hierarchy
        } else {
            source.to_path_buf()
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

fn runtime_binary_display(args: &[String]) -> String {
    args.first()
        .map(PathBuf::from)
        .and_then(|path| path.file_name().map(|value| value.to_string_lossy().to_string()))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "ac_ant_delirium".to_string())
}
