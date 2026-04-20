use crate::structs::AuditContext;
use std::path::Path;

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if let Err(message) = ctx.require_named_global_axiom_parameter("test_case_mapping", "test_case_mapping") {
        push_resolution_violation(ctx, "test_case_mapping", &message);
        return;
    }
    if !ctx.file_path.ends_with("_unit.rs") {
        return;
    }

    let path = Path::new(ctx.file_path);
    let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or_default();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let test_name = format!("{}.rs", stem.replace("_unit", "_test"));
    let test_path = parent.join(&test_name);
    if !test_path.exists() {
        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.pedagogical.test_case_mapping".to_string(),
            message: format!("Test mapping violation: '{}' has no mapped test '{}'.", stem, test_name),
            snippet: Some(stem.to_string()),
            quick_fix: Some(format!("Create {}", test_path.display())),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.pedagogical.test_case_mapping".to_string(),
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
