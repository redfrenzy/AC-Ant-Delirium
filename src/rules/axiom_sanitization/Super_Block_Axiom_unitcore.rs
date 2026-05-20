use crate::governance;
use crate::structs::{AuditConfig, AuditViolation};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
#[cfg(unix)]
use std::process::Command;

#[derive(Debug, Default)]
pub struct SuperBlockPreflight {
    pub violations: Vec<AuditViolation>,
    pub abort_scan: bool,
}

pub fn run_preflight(config: &AuditConfig) -> SuperBlockPreflight {
    if !resolve_bool(config, "super_block.enabled", false) {
        return SuperBlockPreflight::default();
    }

    let taxonomy_path = "hierarchy.super_block".to_string();
    let guard_path = resolve_string(config, "super_block.guard_path");
    let guard_call = resolve_string(config, "super_block.guard_call");
    let guard_symbol = resolve_string(config, "super_block.guard_symbol")
        .unwrap_or_else(|| "ant_delirium_build_guard".to_string());
    let guard_owner =
        resolve_string(config, "super_block.guard_owner").unwrap_or_else(|| "root".to_string());
    let guard_readonly = resolve_bool(config, "super_block.guard_readonly", true);
    let guard_immutable = resolve_bool(
        config,
        "super_block.guard_immutable",
        owner_policy_requires_immutable(&guard_owner),
    );
    let enforced_files = resolve_string_array(config, "super_block.enforced_files");
    let enforced_owner =
        resolve_string(config, "super_block.enforced_owner").unwrap_or_else(|| "root".to_string());
    let enforced_readonly = resolve_bool(config, "super_block.enforced_readonly", true);
    let enforced_immutable =
        resolve_bool(
            config,
            "super_block.enforced_immutable",
            owner_policy_requires_immutable(&enforced_owner),
        );
    let watch_files = resolve_string_array(config, "super_block.watch_files");
    let abort_on_violation = resolve_bool(config, "super_block.abort_on_violation", true);

    let mut violations = Vec::new();
    let mut already_checked = HashSet::new();

    if let Some(guard_rel) = guard_path.as_deref() {
        let guard_display = normalize_display_path(guard_rel);
        if already_checked.insert(guard_display.clone()) {
            validate_protected_file(
                config,
                &mut violations,
                &taxonomy_path,
                &guard_display,
                &guard_owner,
                guard_readonly,
                guard_immutable,
                true,
                "guard",
            );
        }
    }

    for raw_file in &enforced_files {
        let normalized = normalize_display_path(raw_file);
        if already_checked.insert(normalized.clone()) {
            validate_protected_file(
                config,
                &mut violations,
                &taxonomy_path,
                &normalized,
                &enforced_owner,
                enforced_readonly,
                enforced_immutable,
                true,
                "enforced",
            );
        }

        if normalized.ends_with("build.rs") {
            validate_build_guard_integrity(
                config,
                &mut violations,
                &taxonomy_path,
                &normalized,
                &guard_symbol,
                guard_call
                    .as_deref()
                    .unwrap_or("ant_delirium_build_guard::run("),
            );
        }
    }

    for raw_file in &watch_files {
        let normalized = normalize_display_path(raw_file);
        validate_protected_file(
            config,
            &mut violations,
            &taxonomy_path,
            &normalized,
            "any",
            false,
            false,
            false,
            "watch",
        );
    }

    let has_enforced = violations
        .iter()
        .any(|violation| violation.rule_id.contains(".super_block.enforced."));

    SuperBlockPreflight {
        abort_scan: abort_on_violation && has_enforced,
        violations,
    }
}

