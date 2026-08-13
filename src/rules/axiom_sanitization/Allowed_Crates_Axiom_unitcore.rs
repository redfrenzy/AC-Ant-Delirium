use crate::governance;
use crate::rules_logs::messages;
use crate::structs::{AuditContext, AuditViolation};
use std::fs;
use std::path::Path;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let resolution = match ctx.require_named_global_axiom_parameter("allowed_crates", "libres") {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "allowed_crates", message);
            return;
        }
    };

    let allowed = parse_string_array(&resolution.value);
    if ctx.file_path.ends_with("Cargo.toml") {
        patrol_cargo_dependencies(
            ctx,
            &resolution.identity_path,
            &resolution.resolved_path,
            &allowed,
        );
        return;
    }
    if ctx.file_path.ends_with(".rs") {
        if should_run_manifest_pass_once(ctx) {
            patrol_project_cargo_manifests(
                ctx,
                &resolution.identity_path,
                &resolution.resolved_path,
                &allowed,
            );
        }
        patrol_external_usage_outside_libres(
            ctx,
            &resolution.identity_path,
            &resolution.resolved_path,
            &allowed,
        );
    }
}

fn should_run_manifest_pass_once(ctx: &AuditContext<'_>) -> bool {
    ctx.project
        .files
        .first()
        .map(|entry| entry.path == ctx.file_path)
        .unwrap_or(false)
}

fn patrol_project_cargo_manifests(
    ctx: &mut AuditContext<'_>,
    identity_path: &str,
    resolved_path: &str,
    allowed: &[String],
) {
    let root = ctx.config.project_root_path();
    let manifests = collect_project_manifests(&root);
    for manifest in manifests {
        let Ok(source) = fs::read_to_string(root.join(&manifest)) else {
            continue;
        };
        patrol_cargo_dependencies_for_manifest(
            ctx,
            identity_path,
            resolved_path,
            allowed,
            &manifest,
            &source,
        );
    }
}

fn patrol_cargo_dependencies(
    ctx: &mut AuditContext<'_>,
    identity_path: &str,
    resolved_path: &str,
    allowed: &[String],
) {
    patrol_cargo_dependencies_for_manifest(
        ctx,
        identity_path,
        resolved_path,
        allowed,
        ctx.file_path,
        ctx.source,
    );
}

fn patrol_cargo_dependencies_for_manifest(
    ctx: &mut AuditContext<'_>,
    identity_path: &str,
    resolved_path: &str,
    allowed: &[String],
    manifest_path: &str,
    source: &str,
) {
    let strict_empty_block = allowed.is_empty();
    let mut used_allowed_crates: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let allowed_normalized: std::collections::BTreeSet<String> = allowed
        .iter()
        .map(|item| normalize_crate_name(item))
        .collect();

    let dependency_entries = collect_dependency_entries(source);
    for entry in dependency_entries {
        if entry.name.is_empty() {
            continue;
        }
        let normalized_entry_name = normalize_crate_name(&entry.name);
        if allowed_normalized.contains(&normalized_entry_name) {
            used_allowed_crates.insert(normalized_entry_name.clone());
        }
        if is_intrinsic_crate(&entry.name) {
            continue;
        }
        if !strict_empty_block && allowed.iter().any(|item| item == &entry.name) {
            continue;
        }
        if entry.value.contains("optional = true") {
            continue;
        }

        let normalized_name = normalize_crate_name(&entry.name);
        ctx.violations.push(AuditViolation {
            file_path: manifest_path.to_string(),
            line: entry.line,
            rule_id: governance::rule_id(identity_path, &entry.name),
            message: format!(
                "{}: crate '{}' is not authorized by {} and must be wrapped via src/libres/{}_unit.rs before use.",
                messages::DEPS_UNAUTHORIZED,
                entry.name,
                resolved_path,
                normalized_name
            ),
            snippet: Some(entry.raw.clone()),
            quick_fix: Some(format!(
                "# [SANITIZE] Define '{}' in libres, create src/libres/{}_unit.rs as the wrapper boundary, and consume the crate only through that wrapper.",
                entry.name,
                normalized_name
            )),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: resolved_path.to_string(),
        });
    }

    if !allowed.is_empty() && ctx.project.loaded_file_count() > 0 {
        for crate_name in allowed.iter() {
            if crate_name.trim().is_empty() {
                continue;
            }
            let normalized_allowed_name = normalize_crate_name(crate_name);
            if !used_allowed_crates.contains(&normalized_allowed_name) {
                continue;
            }

            let wrapper_path = expected_wrapper_path_for_crate(manifest_path, crate_name);
            if has_libres_wrapper_for_crate(ctx, manifest_path, crate_name) {
                continue;
            }

            let normalized_name = normalize_crate_name(crate_name);
            ctx.violations.push(AuditViolation {
                file_path: manifest_path.to_string(),
                line: 1,
                rule_id: governance::rule_id(
                    identity_path,
                    &format!("{}_wrapper", normalized_name),
                ),
                message: format!(
                    "{}: crate '{}' requires at least one wrapper implementation under '{}', but none was found.",
                    messages::DEPS_UNAUTHORIZED,
                    crate_name,
                    libres_root_for_manifest(manifest_path)
                ),
                snippet: Some(wrapper_path.clone()),
                quick_fix: Some(format!(
                    "# [SANITIZE] Create a wrapper under '{}' for crate '{}'. Suggested canonical name: '{}'.",
                    libres_root_for_manifest(manifest_path),
                    crate_name,
                    wrapper_path
                )),
                detected_dna: ctx.detected_dna.clone(),
                taxonomy_path: resolved_path.to_string(),
            });
        }
    }
}

