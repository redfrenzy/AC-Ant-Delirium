use crate::structs::AuditContext;
use std::path::Path;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let md_pairing = match ctx.require_named_hybrid_axiom_parameter("md_pairing", "md_pairing") {
        Ok(binding) => binding.value == "true",
        Err(message) => {
            push_resolution_violation(ctx, "md_pairing", &message);
            return;
        }
    };
    if !md_pairing || !ctx.file_path.ends_with(".rs") {
        return;
    }

    let path = Path::new(ctx.file_path);
    let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or_default();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let companion_md = parent.join(format!("{}.md", stem));
    if !companion_md.exists() {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.pedagogical.md_pairing".to_string(),
            message: format!("MD pairing violation: '{}' is missing companion '{}'.", stem, companion_md.display()),
            snippet: Some(stem.to_string()),
            quick_fix: Some(format!("Create {}", companion_md.display())),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.pedagogical.md_pairing".to_string(),
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
