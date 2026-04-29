use crate::structs::AuditContext;
use std::path::Path;

#[path = "Pedagogical_Tag_Common_unitcore.rs"]
mod Pedagogical_Tag_Common_unitcore;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if let Err(message) =
        ctx.require_named_global_axiom_parameter("inheritance_visibility", "inheritance_visibility")
    {
        Pedagogical_Tag_Common_unitcore::push_resolution_violation(
            ctx,
            "inheritance_visibility",
            &message,
        );
        return;
    }
    if !ctx.file_path.ends_with(".md") {
        return;
    }

    let tag_separator = ctx
        .config
        .hierarchy
        .resolve_parameter("tag_separator")
        .unwrap_or_else(|| "_".to_string());
    let stem = Path::new(ctx.file_path)
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or_default();

    if stem.contains(&tag_separator) {
        let identity_path =
            Pedagogical_Tag_Common_unitcore::resolve_axiom_identity_path_or_fallback(
                ctx,
                "inheritance_visibility",
            );
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: identity_path.clone(),
            message: "Inheritance visibility violation: child documentation must be inherited from parent, not owned by tagged child.".to_string(),
            snippet: Some(stem.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path,
        });
    }
}