fn validate_protected_file(
    config: &AuditConfig,
    violations: &mut Vec<AuditViolation>,
    taxonomy_path: &str,
    relative_path: &str,
    owner_policy: &str,
    require_readonly: bool,
    require_immutable: bool,
    enforced: bool,
    level: &str,
) {
    let path = config.resolve_project_path(relative_path);
    let rule_path = "hierarchy.sanitization.super_block";
    let suffix = sanitize_suffix(relative_path);
    let level_segment = if enforced { "enforced" } else { level };

    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(_) => {
            violations.push(AuditViolation {
                file_path: relative_path.to_string(),
                line: 1,
                rule_id: governance::rule_id(
                    rule_path,
                    &format!("{}.missing.{}", level_segment, suffix),
                ),
                message: format!(
                    "Super Block violation: protected file '{}' is missing.",
                    relative_path
                ),
                snippet: None,
                quick_fix: Some(format!(
                    "// [SECURE] Restore '{}' before running the governance audit.",
                    relative_path
                )),
                detected_dna: String::new(),
                taxonomy_path: taxonomy_path.to_string(),
            });
            return;
        }
    };

    let readonly_ok = !require_readonly || is_readonly_locked(&metadata);
    let immutable_state = if require_immutable {
        is_immutable_locked(&path)
    } else {
        Some(true)
    };
    let immutable_ok = immutable_state.unwrap_or(false);
    let owner_ok = owner_matches(owner_policy, &metadata)
        || owner_is_squashed_superuser(owner_policy, &metadata, readonly_ok, immutable_ok);

    if !owner_ok {
        violations.push(AuditViolation {
            file_path: relative_path.to_string(),
            line: 1,
            rule_id: governance::rule_id(rule_path, &format!("{}.owner.{}", level_segment, suffix)),
            message: format!(
                "Super Block violation: protected file '{}' is not owned by the expected policy '{}'.",
                relative_path, owner_policy
            ),
            snippet: None,
            quick_fix: Some(format!(
                "// [SECURE] Reassign '{}' to the expected owner policy '{}' before continuing.",
                relative_path, owner_policy
            )),
            detected_dna: String::new(),
            taxonomy_path: taxonomy_path.to_string(),
        });
    }

    if require_readonly && !readonly_ok {
        violations.push(AuditViolation {
            file_path: relative_path.to_string(),
            line: 1,
            rule_id: governance::rule_id(rule_path, &format!("{}.readonly.{}", level_segment, suffix)),
            message: format!(
                "Super Block violation: protected file '{}' is writable but should be locked read-only.",
                relative_path
            ),
            snippet: None,
            quick_fix: Some(format!(
                "// [SECURE] Lock '{}' as read-only before starting the audit.",
                relative_path
            )),
            detected_dna: String::new(),
            taxonomy_path: taxonomy_path.to_string(),
        });
    }

    if require_immutable {
        match immutable_state {
            Some(true) => {}
            Some(false) => {
                violations.push(AuditViolation {
                    file_path: relative_path.to_string(),
                    line: 1,
                    rule_id: governance::rule_id(
                        rule_path,
                        &format!("{}.immutable.{}", level_segment, suffix),
                    ),
                    message: format!(
                        "Super Block violation: protected file '{}' is mutable; expected immutable lock to prevent delete-and-replace attacks.",
                        relative_path
                    ),
                    snippet: None,
                    quick_fix: Some(format!(
                        "// [SECURE] Apply immutable lock on '{}' (example: sudo chattr +i '{}').",
                        relative_path, relative_path
                    )),
                    detected_dna: String::new(),
                    taxonomy_path: taxonomy_path.to_string(),
                });
            }
            None => {
                violations.push(AuditViolation {
                    file_path: relative_path.to_string(),
                    line: 1,
                    rule_id: governance::rule_id(
                        rule_path,
                        &format!("{}.immutable_probe.{}", level_segment, suffix),
                    ),
                    message: format!(
                        "Super Block violation: unable to verify immutable lock for '{}'; install/allow 'lsattr' verification on this host.",
                        relative_path
                    ),
                    snippet: None,
                    quick_fix: Some(
                        "// [SECURE] Ensure immutable-bit verification is available (lsattr) and re-run the audit."
                            .to_string(),
                    ),
                    detected_dna: String::new(),
                    taxonomy_path: taxonomy_path.to_string(),
                });
            }
        }
    }
}

fn validate_build_guard_integrity(
    config: &AuditConfig,
    violations: &mut Vec<AuditViolation>,
    taxonomy_path: &str,
    build_path: &str,
    guard_symbol: &str,
    guard_call_prefix: &str,
) {
    let path = config.resolve_project_path(build_path);
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(_) => return,
    };
    let rule_path = "hierarchy.sanitization.super_block";
    let suffix = sanitize_suffix(build_path);

    if !source.contains(guard_symbol) {
        violations.push(AuditViolation {
            file_path: build_path.to_string(),
            line: 1,
            rule_id: governance::rule_id(rule_path, &format!("enforced.guard_symbol.{}", suffix)),
            message: format!(
                "Super Block violation: '{}' does not reference the standardized guard symbol '{}'.",
                build_path, guard_symbol
            ),
            snippet: None,
            quick_fix: Some(format!(
                "// [SECURE] Import the standardized build guard '{}' in '{}'.",
                guard_symbol, build_path
            )),
            detected_dna: String::new(),
            taxonomy_path: taxonomy_path.to_string(),
        });
        return;
    }

    match first_main_statement(&source) {
        Some((line, statement)) => {
            if !statement.starts_with(guard_call_prefix) {
                violations.push(AuditViolation {
                    file_path: build_path.to_string(),
                    line,
                    rule_id: governance::rule_id(rule_path, &format!("enforced.guard_order.{}", suffix)),
                    message: format!(
                        "Super Block violation: '{}' must call the build guard as the first operational statement inside fn main().",
                        build_path
                    ),
                    snippet: Some(statement.to_string()),
                    quick_fix: Some(format!(
                        "// [SECURE] Move '{}' to the first statement inside fn main().",
                        guard_call_prefix
                    )),
                    detected_dna: String::new(),
                    taxonomy_path: taxonomy_path.to_string(),
                });
            }
        }
        None => {
            violations.push(AuditViolation {
                file_path: build_path.to_string(),
                line: 1,
                rule_id: governance::rule_id(rule_path, &format!("enforced.main_missing.{}", suffix)),
                message: format!(
                    "Super Block violation: '{}' does not expose a parsable fn main() for guard validation.",
                    build_path
                ),
                snippet: None,
                quick_fix: Some("// [SECURE] Restore a canonical fn main() entry point and call the build guard first.".to_string()),
                detected_dna: String::new(),
                taxonomy_path: taxonomy_path.to_string(),
            });
        }
    }
}

