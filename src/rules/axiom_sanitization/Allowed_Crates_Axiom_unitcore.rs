use crate::governance;
use crate::rules_logs::messages;
use crate::structs::{AuditContext, AuditViolation};

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if !ctx.file_path.ends_with("Cargo.toml") {
        return;
    }

    let resolution = match ctx.require_named_global_axiom_parameter("allowed_crates", "libres") {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "allowed_crates", message);
            return;
        }
    };

    let allowed = parse_string_array(&resolution.value);
    if allowed.is_empty() {
        return;
    }

    let dependency_entries = collect_dependency_entries(ctx.source);
    for entry in dependency_entries {
        if entry.name.is_empty() {
            continue;
        }
        if is_local_dependency(&entry.value) {
            continue;
        }
        if is_intrinsic_crate(&entry.name) {
            continue;
        }
        if allowed.iter().any(|item| item == &entry.name) {
            continue;
        }
        if entry.value.contains("optional = true") {
            continue;
        }

        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: entry.line,
            rule_id: governance::rule_id(&resolution.identity_path, &entry.name),
            message: format!(
                "{}: crate '{}' is not authorized by {}.",
                messages::DEPS_UNAUTHORIZED,
                entry.name,
                resolution.resolved_path
            ),
            snippet: Some(entry.raw.clone()),
            quick_fix: Some(format!(
                "# [SANITIZE] Add '{}' to the allowed crate policy or remove it from Cargo.toml.",
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
            .map(|value| {
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string()
            })
            .filter(|value| !value.is_empty())
            .collect()
    } else {
        vec![trimmed.to_string()]
    }
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

fn is_intrinsic_crate(name: &str) -> bool {
    matches!(name, "std" | "core" | "alloc")
}

fn is_local_dependency(value: &str) -> bool {
    value.contains("path =")
}

fn push_resolution_violation(ctx: &mut AuditContext<'_>, rule_suffix: &str, message: String) {
    let identity_path = ctx
        .resolve_axiom_identity_path(rule_suffix)
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
