use crate::structs::AuditContext;

#[path = "Pedagogical_Tag_Common_unitcore.rs"]
mod Pedagogical_Tag_Common_unitcore;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let min_chars = match ctx.require_named_global_axiom_parameter("md_min_chars", "md_min_chars") {
        Ok(binding) => binding.value.parse::<usize>().unwrap_or(80),
        Err(message) => {
            Pedagogical_Tag_Common_unitcore::push_resolution_violation(ctx, "md_min_chars", &message);
            return;
        }
    };

    if !ctx.file_path.ends_with(".md") {
        return;
    }
    let chars = ctx.source.chars().filter(|c| !c.is_whitespace()).count();
    if chars < min_chars {
        let identity_path = Pedagogical_Tag_Common_unitcore::resolve_axiom_identity_path_or_fallback(ctx, "md_min_chars");
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: identity_path.clone(),
            message: format!("MD min chars violation: {} < {}.", chars, min_chars),
            snippet: Some(ctx.source.lines().next().unwrap_or_default().to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path,
        });
    }
}
