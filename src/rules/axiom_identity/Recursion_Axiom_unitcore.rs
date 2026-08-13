use crate::structs::AuditContext;
use std::path::Path;

#[path = "Identity_Axiom_Common_unitcore.rs"]
mod Identity_Axiom_Common_unitcore;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let recursion_enabled = match ctx.require_named_hybrid_axiom_parameter("recursion", "recursion")
    {
        Ok(binding) => binding.value == "true",
        Err(message) => {
            Identity_Axiom_Common_unitcore::push_resolution_violation(ctx, "recursion", &message);
            return;
        }
    };
    if !recursion_enabled || ctx.detected_dna.is_empty() || !ctx.file_path.ends_with(".rs") {
        return;
    }
    let identity_path =
        Identity_Axiom_Common_unitcore::resolve_axiom_identity_path_or_fallback(ctx, "recursion");

    let tag_separator = ctx
        .config
        .hierarchy
        .resolve_parameter("tag_separator")
        .unwrap_or_else(|| "_".to_string());
    let file_path = Path::new(ctx.file_path);
    let stem = file_path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or_default();
    let dna_suffix = format!("{}{}", tag_separator, ctx.detected_dna);
    let core_name = if stem.ends_with(&dna_suffix) {
        stem.trim_end_matches(&dna_suffix)
    } else {
        stem.rsplit_once(&tag_separator)
            .map(|(base, _)| base)
            .unwrap_or(stem)
    };
    let parent = file_path.parent().unwrap_or_else(|| Path::new("."));

    enforce_spatial_jurisdiction(ctx, &identity_path);

    let child_suffixes = find_immediate_child_suffixes(ctx);
    for suffix in child_suffixes {
        let companion = format!("{}{}{}.rs", core_name, tag_separator, suffix);
        let companion_path = ctx.resolve_sibling_project_path(&companion);
        if !ctx.project_contains_path(&companion_path) {
            ctx.violations.push(crate::structs::AuditViolation {
                file_path: ctx.file_path.to_string(),
                line: 1,
                rule_id: format!("{}.{}", identity_path, suffix),
                message: format!(
                    "Recursion violation: '{}' requires companion '{}', but it was not found in '{}'.",
                    ctx.taxonomy_path,
                    companion,
                    parent.display()
                ),
                snippet: Some(companion),
                quick_fix: None,
                detected_dna: ctx.detected_dna.clone(),
                taxonomy_path: identity_path.clone(),
            });
        }
    }
}

fn enforce_spatial_jurisdiction(ctx: &mut AuditContext<'_>, identity_path: &str) {
    let Ok((_, jurisdiction)) = ctx
        .config
        .hierarchy
        .resolve_taxonomy_parameter(&ctx.taxonomy_path, "jurisdiction")
    else {
        return;
    };

    let expected_root = resolve_expected_jurisdiction_root(ctx, &jurisdiction, ctx.file_path);
    if expected_root.is_empty() {
        return;
    }

    let observed_path = normalize_relative_project_path(ctx.file_path);
    if observed_path.is_empty() || path_is_within_jurisdiction(&observed_path, &expected_root) {
        return;
    }

    ctx.violations.push(crate::structs::AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: format!("{}.jurisdiction", identity_path),
        message: format!(
            "Recursion violation: '{}' must stay inside jurisdiction '{}', but observed '{}'.",
            ctx.taxonomy_path, expected_root, observed_path
        ),
        snippet: Some(expected_root),
        quick_fix: Some(
            "// [TODO] Move this file into the resolved jurisdiction or revise the hierarchy if the topology intentionally changed."
                .to_string(),
        ),
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: identity_path.to_string(),
    });
}

fn resolve_expected_jurisdiction_root(
    ctx: &AuditContext<'_>,
    jurisdiction: &str,
    observed_path: &str,
) -> String {
    let project_root = ctx.config.project_root_path();
    let observed_absolute = ctx.config.resolve_project_path(observed_path);
    let package_root =
        resolve_nearest_package_root(&observed_absolute, &project_root).unwrap_or(project_root);
    let candidate = Path::new(jurisdiction);
    let resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        package_root.join(candidate)
    };

    normalize_relative_project_path(&ctx.config.display_path(resolved))
}

fn find_immediate_child_suffixes(ctx: &AuditContext<'_>) -> Vec<String> {
    let mut suffixes = Vec::new();
    let prefix = format!("{}.", ctx.taxonomy_path);
    let current_depth = ctx.taxonomy_path.matches('.').count();

    for (path, node) in ctx.config.hierarchy.nodes.iter() {
        if let Some(remainder) = path.strip_prefix(&prefix) {
            let parts: Vec<&str> = remainder.split('.').collect();
            if parts.len() == 1 && path.matches('.').count() == current_depth + 1 {
                let is_file_dna_child = node.properties.contains_key("type_logic")
                    || node.properties.contains_key("jurisdiction")
                    || node.properties.contains_key("manifest");

                if is_file_dna_child {
                    suffixes.push(parts[0].to_string());
                }
            }
        }
    }

    suffixes.sort();
    suffixes.dedup();
    suffixes
}

fn normalize_relative_project_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized.trim_matches('/').to_string()
}

fn resolve_nearest_package_root(
    observed_absolute: &Path,
    project_root: &Path,
) -> Option<std::path::PathBuf> {
    let mut current = observed_absolute.parent()?.to_path_buf();

    loop {
        if current.join("Cargo.toml").is_file() {
            return Some(current);
        }

        if current == project_root || !current.starts_with(project_root) || !current.pop() {
            break;
        }
    }

    None
}

fn path_is_within_jurisdiction(observed_path: &str, expected_root: &str) -> bool {
    if observed_path == expected_root {
        return true;
    }
    let expected_prefix = format!("{expected_root}/");
    observed_path.starts_with(&expected_prefix)
}