fn first_main_statement(source: &str) -> Option<(usize, &str)> {
    let mut in_main = false;
    let mut brace_depth = 0usize;
    let mut entered_body = false;

    for (idx, raw_line) in source.lines().enumerate() {
        let trimmed = raw_line.trim();

        if !in_main {
            if trimmed.starts_with("fn main(") || trimmed.starts_with("fn main ()") {
                in_main = true;
            } else {
                continue;
            }
        }

        if !entered_body {
            let opens = trimmed.matches('{').count();
            if opens > 0 {
                brace_depth += opens;
                brace_depth = brace_depth.saturating_sub(trimmed.matches('}').count());
                entered_body = true;
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with("//") {
            brace_depth += trimmed.matches('{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.matches('}').count());
            continue;
        }

        if brace_depth == 0 {
            return None;
        }

        return Some((idx + 1, trimmed));
    }

    None
}

fn resolve_bool(config: &AuditConfig, key: &str, default_value: bool) -> bool {
    config
        .hierarchy
        .resolve_parameter(key)
        .map(|value| is_truthy(&value))
        .unwrap_or(default_value)
}

fn resolve_string(config: &AuditConfig, key: &str) -> Option<String> {
    config
        .hierarchy
        .resolve_parameter(key)
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_string_array(config: &AuditConfig, key: &str) -> Vec<String> {
    config
        .hierarchy
        .resolve_parameter(key)
        .map(|raw| parse_array_like(&raw))
        .unwrap_or_default()
}

fn parse_array_like(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return Vec::new();
    }

    trimmed[1..trimmed.len() - 1]
        .split(',')
        .map(|item| item.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn is_truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "false" | "0" | "off" | "no" | "disabled"
    )
}

fn sanitize_suffix(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '_',
        })
        .collect()
}

fn normalize_display_path(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn owner_policy_requires_immutable(policy: &str) -> bool {
    matches!(
        policy.trim().to_ascii_lowercase().as_str(),
        "root" | "sudo" | "superuser"
    )
}

#[cfg(unix)]
fn owner_is_squashed_superuser(
    policy: &str,
    metadata: &fs::Metadata,
    readonly_ok: bool,
    immutable_ok: bool,
) -> bool {
    matches!(
        policy.trim().to_ascii_lowercase().as_str(),
        "root" | "sudo" | "superuser"
    ) && metadata.uid() == 65534
        && readonly_ok
        && immutable_ok
}

#[cfg(not(unix))]
fn owner_is_squashed_superuser(
    _policy: &str,
    _metadata: &fs::Metadata,
    _readonly_ok: bool,
    _immutable_ok: bool,
) -> bool {
    false
}

#[cfg(unix)]
fn is_readonly_locked(metadata: &fs::Metadata) -> bool {
    metadata.permissions().mode() & 0o222 == 0
}

#[cfg(not(unix))]
fn is_readonly_locked(metadata: &fs::Metadata) -> bool {
    metadata.permissions().readonly()
}

#[cfg(unix)]
fn owner_matches(policy: &str, metadata: &fs::Metadata) -> bool {
    match policy.trim().to_ascii_lowercase().as_str() {
        "" | "any" => true,
        "root" | "sudo" | "superuser" => metadata.uid() == 0,
        _ => true,
    }
}

#[cfg(not(unix))]
fn owner_matches(policy: &str, _metadata: &fs::Metadata) -> bool {
    matches!(policy.trim().to_ascii_lowercase().as_str(), "" | "any")
}

#[cfg(unix)]
fn is_immutable_locked(path: &Path) -> Option<bool> {
    let output = Command::new("lsattr").arg("-d").arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let attrs = stdout.split_whitespace().next()?;
    Some(attrs.contains('i'))
}

#[cfg(not(unix))]
fn is_immutable_locked(_path: &Path) -> Option<bool> {
    Some(true)
}
