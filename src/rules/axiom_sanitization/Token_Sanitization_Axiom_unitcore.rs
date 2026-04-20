use crate::governance;
use crate::structs::{AuditContext, AuditViolation};

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let resolution = match ctx.require_named_global_axiom_parameter("token_sanitization", "restricted_tokens") {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "token_sanitization", message);
            return;
        }
    };

    let tokens = parse_restricted_tokens(&resolution.value);
    if tokens.is_empty() {
        return;
    }

    for (idx, line) in ctx.source.lines().enumerate() {
        let trimmed = line.trim();
        if should_skip_line(trimmed) {
            continue;
        }

        for (token, _) in &tokens {
            if !trimmed.contains(token) {
                continue;
            }
            if has_manual_bypass(trimmed) {
                continue;
            }

            ctx.violations.push(AuditViolation {
                file_path: ctx.file_path.to_string(),
                line: idx + 1,
                rule_id: governance::rule_id("hierarchy.sanitization.token_sanitization", token),
                message: format!(
                    "Token Sanitization Violation: '{}' must be isolated or replaced according to {}.",
                    token, resolution.resolved_path
                ),
                snippet: Some(trimmed.to_string()),
                quick_fix: Some(format!(
                    "// [SANITIZE] Rewrite this usage so '{}' no longer appears in governed source.",
                    token
                )),
                detected_dna: ctx.detected_dna.clone(),
                taxonomy_path: resolution.resolved_path.clone(),
            });
        }
    }
}

fn parse_restricted_tokens(raw: &str) -> Vec<(String, String)> {
    let trimmed = raw.trim().trim_start_matches('{').trim_end_matches('}');
    if trimmed.is_empty() {
        return Vec::new();
    }

    trimmed
        .split(',')
        .filter_map(|entry| {
            let (key, value) = entry.split_once('=')?;
            let token = key.trim().trim_matches('"').to_string();
            let hint = value.trim().trim_matches('"').to_string();
            if token.is_empty() || hint.is_empty() {
                return None;
            }
            Some((token, hint))
        })
        .collect()
}

fn should_skip_line(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*")
}

fn has_manual_bypass(trimmed: &str) -> bool {
    trimmed.contains("[Safety Required]") || trimmed.contains("[TEMP_ALLOWED]")
}

fn push_resolution_violation(ctx: &mut AuditContext<'_>, rule_suffix: &str, message: String) {
    ctx.violations.push(AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: format!("hierarchy.sanitization.{}.resolution", rule_suffix),
        message,
        snippet: None,
        quick_fix: None,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: ctx.taxonomy_path.clone(),
    });
}
