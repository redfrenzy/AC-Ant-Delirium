use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ant_delirium::structs::HierarchyConfig;

#[derive(Debug, Clone)]
pub struct PluginContext {
    pub plugin_dir: PathBuf,
    pub config_path: PathBuf,
    pub repository_path: PathBuf,
    pub workspace_path: PathBuf,
    pub is_worktree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedAgent {
    pub internal_id: i64,
    pub worktree_path: String,
    pub assigned_mission: String,
    pub last_ping: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceTask {
    pub file_path: String,
    pub violation_count: i64,
    pub owner_id: i64,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeCommandContext {
    pub plugin: Option<PluginContext>,
    pub hierarchy: Option<HierarchyConfig>,
    pub hierarchy_hint_path: Option<String>,
}

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
    ensure_support_files(plugin)?;
    write_context_snapshot(plugin)?;

    if !plugin.is_worktree {
        return Ok(None);
    }

    let mut agents = read_connected_agents(&plugin.connected_path())?;
    let mut tasks = read_governance_tasks(&plugin.governance_path())?;
    let now = unix_timestamp();

    if let Some(existing_idx) = agents.iter().position(|agent| {
        normalize_path_str(&agent.worktree_path) == normalize_path(plugin.workspace_path.as_path())
    }) {
        let existing = &mut agents[existing_idx];
        existing.last_ping = now;
        if existing.assigned_mission.trim().is_empty() {
            if let Some(task) = tasks.iter().find(|task| task.owner_id == existing.internal_id) {
                existing.assigned_mission = task.file_path.clone();
            }
        }
        let existing_snapshot = existing.clone();
        write_connected_agents(&plugin.connected_path(), &agents)?;
        return Ok(Some(existing_snapshot));
    }

    let next_id = agents.iter().map(|agent| agent.internal_id).max().unwrap_or(-1) + 1;
    let assigned_mission = assign_governance_task(&mut tasks, next_id).unwrap_or_default();
    let new_agent = ConnectedAgent {
        internal_id: next_id,
        worktree_path: plugin.workspace_path.to_string_lossy().to_string(),
        assigned_mission,
        last_ping: now,
    };
    agents.push(new_agent.clone());

    write_connected_agents(&plugin.connected_path(), &agents)?;
    write_governance_tasks(&plugin.governance_path(), &tasks)?;
    Ok(Some(new_agent))
}

pub fn render_agents_report(plugin: &PluginContext) -> io::Result<String> {
    ensure_support_files(plugin)?;
    let agents = read_connected_agents(&plugin.connected_path())?;
    let tasks = read_governance_tasks(&plugin.governance_path())?;

    let mut out = String::new();
    out.push_str("Active agents:\n");
    if agents.is_empty() {
        out.push_str("- none\n");
        return Ok(out);
    }

    let now = unix_timestamp();
    for agent in agents {
        let owned: Vec<&GovernanceTask> = tasks
            .iter()
            .filter(|task| task.owner_id == agent.internal_id)
            .collect();
        let violations: i64 = owned.iter().map(|task| task.violation_count).sum();
        let status = if now - agent.last_ping <= 3600 {
            "active"
        } else {
            "stale"
        };
        out.push_str(&format!(
            "- agent {} [{}] workspace={} files={} violations={} mission={}\n",
            agent.internal_id,
            status,
            agent.worktree_path,
            owned.len(),
            violations,
            empty_or_dash(&agent.assigned_mission),
        ));
    }

    Ok(out)
}

pub fn append_message(plugin: &PluginContext, agent_id: i64, body: &str) -> io::Result<String> {
    ensure_support_files(plugin)?;
    let mut entries = read_message_entries(&plugin.messages_path(), "messages")?;
    entries.push(JsonEntry {
        fields: vec![
            ("target_agent_id".to_string(), JsonValue::Number(agent_id)),
            ("body".to_string(), JsonValue::String(body.trim().to_string())),
            (
                "workspace_path".to_string(),
                JsonValue::String(plugin.workspace_path.to_string_lossy().to_string()),
            ),
            ("created_at".to_string(), JsonValue::Number(unix_timestamp())),
        ],
    });
    write_entry_collection(&plugin.messages_path(), "messages", &entries)?;
    Ok(format!("message stored for agent {}", agent_id))
}

pub fn append_feedback(plugin: &PluginContext, body: &str) -> io::Result<String> {
    ensure_support_files(plugin)?;
    let mut entries = read_message_entries(&plugin.feedback_path(), "entries")?;
    entries.push(JsonEntry {
        fields: vec![
            ("body".to_string(), JsonValue::String(body.trim().to_string())),
            (
                "workspace_path".to_string(),
                JsonValue::String(plugin.workspace_path.to_string_lossy().to_string()),
            ),
            ("created_at".to_string(), JsonValue::Number(unix_timestamp())),
        ],
    });
    write_entry_collection(&plugin.feedback_path(), "entries", &entries)?;
    Ok("feedback stored".to_string())
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
    let current_agent = agents.iter().find(|agent| {
        normalize_path_str(&agent.worktree_path) == normalize_path(plugin.workspace_path.as_path())
    });
    let active_agents = agents
        .iter()
        .filter(|agent| unix_timestamp() - agent.last_ping <= 3600)
        .count();
    let unassigned_tasks = tasks.iter().filter(|task| task.owner_id < 0).count();
    let top_unassigned = tasks
        .iter()
        .filter(|task| task.owner_id < 0)
        .max_by_key(|task| task.violation_count);

    let mut installation = String::new();
    installation.push_str("Multi-agent execution panel.\n\n");
    installation.push_str(&format!(
        "Plugin path: {}\nRepository path: {}\nWorkspace path: {}\nExecution context: {}\n",
        plugin.plugin_dir.to_string_lossy(),
        plugin.repository_path.to_string_lossy(),
        plugin.workspace_path.to_string_lossy(),
        if plugin.is_worktree { "git worktree" } else { "repository root" }
    ));
    installation.push('\n');
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
    if let Some(task) = top_unassigned {
        installation.push_str(&format!(
            "Highest unassigned task: {} ({} violations)\n",
            task.file_path, task.violation_count
        ));
    }
    installation.push('\n');
    installation.push_str("Operational commands:\n");
    installation.push_str(&format!("- {} --getagents\n", binary_name));
    installation.push_str(&format!("- {} --message \"...\" --agent <id>\n", binary_name));
    installation.push_str(&format!("- {} --feedback \"...\"\n", binary_name));
    installation.push_str(&format!("- {} --api {}.installation.context\n", binary_name, root));
    installation.push_str(&format!("- {} --api {}.installation.agents\n", binary_name, root));
    installation.push_str(&format!("- {} --api {}.installation.messages\n", binary_name, root));
    installation.push_str(&format!("- {} --api {}.installation.feedback\n", binary_name, root));

    let context = format!(
        "Context artifact: {}\nRepository root: {}\nActive workspace: {}\nPlugin directory: {}\n",
        plugin.context_path().to_string_lossy(),
        plugin.repository_path.to_string_lossy(),
        plugin.workspace_path.to_string_lossy(),
        plugin.plugin_dir.to_string_lossy(),
    );

    let agents_doc = render_agents_report(plugin)?;
    let messages = format!(
        "Messages artifact: {}\nUse {} --message \"...\" --agent <id> to append a directed instruction.\n",
        plugin.messages_path().to_string_lossy(),
        binary_name,
    );
    let feedback = format!(
        "Feedback artifact: {}\nUse {} --feedback \"...\" to append human guidance for the system.\n",
        plugin.feedback_path().to_string_lossy(),
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
    let mut best_match: Option<(usize, PluginContext)> = None;
    for plugins_root in candidate_plugin_roots(project_root, argv0) {
        let Ok(entries) = fs::read_dir(&plugins_root) else {
            continue;
        };

        for entry in entries.filter_map(|entry| entry.ok()) {
            let plugin_dir = entry.path();
            let config_path = plugin_dir.join("delirium.toml");
            if !config_path.is_file() {
                continue;
            }

            let Some(repository_path) = extract_repository_path(&config_path) else {
                continue;
            };

            let score = match_plugin_context(project_root, &plugin_dir, &repository_path);
            if score == 0 {
                continue;
            }

            let context = PluginContext {
                plugin_dir,
                config_path,
                repository_path: repository_path.clone(),
                workspace_path: project_root.to_path_buf(),
                is_worktree: normalize_path(project_root) != normalize_path(&repository_path),
            };

            match &best_match {
                Some((best_score, _)) if *best_score >= score => {}
                _ => best_match = Some((score, context)),
            }
        }
    }

    best_match.map(|(_, context)| context)
}

fn build_plugin_context_from_config(project_root: &Path, config_path: &Path) -> Option<PluginContext> {
    if config_path.file_name().and_then(|value| value.to_str()) != Some("delirium.toml") {
        return None;
    }
    let plugin_dir = config_path.parent()?.to_path_buf();
    let repository_path = extract_repository_path(config_path)?;
    Some(PluginContext {
        plugin_dir,
        config_path: config_path.to_path_buf(),
        repository_path: repository_path.clone(),
        workspace_path: project_root.to_path_buf(),
        is_worktree: normalize_path(project_root) != normalize_path(&repository_path),
    })
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

    let workspace_name = project_root.file_name().and_then(|value| value.to_str()).unwrap_or("");
    let repository_name = repository_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let plugin_name = plugin_dir.file_name().and_then(|value| value.to_str()).unwrap_or("");

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
    let input = fs::read_to_string(config_path).ok()?;
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

fn ensure_support_files(plugin: &PluginContext) -> io::Result<()> {
    if !plugin.connected_path().exists() {
        write_connected_agents(&plugin.connected_path(), &[])?;
    }
    if !plugin.governance_path().exists() {
        write_governance_tasks(&plugin.governance_path(), &[])?;
    }
    if !plugin.messages_path().exists() {
        write_entry_collection(&plugin.messages_path(), "messages", &[])?;
    }
    if !plugin.feedback_path().exists() {
        write_entry_collection(&plugin.feedback_path(), "entries", &[])?;
    }
    Ok(())
}

fn write_context_snapshot(plugin: &PluginContext) -> io::Result<()> {
    let output = format!(
        "{{\n  \"repository_path\": \"{}\",\n  \"active_workspace_path\": \"{}\",\n  \"plugin_path\": \"{}\"\n}}\n",
        json_escape(&plugin.repository_path.to_string_lossy()),
        json_escape(&plugin.workspace_path.to_string_lossy()),
        json_escape(&plugin.plugin_dir.to_string_lossy()),
    );
    fs::write(plugin.context_path(), output)
}

fn read_connected_agents(path: &Path) -> io::Result<Vec<ConnectedAgent>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, "agents");
    Ok(objects
        .into_iter()
        .map(|object| ConnectedAgent {
            internal_id: extract_number_field(&object, "internal_id").unwrap_or(-1),
            worktree_path: extract_string_field(&object, "worktree_path").unwrap_or_default(),
            assigned_mission: extract_string_field(&object, "assigned_mission").unwrap_or_default(),
            last_ping: extract_number_field(&object, "last_ping").unwrap_or_default(),
        })
        .collect())
}

fn write_connected_agents(path: &Path, agents: &[ConnectedAgent]) -> io::Result<()> {
    let mut out = String::new();
    out.push_str("{\n  \"agents\": [\n");
    for (idx, agent) in agents.iter().enumerate() {
        let comma = if idx + 1 < agents.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{\n      \"internal_id\": {},\n      \"worktree_path\": \"{}\",\n      \"assigned_mission\": \"{}\",\n      \"last_ping\": {}\n    }}{}\n",
            agent.internal_id,
            json_escape(&agent.worktree_path),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct JsonEntry {
    fields: Vec<(String, JsonValue)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonValue {
    String(String),
    Number(i64),
}

fn read_message_entries(path: &Path, key: &str) -> io::Result<Vec<JsonEntry>> {
    let raw = read_text_or_empty(path)?;
    let objects = parse_object_array(&raw, key);
    Ok(objects
        .into_iter()
        .map(|object| {
            let mut fields = Vec::new();
            for field in ["target_agent_id", "body", "workspace_path", "created_at"] {
                if let Some(value) = extract_string_field(&object, field) {
                    fields.push((field.to_string(), JsonValue::String(value)));
                    continue;
                }
                if let Some(value) = extract_number_field(&object, field) {
                    fields.push((field.to_string(), JsonValue::Number(value)));
                }
            }
            JsonEntry { fields }
        })
        .collect())
}

fn write_entry_collection(path: &Path, key: &str, entries: &[JsonEntry]) -> io::Result<()> {
    let mut out = String::new();
    out.push_str(&format!("{{\n  \"{}\": [\n", key));
    for (idx, entry) in entries.iter().enumerate() {
        let comma = if idx + 1 < entries.len() { "," } else { "" };
        out.push_str("    {\n");
        for (field_idx, (name, value)) in entry.fields.iter().enumerate() {
            let field_comma = if field_idx + 1 < entry.fields.len() { "," } else { "" };
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
            return Some(unescape_json_string(&raw[quote_start..quote_start + offset]));
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
    let is_dir = fs::metadata(path).map(|meta| meta.is_dir()).unwrap_or(false);
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

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if unique.iter().any(|seen: &PathBuf| normalize_path(seen) == normalize_path(&path)) {
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
    .any(|branch| hierarchy.nodes.contains_key(&format!("{}.{}", root, branch)))
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

impl PluginContext {
    pub fn connected_path(&self) -> PathBuf {
        self.plugin_dir.join("connected.json")
    }

    pub fn governance_path(&self) -> PathBuf {
        self.plugin_dir.join("governance.json")
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
                worktree_path: "/tmp/ws".to_string(),
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
}
