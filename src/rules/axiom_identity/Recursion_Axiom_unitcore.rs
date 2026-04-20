use crate::structs::AuditContext;
use std::path::Path;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let recursion_enabled = match ctx.require_named_hybrid_axiom_parameter("recursion", "recursion") {
        Ok(binding) => binding.value == "true",
        Err(message) => {
            push_resolution_violation(ctx, "recursion", &message);
            return;
        }
    };
    if !recursion_enabled || ctx.detected_dna.is_empty() || !ctx.file_path.ends_with(".rs") {
        return;
    }

    let tag_separator = ctx.config.hierarchy.resolve_parameter("tag_separator")
        .unwrap_or_else(|| "_".to_string());
    let file_path = Path::new(ctx.file_path);
    let stem = file_path.file_stem().and_then(|v| v.to_str()).unwrap_or_default();
    let core_name = stem.rsplit_once(&tag_separator).map(|(base, _)| base).unwrap_or(stem);
    let parent = file_path.parent().unwrap_or_else(|| Path::new("."));

    let child_suffixes = find_immediate_child_suffixes(ctx);
    for suffix in child_suffixes {
        let companion = format!("{}{}{}.rs", core_name, tag_separator, suffix);
        let companion_path = parent.join(&companion);
        if !companion_path.exists() {
            ctx.violations.push(crate::structs::AuditViolation {
                file_path: ctx.file_path.to_string(),
                line: 1,
                rule_id: format!("hierarchy.identity.recursion.{}", suffix),
                message: format!(
                    "Recursion violation: '{}' requires companion '{}', but it was not found in '{}'.",
                    ctx.taxonomy_path,
                    companion,
                    parent.display()
                ),
                snippet: Some(companion),
                quick_fix: None,
                detected_dna: ctx.detected_dna.clone(),
                taxonomy_path: "hierarchy.identity.recursion".to_string(),
            });
        }
    }
}

fn push_resolution_violation(ctx: &mut AuditContext<'_>, rule_suffix: &str, message: &str) {
    ctx.violations.push(crate::structs::AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: format!("hierarchy.identity.{}.resolution", rule_suffix),
        message: message.to_string(),
        snippet: Some(ctx.file_path.to_string()),
        quick_fix: None,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: ctx.taxonomy_path.clone(),
    });
}

fn find_immediate_child_suffixes(ctx: &AuditContext<'_>) -> Vec<String> {
    let mut suffixes = Vec::new();
    let prefix = format!("{}.", ctx.taxonomy_path);
    let current_depth = ctx.taxonomy_path.matches('.').count();

    for path in ctx.config.hierarchy.nodes.keys() {
        if let Some(remainder) = path.strip_prefix(&prefix) {
            let parts: Vec<&str> = remainder.split('.').collect();
            if parts.len() == 1 && path.matches('.').count() == current_depth + 1 {
                suffixes.push(parts[0].to_string());
            }
        }
    }

    suffixes.sort();
    suffixes.dedup();
    suffixes
}