fn collect_project_manifests(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    collect_project_manifests_recursive(root, root, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_project_manifests_recursive(current: &Path, root: &Path, out: &mut Vec<String>) {
    if !current.starts_with(root) || !current.exists() {
        return;
    }
    if should_skip_manifest_walk(current, root) {
        return;
    }
    if current.is_file() {
        if current.file_name().and_then(|v| v.to_str()) == Some("Cargo.toml") {
            out.push(relative_path_string(current, root));
        }
        return;
    }
    if !current.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.filter_map(|value| value.ok()) {
        let path = entry.path();
        collect_project_manifests_recursive(&path, root, out);
    }
}

fn should_skip_manifest_walk(path: &Path, root: &Path) -> bool {
    let rel = path
        .strip_prefix(root)
        .ok()
        .map(|v| v.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    if rel.is_empty() {
        return false;
    }
    rel == ".git"
        || rel.starts_with(".git/")
        || rel == "target"
        || rel.starts_with("target/")
        || rel == "AC_Ant-Delirium_props"
        || rel.starts_with("AC_Ant-Delirium_props/")
        || rel == "src/Logs"
        || rel.starts_with("src/Logs/")
}

fn relative_path_string(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .map(|v| v.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
}

fn patrol_external_usage_outside_libres(
    ctx: &mut AuditContext<'_>,
    identity_path: &str,
    resolved_path: &str,
    allowed: &[String],
) {
    if is_inside_libres_wrapper(ctx.file_path) {
        return;
    }
    if allowed.is_empty() {
        return;
    }

    let mut seen = std::collections::HashSet::new();
    let candidates = collect_external_crate_candidates(ctx.source, allowed);
    for candidate in candidates {
        if is_intrinsic_crate(&candidate.name) || is_internal_scope_token(&candidate.name) {
            continue;
        }
        let normalized = normalize_crate_name(&candidate.name);
        if !allowed
            .iter()
            .map(|item| normalize_crate_name(item))
            .any(|item| item == normalized)
        {
            continue;
        }
        if !seen.insert(normalized.clone()) {
            continue;
        }

        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: candidate.line,
            rule_id: governance::rule_id(identity_path, &candidate.name),
            message: format!(
                "{}: external crate '{}' must be used only inside src/libres wrappers.",
                messages::DEPS_UNAUTHORIZED,
                candidate.name
            ),
            snippet: Some(candidate.raw.clone()),
            quick_fix: Some(format!(
                "# [SANITIZE] Move external crate usage to src/libres/{}_unit.rs and expose project-safe helper functions from that file.",
                normalized
            )),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: resolved_path.to_string(),
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
    let mut scoped_dependency_name: Option<String> = None;
    let mut scoped_dependency_line = 0usize;
    let mut scoped_dependency_body = String::new();

    for (idx, raw_line) in source.lines().enumerate() {
        let stripped = strip_inline_comment(raw_line);
        let line = stripped.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            if let Some(name) = scoped_dependency_name.take() {
                let raw = if scoped_dependency_body.trim().is_empty() {
                    format!("[dependencies.{name}]")
                } else {
                    format!("[dependencies.{name}] {}", scoped_dependency_body.trim())
                };
                entries.push(DependencyEntry {
                    line: scoped_dependency_line,
                    name,
                    value: scoped_dependency_body.trim().to_string(),
                    raw,
                });
                scoped_dependency_body.clear();
                scoped_dependency_line = 0;
            }
            in_deps = is_active_dependency_section(line);
            scoped_dependency_name = parse_scoped_dependency_header(line);
            if scoped_dependency_name.is_some() {
                scoped_dependency_line = idx + 1;
            }
            continue;
        }

        if let Some(name) = scoped_dependency_name.clone() {
            if !scoped_dependency_body.is_empty() {
                scoped_dependency_body.push(' ');
            }
            scoped_dependency_body.push_str(line);
            if line.contains("optional = true") {
                entries.push(DependencyEntry {
                    line: scoped_dependency_line.max(idx + 1),
                    name,
                    value: scoped_dependency_body.trim().to_string(),
                    raw: format!(
                        "[dependencies.{}] {}",
                        scoped_dependency_name.as_deref().unwrap_or("crate"),
                        scoped_dependency_body.trim()
                    ),
                });
            }
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

    if let Some(name) = scoped_dependency_name.take() {
        let raw = if scoped_dependency_body.trim().is_empty() {
            format!("[dependencies.{name}]")
        } else {
            format!("[dependencies.{name}] {}", scoped_dependency_body.trim())
        };
        entries.push(DependencyEntry {
            line: scoped_dependency_line.max(1),
            name,
            value: scoped_dependency_body.trim().to_string(),
            raw,
        });
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
    if let Some(name) = scoped_dependency_section_name(header) {
        return is_dependencies_section_name(&name);
    }
    false
}

fn parse_scoped_dependency_header(header: &str) -> Option<String> {
    let section = scoped_dependency_section_name(header)?;
    if section.starts_with("dependencies.") {
        return section
            .split_once('.')
            .map(|(_, value)| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }
    if section.starts_with("dev-dependencies.") {
        return section
            .split_once('.')
            .map(|(_, value)| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }
    if section.starts_with("build-dependencies.") {
        return section
            .split_once('.')
            .map(|(_, value)| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }
    if section.starts_with("workspace.dependencies.") {
        return section
            .split_once("workspace.dependencies.")
            .map(|(_, value)| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }
    if let Some(target_tail) = section.strip_prefix("target.") {
        if let Some((_, dep_tail)) = target_tail.split_once(".dependencies.") {
            return Some(dep_tail.trim().to_string()).filter(|value| !value.is_empty());
        }
        if let Some((_, dep_tail)) = target_tail.split_once(".dev-dependencies.") {
            return Some(dep_tail.trim().to_string()).filter(|value| !value.is_empty());
        }
        if let Some((_, dep_tail)) = target_tail.split_once(".build-dependencies.") {
            return Some(dep_tail.trim().to_string()).filter(|value| !value.is_empty());
        }
    }
    None
}

fn scoped_dependency_section_name(header: &str) -> Option<String> {
    if !header.starts_with('[') || !header.ends_with(']') {
        return None;
    }
    Some(header[1..header.len() - 1].trim().to_string())
}

fn is_dependencies_section_name(section: &str) -> bool {
    section == "dependencies"
        || section == "dev-dependencies"
        || section == "build-dependencies"
        || section == "workspace.dependencies"
        || section.starts_with("target.") && section.contains(".dependencies")
}

fn is_intrinsic_crate(name: &str) -> bool {
    matches!(name, "std" | "core" | "alloc")
}

#[derive(Debug, Clone)]
struct ExternalCrateCandidate {
    line: usize,
    name: String,
    raw: String,
}

fn collect_external_crate_candidates(
    source: &str,
    allowed: &[String],
) -> Vec<ExternalCrateCandidate> {
    let mut out = Vec::new();
    let allowed_aliases = resolve_allowed_aliases(allowed);
    for (idx, raw_line) in source.lines().enumerate() {
        let line = strip_inline_comment(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(name) = parse_use_root(trimmed) {
            out.push(ExternalCrateCandidate {
                line: idx + 1,
                name,
                raw: trimmed.to_string(),
            });
            continue;
        }
        if let Some(name) = parse_extern_crate(trimmed) {
            out.push(ExternalCrateCandidate {
                line: idx + 1,
                name,
                raw: trimmed.to_string(),
            });
            continue;
        }
        for alias in allowed_aliases.iter() {
            let token = format!("{}::", alias);
            if contains_path_token(trimmed, &token) {
                out.push(ExternalCrateCandidate {
                    line: idx + 1,
                    name: alias.clone(),
                    raw: trimmed.to_string(),
                });
                break;
            }
        }
    }
    out
}

fn resolve_allowed_aliases(allowed: &[String]) -> std::collections::BTreeSet<String> {
    let mut aliases = std::collections::BTreeSet::new();
    for item in allowed.iter() {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        aliases.insert(trimmed.to_string());
        aliases.insert(normalize_crate_name(trimmed));
    }
    aliases
}

fn contains_path_token(line: &str, token: &str) -> bool {
    let Some(pos) = line.find(token) else {
        return false;
    };
    if pos == 0 {
        return true;
    }
    let prev = line[..pos].chars().last().unwrap_or(' ');
    !(prev.is_ascii_alphanumeric() || prev == '_')
}

fn parse_use_root(line: &str) -> Option<String> {
    let remainder = if line.starts_with("use ") {
        line.trim_start_matches("use ")
    } else if line.starts_with("pub use ") {
        line.trim_start_matches("pub use ")
    } else {
        return None;
    };
    let remainder = remainder.trim();
    let root = remainder
        .split("::")
        .next()
        .map(|value| value.trim().trim_end_matches(';'))?;
    if root.is_empty() {
        return None;
    }
    Some(root.to_string())
}

fn parse_extern_crate(line: &str) -> Option<String> {
    if !line.starts_with("extern crate ") {
        return None;
    }
    let remainder = line.trim_start_matches("extern crate ").trim();
    let name = remainder
        .split_whitespace()
        .next()
        .map(|value| value.trim_end_matches(';'))?;
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn normalize_crate_name(name: &str) -> String {
    name.trim().replace('-', "_")
}

fn is_inside_libres_wrapper(file_path: &str) -> bool {
    file_path.starts_with("src/libres/")
        || file_path.contains("/src/libres/")
        || file_path == "src/libres"
}

fn is_internal_scope_token(name: &str) -> bool {
    matches!(name, "crate" | "self" | "super")
}

fn expected_wrapper_path_for_crate(cargo_toml_path: &str, crate_name: &str) -> String {
    let normalized_name = normalize_crate_name(crate_name);
    let package_root = std::path::Path::new(cargo_toml_path)
        .parent()
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    if package_root.is_empty() {
        format!("src/libres/{}_unit.rs", normalized_name)
    } else {
        format!("{}/src/libres/{}_unit.rs", package_root, normalized_name)
    }
}

fn libres_root_for_manifest(cargo_toml_path: &str) -> String {
    let manifest_path = std::path::Path::new(cargo_toml_path);
    let parent = manifest_path
        .parent()
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    if parent.is_empty() {
        "src/libres".to_string()
    } else {
        format!("{}/src/libres", parent.trim_end_matches('/'))
    }
}

fn has_libres_wrapper_for_crate(
    ctx: &AuditContext<'_>,
    manifest_path: &str,
    crate_name: &str,
) -> bool {
    let normalized = normalize_crate_name(crate_name);
    let libres_root = libres_root_for_manifest(manifest_path);

    for path in ctx.project.known_paths.iter() {
        if !path.starts_with(&libres_root) || !path.ends_with(".rs") {
            continue;
        }

        if let Some(file) = ctx.project.file(path) {
            let candidates =
                collect_external_crate_candidates(&file.content, &[crate_name.to_string()]);
            if candidates
                .iter()
                .any(|candidate| normalize_crate_name(&candidate.name) == normalized)
            {
                return true;
            }
        }
    }

    false
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
