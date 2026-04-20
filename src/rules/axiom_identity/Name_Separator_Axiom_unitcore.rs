use crate::structs::AuditContext;
use std::path::Path;

#[path = "Identity_Axiom_Common_unitcore.rs"]
mod Identity_Axiom_Common_unitcore;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let name_separator = match ctx.require_named_global_axiom_parameter("name_separator", "name_separator") {
        Ok(binding) => binding.value,
        Err(message) => {
            Identity_Axiom_Common_unitcore::push_resolution_violation(ctx, "name_separator", &message);
            return;
        }
    };
    let identity_path = Identity_Axiom_Common_unitcore::resolve_axiom_identity_path_or_fallback(ctx, "name_separator");

    let tag_separator = ctx.config.hierarchy.resolve_parameter("tag_separator")
        .unwrap_or_else(|| "_".to_string());

    if !ctx.file_path.ends_with(".rs") {
        return;
    }

    let stem = Path::new(ctx.file_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let dna_suffix = if ctx.detected_dna.is_empty() {
        String::new()
    } else {
        format!("{}{}", tag_separator, ctx.detected_dna)
    };
    let core_name = if !dna_suffix.is_empty() && stem.ends_with(&dna_suffix) {
        stem.trim_end_matches(&dna_suffix)
    } else {
        stem
            .rsplit_once(&tag_separator)
            .map(|(base, _)| base)
            .unwrap_or(stem)
    };

    if core_name.contains(&tag_separator) {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: identity_path.clone(),
            message: format!(
                "Name separator violation: base name '{}' must use '{}' instead of reserved tag separator '{}'.",
                core_name, name_separator, tag_separator
            ),
            snippet: Some(core_name.to_string()),
            quick_fix: Some(core_name.replace(&tag_separator, &name_separator)),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path,
        });
    }
}
