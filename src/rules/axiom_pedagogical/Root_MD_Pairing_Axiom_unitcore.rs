use crate::structs::AuditContext;
use std::path::Path;

#[path = "Pedagogical_Tag_Common_unitcore.rs"]
mod Pedagogical_Tag_Common_unitcore;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let root_md_pairing = match ctx.require_named_global_axiom_parameter("root_md_pairing", "root_md_pairing") {
        Ok(binding) => binding.value == "true",
        Err(message) => {
            Pedagogical_Tag_Common_unitcore::push_resolution_violation(ctx, "root_md_pairing", &message);
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
        let identity_path = Pedagogical_Tag_Common_unitcore::resolve_axiom_identity_path_or_fallback(ctx, "root_md_pairing");
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: identity_path.clone(),
            message: format!("Root MD pairing violation: '{}' is missing '{}'.", stem, manifest.display()),
            snippet: Some(stem.to_string()),
            quick_fix: Some(format!("Create {}", manifest.display())),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path,
        });
    }
}
