use crate::structs::AuditContext;
use std::path::Path;

#[path = "Identity_Axiom_Common_unitcore.rs"]
mod Identity_Axiom_Common_unitcore;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let tag_separator = match ctx
        .require_named_global_axiom_parameter("dna_suffix", "tag_separator")
    {
        Ok(binding) => binding.value,
        Err(message) => {
            Identity_Axiom_Common_unitcore::push_resolution_violation(ctx, "dna_suffix", &message);
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
    if Identity_Axiom_Common_unitcore::is_generic_rust_file_stem(stem) {
        return;
    }
    let has_logic = !ctx.functions.is_empty()
        || ctx.source.contains("fn ")
        || ctx.source.contains("struct ")
        || ctx.source.contains("enum ")
        || ctx.source.contains("macro_rules!");
    let has_dna_in_name = stem
        .rsplit_once(&tag_separator)
        .map(|(_, suffix)| !suffix.is_empty())
        .unwrap_or(false);
    let identity_path =
        Identity_Axiom_Common_unitcore::resolve_axiom_identity_path_or_fallback(ctx, "dna_suffix");
    let dna_matches_taxonomy = !ctx.detected_dna.is_empty()
        && ctx
            .taxonomy_path
            .rsplit('.')
            .next()
            .map(|segment| segment == ctx.detected_dna)
            .unwrap_or(false);

    if has_logic && !dna_matches_taxonomy && !has_dna_in_name {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: identity_path.clone(),
            message: format!(
                "DNA suffix missing: '{}' contains logic but has no '{}' DNA tag in filename.",
                stem, tag_separator
            ),
            snippet: Some(stem.to_string()),
            quick_fix: Some(format!("{}{}unit.rs", stem, tag_separator)),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path,
        });
    } else if has_logic && !dna_matches_taxonomy && has_dna_in_name {
        let invalid_suffix = stem
            .rsplit_once(&tag_separator)
            .map(|(_, suffix)| suffix)
            .unwrap_or_default();

        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: identity_path.clone(),
            message: format!(
                "DNA suffix invalid: '{}' uses '{}' after '{}', but this suffix is not defined in the active taxonomy.",
                stem, invalid_suffix, tag_separator
            ),
            snippet: Some(stem.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path,
        });
    }
}
