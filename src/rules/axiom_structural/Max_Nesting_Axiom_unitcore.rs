use crate::governance;
use crate::rules_logs::messages;
use crate::structs::{AuditContext, AuditViolation};

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let requirement = match ctx.require_named_taxonomy_axiom_parameter("max_nesting", "max_nesting") {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "max_nesting", message);
            return;
        }
    };

    let resolved_path = requirement.resolved_path.clone();
    let raw_limit = requirement.value;

    let nesting_limit = match raw_limit.parse::<usize>() {
        Ok(value) => value,
        Err(_) => {
            push_resolution_violation(
                ctx,
                "max_nesting",
                format!("max_nesting requires a numeric value, but '{}' was found in '{}'.", raw_limit, resolved_path),
            );
            return;
        }
    };

    if nesting_limit == 0 {
        return;
    }

    for func in ctx.functions {
        let max_depth = count_max_nesting(func.body);
        if max_depth > nesting_limit {
            ctx.violations.push(AuditViolation {
                file_path: ctx.file_path.to_string(),
                line: func.start_line,
                rule_id: governance::rule_id(&requirement.identity_path, &func.name),
                message: format!(
                    "{} - {}/{} levels. Flatten this logic tree.",
                    messages::NESTING_TOO_DEEP,
                    max_depth,
                    nesting_limit
                ),
                snippet: Some(func.declaration.to_string()),
                quick_fix: Some("// [REFAC] Extract nested branches into smaller helper functions.".to_string()),
                detected_dna: ctx.detected_dna.clone(),
                taxonomy_path: format!("{}.max_nesting", resolved_path),
            });
        }
    }
}

fn count_max_nesting(body: &str) -> usize {
    let mut current = 0usize;
    let mut max_depth = 0usize;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }

        for ch in trimmed.chars() {
            if ch == '{' {
                current += 1;
                max_depth = max_depth.max(current);
            } else if ch == '}' && current > 0 {
                current -= 1;
            }
        }
    }

    max_depth
}

fn push_resolution_violation(ctx: &mut AuditContext<'_>, axiom_id: &str, message: String) {
    let identity_path = ctx.resolve_axiom_identity_path(axiom_id)
        .unwrap_or_else(|_| format!("axiom.{}", axiom_id));
    ctx.violations.push(AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: format!("{}.resolution", identity_path),
        message,
        snippet: None,
        quick_fix: None,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: identity_path,
    });
}
