use crate::governance;
use crate::rules_logs::messages;
use crate::structs::{AuditContext, AuditViolation};

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if !ctx.file_path.ends_with("Cargo.toml") {
        return;
    }

    let resolution = match ctx.require_named_global_axiom_parameter("dependency_purification", "libres") {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "dependency_purification", message);
            return;
        }
    };

    let allowed = parse_string_array(&resolution.value);
    if allowed.is_empty() {
        return;
    }

    let default_features = resolve_default_features(ctx.source);
    let dependency_entries = collect_dependency_entries(ctx.source);

    for entry in dependency_entries {
        if entry.name.is_empty() {
            continue;
        }
        if !entry.value.contains("optional = true") {
            continue;
        }
        if allowed.iter().any(|item| item == &entry.name) {
            continue;
        }
        let force_activated = default_features.iter().any(|feature| {
            feature == &entry.name
                || feature == &format!("dep:{}", entry.name)
                || feature.starts_with(&format!("{}/", entry.name))
        });
        if !force_activated {
            continue;
        }

        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: entry.line,
            rule_id: governance::rule_id(&resolution.identity_path, &entry.name),
            message: format!(
                "{}: optional crate '{}' is force-activated outside the authorized policy in {}.",
                messages::IMPURE_ACTIVATION,
                entry.name,
                resolution.resolved_path
            ),
            snippet: Some(entry.raw.clone()),
            quick_fix: Some(format!(
                "# [SANITIZE] Remove '{}' from default features or promote it into the authorized dependency policy.",
                entry.name
            )),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: resolution.resolved_path.clone(),
        });
    }
}

fn parse_string_array(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed[1..trimmed.len() - 1]
            .split(',')
            .map(|value| value.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|value| !value.is_empty())
            .collect()
    } else {
        vec![trimmed.to_string()]
    }
}

fn resolve_default_features(source: &str) -> Vec<String> {
    let mut active = Vec::new();
    let mut in_features = false;

    for raw_line in source.lines() {
        let stripped = strip_inline_comment(raw_line);
        let line = stripped.trim();
        if line == "[features]" {
            in_features = true;
            continue;
        }
        if line.starts_with('[') {
            in_features = false;
            continue;
        }
        if !in_features || !line.starts_with("default") {
            continue;
        }
        let (_, value) = match line.split_once('=') {
            Some(parts) => parts,
            None => continue,
        };
        active.extend(parse_string_array(value));
    }

    active
}

#[derive(Debug, Clone)]
struct DependencyEntry {
    line: usize,
    name: String,
    value: String,
    raw: String,
}

fn collect_dependency_entries(source: &str) -> Vec<DependencyEntry> {
    let mut entries = Vec::new();
    let mut in_deps = false;
    let mut buffer = String::new();
    let mut buffer_line = 0usize;

    for (idx, raw_line) in source.lines().enumerate() {
        let stripped = strip_inline_comment(raw_line);
        let line = stripped.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_deps = is_active_dependency_section(line);
            continue;
        }

        if !in_deps {
            continue;
        }

        if buffer.is_empty() {
            buffer_line = idx + 1;
        }

        if !buffer.is_empty() {
            buffer.push(' ');
        }
        buffer.push_str(line);

        if line.contains('{') && !line.contains('}') {
            continue;
        }

        if let Some(entry) = parse_dependency_entry(buffer_line, &buffer) {
            entries.push(entry);
        }
        buffer.clear();
        buffer_line = 0;
    }

    entries
}

fn parse_dependency_entry(line: usize, raw: &str) -> Option<DependencyEntry> {
    let (name, value) = raw.split_once('=')?;
    let name = name.trim().trim_matches('"').trim_matches('\'').to_string();
    let value = value.trim().to_string();
    Some(DependencyEntry {
        line,
        name,
        value,
        raw: raw.to_string(),
    })
}

fn strip_inline_comment(line: &str) -> String {
    match line.find('#') {
        Some(pos) => line[..pos].trim().to_string(),
        None => line.trim().to_string(),
    }
}

fn is_active_dependency_section(header: &str) -> bool {
    header.contains("dependencies")
        && !header.contains("dev-dependencies")
        && !header.contains("build-dependencies")
}

fn push_resolution_violation(ctx: &mut AuditContext<'_>, rule_suffix: &str, message: String) {
    let identity_path = ctx.resolve_axiom_identity_path(rule_suffix)
        .unwrap_or_else(|_| format!("axiom.{}", rule_suffix));
    ctx.violations.push(AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: format!("{}.resolution", identity_path),
        message,
        snippet: None,
        quick_fix: None,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: identity_path,
    });
}
