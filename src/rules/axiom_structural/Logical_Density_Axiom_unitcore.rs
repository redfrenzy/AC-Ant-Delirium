use crate::governance;
use crate::rules_logs::messages;
use crate::structs::{AuditContext, AuditViolation, ScannedBlock};

/// [AESTHETIC CORE - LOGICAL DENSITY PATROL]
pub fn patrol(ctx: &mut AuditContext<'_>) {
    let requirement =
        match ctx.require_named_taxonomy_axiom_parameter("logical_density", "logical_density") {
            Ok(value) => value,
            Err(message) => {
                push_resolution_violation(ctx, "logical_density", message);
                return;
            }
        };

    let resolved_path = requirement.resolved_path.clone();
    let raw_limit = requirement.value;

    let logical_limit = match raw_limit.parse::<usize>() {
        Ok(value) => value,
        Err(_) => {
            push_resolution_violation(
                ctx,
                "logical_density",
                format!(
                    "logical_density requires a numeric value, but '{}' was found in '{}'.",
                    raw_limit, resolved_path
                ),
            );
            return;
        }
    };

    if logical_limit == 0 {
        return;
    }

    check_function_metrics(
        ctx,
        logical_limit,
        &resolved_path,
        &requirement.identity_path,
    );
}

fn check_function_metrics(
    ctx: &mut AuditContext<'_>,
    logical_limit: usize,
    resolved_path: &str,
    identity_path: &str,
) {
    for func in ctx.functions {
        verify_single_function_metrics(ctx, func, logical_limit, resolved_path, identity_path);
    }
}

fn verify_single_function_metrics(
    ctx: &mut AuditContext<'_>,
    func: &ScannedBlock<'_>,
    logical_limit: usize,
    resolved_path: &str,
    identity_path: &str,
) {
    let logical_count = count_logical_statements(func);

    if logical_count > logical_limit {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: func.start_line,
            rule_id: governance::rule_id(identity_path, &func.name),
            message: format!(
                "{} - {}/{} statements. Fragment this logic.",
                messages::LOGICAL_DENSITY,
                logical_count,
                logical_limit
            ),
            snippet: Some(func.declaration.to_string()),
            quick_fix: Some("// [REFAC] Fragment this function into smaller atomic units to reduce logical mass.".to_string()),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: format!("{}.logical_density", resolved_path),
        });
    }
}

fn count_logical_statements(func: &ScannedBlock<'_>) -> usize {
    let mut count = 0;

    for line in func.body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        count += trimmed.matches(';').count();

        if trimmed.contains('{') && !trimmed.contains(';') && !trimmed.starts_with("fn ") {
            count += 1;
        }
    }

    count + 1
}

fn push_resolution_violation(ctx: &mut AuditContext<'_>, axiom_id: &str, message: String) {
    let identity_path = ctx
        .resolve_axiom_identity_path(axiom_id)
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
