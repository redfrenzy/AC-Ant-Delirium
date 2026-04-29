use crate::governance;
use crate::structs::{AuditContext, AuditViolation, ScannedBlock};

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if !ctx.file_path.ends_with(".rs") {
        return;
    }

    let enabled_resolution =
        match ctx.require_named_taxonomy_axiom_parameter("else_free", "else_free") {
            Ok(value) => value,
            Err(message) => {
                push_resolution_violation(ctx, "else_free", message);
                return;
            }
        };
    let enabled = match parse_bool_parameter(
        "else_free",
        "else_free",
        &enabled_resolution.value,
        &enabled_resolution.resolved_path,
    ) {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "else_free", message);
            return;
        }
    };
    if !enabled {
        return;
    }

    let allow_else_if_resolution =
        match ctx.require_named_taxonomy_axiom_parameter("else_free", "else_free_allow_else_if") {
            Ok(value) => value,
            Err(message) => {
                push_resolution_violation(ctx, "else_free", message);
                return;
            }
        };
    let allow_else_if = match parse_bool_parameter(
        "else_free",
        "else_free_allow_else_if",
        &allow_else_if_resolution.value,
        &allow_else_if_resolution.resolved_path,
    ) {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "else_free", message);
            return;
        }
    };

    let bypass_resolution =
        match ctx.require_named_taxonomy_axiom_parameter("else_free", "else_free_bypass_token") {
            Ok(value) => value,
            Err(message) => {
                push_resolution_violation(ctx, "else_free", message);
                return;
            }
        };
    let bypass_token = bypass_resolution.value.trim().to_string();
    if bypass_token.is_empty() {
        push_resolution_violation(
            ctx,
            "else_free",
            format!(
                "else_free_bypass_token resolved at '{}' but is empty.",
                bypass_resolution.resolved_path
            ),
        );
        return;
    }

    for func in ctx.functions {
        if func.kind != "function" {
            continue;
        }

        let violations = collect_else_violations(func, allow_else_if, &bypass_token);
        for violation in violations {
            ctx.violations.push(AuditViolation {
                file_path: ctx.file_path.to_string(),
                line: func.start_line + violation.line_offset,
                rule_id: governance::rule_id(&enabled_resolution.identity_path, &func.name),
                message: format!(
                    "Else-Free violation: function '{}' contains '{}' without bypass token '{}'. Prefer guard clauses or smaller helpers.",
                    func.name, violation.pattern, bypass_token
                ),
                snippet: Some(violation.snippet),
                quick_fix: Some(format!(
                    "// [REFAC] Replace '{}' with early return / extracted helper, or justify the branch with '{}'.",
                    violation.pattern, bypass_token
                )),
                detected_dna: ctx.detected_dna.clone(),
                taxonomy_path: format!("{}.else_free", enabled_resolution.resolved_path),
            });
        }
    }
}

#[derive(Debug)]
struct ElseViolation {
    line_offset: usize,
    pattern: &'static str,
    snippet: String,
}

fn collect_else_violations(
    func: &ScannedBlock<'_>,
    allow_else_if: bool,
    bypass_token: &str,
) -> Vec<ElseViolation> {
    let mut violations = Vec::new();

    for (line_offset, raw_line) in func.body.lines().enumerate() {
        let analysis = sanitize_code_segment(raw_line);
        if !contains_word(&analysis.code, "else") {
            continue;
        }

        let is_else_if = contains_else_if(&analysis.code);
        if is_else_if && allow_else_if {
            continue;
        }
        if analysis.comment.contains(bypass_token) {
            continue;
        }

        violations.push(ElseViolation {
            line_offset,
            pattern: if is_else_if { "else if" } else { "else" },
            snippet: raw_line.trim().to_string(),
        });
    }

    violations
}

struct LineAnalysis {
    code: String,
    comment: String,
}

fn sanitize_code_segment(raw_line: &str) -> LineAnalysis {
    let mut code = String::with_capacity(raw_line.len());
    let mut comment = String::new();
    let mut in_string = false;
    let mut chars = raw_line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '"' {
            in_string = !in_string;
            code.push(' ');
            continue;
        }
        if in_string {
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            let mut comment_text = String::from("//");
            let _ = chars.next();
            for rest in chars {
                comment_text.push(rest);
            }
            comment = comment_text;
            break;
        }
        code.push(ch);
    }

    LineAnalysis { code, comment }
}

fn contains_else_if(code: &str) -> bool {
    let Some(idx) = find_word_index(code, "else") else {
        return false;
    };
    code[idx + 4..].trim_start().starts_with("if")
}

fn contains_word(code: &str, needle: &str) -> bool {
    find_word_index(code, needle).is_some()
}

fn find_word_index(code: &str, needle: &str) -> Option<usize> {
    let bytes = code.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() || bytes.len() < needle_bytes.len() {
        return None;
    }

    for idx in 0..=bytes.len() - needle_bytes.len() {
        if &bytes[idx..idx + needle_bytes.len()] != needle_bytes {
            continue;
        }
        let prev_ok = idx == 0 || !is_word_char(bytes[idx - 1]);
        let next_idx = idx + needle_bytes.len();
        let next_ok = next_idx == bytes.len() || !is_word_char(bytes[next_idx]);
        if prev_ok && next_ok {
            return Some(idx);
        }
    }

    None
}

fn is_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn parse_bool_parameter(
    axiom_id: &str,
    requirement_id: &str,
    raw: &str,
    resolved_path: &str,
) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(format!(
            "{} requires '{}' to be boolean, but '{}' was found in '{}'.",
            axiom_id, requirement_id, raw, resolved_path
        )),
    }
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
