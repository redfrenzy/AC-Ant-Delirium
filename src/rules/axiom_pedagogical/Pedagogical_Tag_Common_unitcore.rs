use crate::structs::AuditContext;

pub fn resolve_axiom_identity_path_or_fallback(ctx: &AuditContext<'_>, axiom_id: &str) -> String {
    ctx.resolve_axiom_identity_path(axiom_id)
        .unwrap_or_else(|_| format!("axiom.{}", axiom_id))
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

pub fn resolve_rule_enabled(
    ctx: &AuditContext<'_>,
    axiom_id: &str,
    requirement_id: &str,
    documentation_key: &str,
) -> Result<bool, String> {
    if let Some(value) = resolve_documentation_setting(ctx, documentation_key) {
        return Ok(parse_bool_like(&value));
    }

    let binding = ctx.require_named_hybrid_axiom_parameter(axiom_id, requirement_id)?;
    Ok(parse_bool_like(&binding.value))
}

pub fn verify_required_tag_on_functions(
    ctx: &mut AuditContext<'_>,
    axiom_id: &str,
    required_tag: &str,
    human_name: &str,
) {
    let required = required_tag.to_ascii_lowercase();
    let identity_path = resolve_axiom_identity_path_or_fallback(ctx, axiom_id);

    for func in ctx.functions.iter().filter(|func| func.kind == "function") {
        if func.docstring.to_ascii_lowercase().contains(&required) {
            continue;
        }

        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: func.start_line,
            rule_id: identity_path.clone(),
            message: format!(
                "{} violation: function '{}' must include {} in the documentation block above the declaration.",
                human_name,
                func.name,
                required_tag
            ),
            snippet: Some(func.declaration.to_string()),
            quick_fix: Some(format!("/// {} ...", required_tag)),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path.clone(),
        });
    }
}

fn parse_bool_like(value: &str) -> bool {
    let lowered = value.trim().to_ascii_lowercase();
    match lowered.as_str() {
        "false" | "0" | "off" | "no" | "disabled" => false,
        _ => true,
    }
}

fn resolve_documentation_setting(ctx: &AuditContext<'_>, key: &str) -> Option<String> {
    let mut current = ctx.taxonomy_path.trim().trim_matches('.').to_string();
    if current.is_empty() {
        return None;
    }

    loop {
        let direct = format!("{}.documentation.{}", current, key);
        if let Some(value) = ctx.config.hierarchy.search_params(&direct) {
            return Some(value);
        }

        match current.rsplit_once('.') {
            Some((parent, _)) => current = parent.to_string(),
            None => break,
        }
    }

    None
}
