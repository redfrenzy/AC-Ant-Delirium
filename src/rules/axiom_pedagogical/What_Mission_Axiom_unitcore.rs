use crate::structs::AuditContext;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if let Err(message) = ctx.require_named_global_axiom_parameter("what_mission", "what_mission") {
        push_resolution_violation(ctx, "what_mission", &message);
        return;
    }
    if !ctx.file_path.ends_with(".md") {
        return;
    }
    if !ctx.source.to_ascii_lowercase().contains("[what]") {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.pedagogical.what_mission".to_string(),
            message: "What mission violation: markdown must include [WHAT].".to_string(),
            snippet: Some(ctx.source.lines().next().unwrap_or_default().to_string()),
            quick_fix: Some("[WHAT] Explain the mission.".to_string()),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.pedagogical.what_mission".to_string(),
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
