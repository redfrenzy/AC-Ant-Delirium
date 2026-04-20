use crate::structs::AuditContext;
use std::path::Path;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let root_md_pairing = match ctx.require_named_global_axiom_parameter("root_md_pairing", "root_md_pairing") {
        Ok(binding) => binding.value == "true",
        Err(message) => {
            push_resolution_violation(ctx, "root_md_pairing", &message);
            return;
        }
    };
    if !root_md_pairing || !ctx.file_path.ends_with(".md") {
        return;
    }

    let path = Path::new(ctx.file_path);
    let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or_default();
    let tag_separator = ctx.config.hierarchy.resolve_parameter("tag_separator")
        .unwrap_or_else(|| "_".to_string());
    if stem.contains(&tag_separator) {
        return;
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let manifest = parent.join(format!("{}_manifest.json", stem));
    if !manifest.exists() {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.pedagogical.root_md_pairing".to_string(),
            message: format!("Root MD pairing violation: '{}' is missing '{}'.", stem, manifest.display()),
            snippet: Some(stem.to_string()),
            quick_fix: Some(format!("Create {}", manifest.display())),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.pedagogical.root_md_pairing".to_string(),
        });
    }
}

fn push_resolution_violation(ctx: &mut AuditContext<'_>, rule_suffix: &str, message: &str) {
    ctx.violations.push(crate::structs::AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: format!("hierarchy.pedagogical.{}.resolution", rule_suffix),
        message: message.to_string(),
        snippet: Some(ctx.file_path.to_string()),
        quick_fix: None,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: ctx.taxonomy_path.clone(),
    });
}
