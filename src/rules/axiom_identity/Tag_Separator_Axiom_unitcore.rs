use crate::structs::AuditContext;
use std::path::Path;

#[path = "Identity_Axiom_Common_unitcore.rs"]
mod Identity_Axiom_Common_unitcore;

/// [AESTHETIC CORE - INTERNAL VACUUM]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let tag_separator = match ctx.require_named_global_axiom_parameter("tag_separator", "tag_separator") {
        Ok(binding) => binding.value,
        Err(message) => {
            Identity_Axiom_Common_unitcore::push_resolution_violation(ctx, "tag_separator", &message);
            return;
        }
    };
    let identity_path = Identity_Axiom_Common_unitcore::resolve_axiom_identity_path_or_fallback(ctx, "tag_separator");

    if !ctx.file_path.ends_with(".rs") {
        return;
    }

    let stem = Path::new(ctx.file_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    let expected = tag_separator.chars().next().unwrap_or('_');
    let known_separators = ['_', '-', '.'];
    let candidate_suffixes = collect_scope_suffixes(ctx);

    for separator in known_separators {
        if separator == expected {
            continue;
        }

        if let Some((_, suffix)) = stem.rsplit_once(separator) {
            if candidate_suffixes.iter().any(|candidate| candidate == suffix) {
                ctx.violations.push(crate::structs::AuditViolation {
                    file_path: ctx.file_path.to_string(),
                    line: 1,
                    rule_id: identity_path.clone(),
                    message: format!(
                        "Tag separator violation: '{}' uses '{}' before DNA '{}', but expected '{}'.",
                        stem, separator, suffix, expected
                    ),
                    snippet: Some(stem.to_string()),
                    quick_fix: Some(stem.replace(separator, &tag_separator)),
                    detected_dna: ctx.detected_dna.clone(),
                    taxonomy_path: identity_path.clone(),
                });
                return;
            }
        }
    }

    // Fallback for files without detected_dna: if they end with a plausible DNA token
    // using a foreign separator, we still flag the separator drift.
    if ctx.detected_dna.is_empty() && !stem.contains(expected) {
        for separator in ['-', '.'] {
            if let Some((_, suffix)) = stem.rsplit_once(separator) {
                let plausible_dna = !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_alphanumeric());
                if plausible_dna {
                    ctx.violations.push(crate::structs::AuditViolation {
                        file_path: ctx.file_path.to_string(),
                        line: 1,
                        rule_id: identity_path.clone(),
                        message: format!(
                            "Tag separator violation: '{}' likely uses '{}' before DNA-like suffix '{}', expected '{}'.",
                            stem, separator, suffix, expected
                        ),
                        snippet: Some(stem.to_string()),
                        quick_fix: Some(stem.replace(separator, &tag_separator)),
                        detected_dna: ctx.detected_dna.clone(),
                        taxonomy_path: identity_path.clone(),
                    });
                    return;
                }
            }
        }
    }
}

fn collect_scope_suffixes(ctx: &AuditContext<'_>) -> Vec<String> {
    let mut values = Vec::new();
    let scope_prefix = format!("{}.", ctx.taxonomy_path);

    for path in ctx.config.hierarchy.nodes.keys() {
        if let Some(remainder) = path.strip_prefix(&scope_prefix) {
            for segment in remainder.split('.') {
                if !segment.is_empty() {
                    values.push(segment.to_string());
                }
            }
        }
    }

    values.sort();
    values.dedup();
    values
}
