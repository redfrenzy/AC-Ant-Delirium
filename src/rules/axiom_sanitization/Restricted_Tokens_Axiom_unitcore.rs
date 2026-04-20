use crate::governance;
use crate::rules_logs::messages;
use crate::structs::{AuditContext, AuditViolation};

pub fn patrol(ctx: &mut AuditContext<'_>) {
    let resolution = match ctx.require_named_global_axiom_parameter("restricted_tokens", "restricted_tokens") {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "restricted_tokens", message);
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

        for (token, hint) in &tokens {
            if !trimmed.contains(token) {
                continue;
            }
            if has_manual_bypass(trimmed) {
                continue;
            }

            let (message, quick_fix) = build_restricted_token_feedback(token, hint, &resolution.resolved_path, ctx.file_path);

            ctx.violations.push(AuditViolation {
                file_path: ctx.file_path.to_string(),
                line: idx + 1,
                rule_id: governance::rule_id("hierarchy.sanitization.restricted_tokens", token),
                message,
                snippet: Some(trimmed.to_string()),
                quick_fix,
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

fn build_restricted_token_feedback(
    token: &str,
    hint: &str,
    resolved_path: &str,
    file_path: &str,
) -> (String, Option<String>) {
    if file_path.contains("src/restricted_tokens/") {
        return (
            format!(
                "Illegal Logic: token '{}' found inside restricted quarantine without explicit authorization.",
                token
            ),
            Some(format!(
                "# [SANITIZE] Add an explicit allowlist rule for '{}' or move this logic out of quarantine.",
                file_path
            )),
        );
    }

    (
        format!("{}: '{}' found. Hint: {}", messages::RESTRICTED_TOKEN, token, hint),
        Some(format!(
            "// [SANITIZE] Replace '{}' using the policy declared in {}.",
            token, resolved_path
        )),
    )
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
