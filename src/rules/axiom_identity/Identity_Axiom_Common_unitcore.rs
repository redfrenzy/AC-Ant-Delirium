use crate::structs::AuditContext;

#[allow(dead_code)]
const GENERIC_RUST_FILE_STEMS: &[&str] = &["main", "lib", "build", "mod"];

pub fn resolve_axiom_identity_path_or_fallback(ctx: &AuditContext<'_>, axiom_id: &str) -> String {
    ctx.resolve_axiom_identity_path(axiom_id)
        .unwrap_or_else(|_| format!("axiom.{}", axiom_id))
}

#[allow(dead_code)]
pub fn is_generic_rust_file_stem(stem: &str) -> bool {
    GENERIC_RUST_FILE_STEMS.contains(&stem)
}

pub fn push_resolution_violation(ctx: &mut AuditContext<'_>, axiom_id: &str, message: &str) {
    let identity_path = resolve_axiom_identity_path_or_fallback(ctx, axiom_id);
    ctx.violations.push(crate::structs::AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: format!("{}.resolution", identity_path),
        message: message.to_string(),
        snippet: Some(ctx.file_path.to_string()),
        quick_fix: None,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: identity_path,
    });
}
