use crate::structs::AuditContext;
use std::path::Path;

#[path = "Identity_Axiom_Common_unitcore.rs"]
mod Identity_Axiom_Common_unitcore;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let casing = match ctx.require_named_global_axiom_parameter("pascal_case", "casing") {
        Ok(binding) => binding.value,
        Err(message) => {
            Identity_Axiom_Common_unitcore::push_resolution_violation(ctx, "pascal_case", &message);
            return;
        }
    };

    if !ctx.file_path.ends_with(".rs") {
        return;
    }

    if casing != "PascalCase" {
        return;
    }
    let identity_path =
        Identity_Axiom_Common_unitcore::resolve_axiom_identity_path_or_fallback(ctx, "pascal_case");

    let tag_separator = ctx
        .config
        .hierarchy
        .resolve_parameter("tag_separator")
        .unwrap_or_else(|| "_".to_string());
    let stem = Path::new(ctx.file_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if Identity_Axiom_Common_unitcore::is_generic_rust_file_stem(stem) {
        return;
    }
    let has_explicit_dna_suffix = stem
        .rsplit_once(&tag_separator)
        .map(|(_, suffix)| !suffix.is_empty())
        .unwrap_or(false);
    if !has_explicit_dna_suffix && ctx.detected_dna.is_empty() {
        return;
    }
    let core_name = stem
        .rsplit_once(&tag_separator)
        .map(|(base, _)| base)
        .unwrap_or(stem);

    if !is_pascal_case(core_name) {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: identity_path.clone(),
            message: format!(
                "PascalCase violation: '{}' must use PascalCase segments separated by '-'.",
                core_name
            ),
            snippet: Some(core_name.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path,
        });
    }
}


fn is_pascal_case(input: &str) -> bool {
    if input.is_empty() || input.contains('_') || input.contains('.') {
        return false;
    }

    input.split('-').all(is_pascal_segment)
}

fn is_pascal_segment(segment: &str) -> bool {
    if segment.is_empty() {
        return false;
    }
    let mut chars = segment.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => (),
        _ => return false,
    }
    segment.chars().all(|c| c.is_ascii_alphanumeric())
}
