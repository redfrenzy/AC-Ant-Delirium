use crate::governance;
use crate::rules_logs::messages;
use crate::structs::{AuditContext, AuditViolation};

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let requirement = match ctx.require_named_taxonomy_axiom_parameter("max_params", "max_params") {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "max_params", message);
            return;
        }
    };

    let resolved_path = requirement.resolved_path.clone();
    let raw_limit = requirement.value;

    let param_limit = match raw_limit.parse::<usize>() {
        Ok(value) => value,
        Err(_) => {
            push_resolution_violation(
                ctx,
                "max_params",
                format!("max_params requires a numeric value, but '{}' was found in '{}'.", raw_limit, resolved_path),
            );
            return;
        }
    };

    if param_limit == 0 {
        return;
    }

    for func in ctx.functions {
        let param_count = count_parameters(func.declaration);
        if param_count > param_limit {
            ctx.violations.push(AuditViolation {
                file_path: ctx.file_path.to_string(),
                line: func.start_line,
                rule_id: governance::rule_id(&requirement.identity_path, &func.name),
                message: format!(
                    "{} - {}/{} parameters. Group them into a Struct.",
                    messages::PARAMS_TOO_MANY,
                    param_count,
                    param_limit
                ),
                snippet: Some(func.declaration.to_string()),
                quick_fix: Some("// [REFAC] Group these parameters into a dedicated Config/State Struct.".to_string()),
                detected_dna: ctx.detected_dna.clone(),
                taxonomy_path: format!("{}.max_params", resolved_path),
            });
        }
    }
}

fn count_parameters(declaration: &str) -> usize {
    let (start_idx, end_idx) = match (declaration.find('('), declaration.rfind(')')) {
        (Some(s), Some(e)) => (s, e),
        _ => return 0,
    };

    if end_idx <= start_idx {
        return 0;
    }

    let params_str = declaration[start_idx + 1..end_idx].trim();
    if params_str.is_empty() {
        return 0;
    }

    let mut count = 0;
    let mut depth = 0;
    for c in params_str.chars() {
        if c == '<' {
            depth += 1;
        }
        if c == '>' {
            depth -= 1;
        }
        if c == ',' && depth == 0 {
            count += 1;
        }
    }
    count + 1
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
