use crate::structs::AuditContext;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if let Err(message) = ctx.require_named_global_axiom_parameter("why_justification", "why_justification") {
        push_resolution_violation(ctx, "why_justification", &message);
        return;
    }
    if !ctx.file_path.ends_with(".md") {
        return;
    }
    if !ctx.source.to_ascii_lowercase().contains("[why]") {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.pedagogical.why_justification".to_string(),
            message: "Why justification violation: markdown must include [WHY].".to_string(),
            snippet: Some(ctx.source.lines().next().unwrap_or_default().to_string()),
            quick_fix: Some("[WHY] Explain the rationale.".to_string()),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.pedagogical.why_justification".to_string(),
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
