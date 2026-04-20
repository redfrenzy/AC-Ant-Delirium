use crate::structs::AuditContext;
use std::path::Path;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let name_separator = match ctx.require_named_global_axiom_parameter("name_separator", "name_separator") {
        Ok(binding) => binding.value,
        Err(message) => {
            push_resolution_violation(ctx, "name_separator", &message);
            return;
        }
    };

    let tag_separator = ctx.config.hierarchy.resolve_parameter("tag_separator")
        .unwrap_or_else(|| "_".to_string());

    if !ctx.file_path.ends_with(".rs") {
        return;
    }

    let stem = Path::new(ctx.file_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let core_name = stem
        .rsplit_once(&tag_separator)
        .map(|(base, _)| base)
        .unwrap_or(stem);

    if core_name.contains(&tag_separator) {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.identity.name_separator".to_string(),
            message: format!(
                "Name separator violation: base name '{}' must use '{}' instead of reserved tag separator '{}'.",
                core_name, name_separator, tag_separator
            ),
            snippet: Some(core_name.to_string()),
            quick_fix: Some(core_name.replace(&tag_separator, &name_separator)),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.identity.name_separator".to_string(),
        });
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
