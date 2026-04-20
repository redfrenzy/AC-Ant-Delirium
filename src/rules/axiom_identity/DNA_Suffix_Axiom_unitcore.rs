use crate::structs::AuditContext;
use std::path::Path;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let tag_separator = match ctx.require_named_global_axiom_parameter("dna_suffix", "tag_separator") {
        Ok(binding) => binding.value,
        Err(message) => {
            push_resolution_violation(ctx, "dna_suffix", &message);
            return;
        }
    };

    if !ctx.file_path.ends_with(".rs") {
        return;
    }

    let stem = Path::new(ctx.file_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let has_logic = !ctx.functions.is_empty()
        || ctx.source.contains("fn ")
        || ctx.source.contains("struct ")
        || ctx.source.contains("enum ")
        || ctx.source.contains("macro_rules!");
    let has_dna_in_name = stem.contains(&tag_separator);

    if has_logic && ctx.detected_dna.is_empty() && !has_dna_in_name {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.identity.dna_suffix".to_string(),
            message: format!(
                "DNA suffix missing: '{}' contains logic but has no '{}' DNA tag in filename.",
                stem, tag_separator
            ),
            snippet: Some(stem.to_string()),
            quick_fix: Some(format!("{}{}unit.rs", stem, tag_separator)),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.identity.dna_suffix".to_string(),
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
