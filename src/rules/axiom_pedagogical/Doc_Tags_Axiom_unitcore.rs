use crate::structs::AuditContext;

#[path = "Pedagogical_Tag_Common_unitcore.rs"]
mod Pedagogical_Tag_Common_unitcore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocTagCheckKind {
    MinChars,
    WordSalad,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DocTagCheckFailure {
    kind: DocTagCheckKind,
    detail: String,
}

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if !ctx.file_path.ends_with(".rs") {
        return;
    }

    let enabled = match Pedagogical_Tag_Common_unitcore::resolve_doc_tags_aggregator_enabled(ctx) {
        Ok(value) => value,
        Err(message) => {
            Pedagogical_Tag_Common_unitcore::push_resolution_violation(ctx, "doc_tags", &message);
            return;
        }
    };

    if !enabled {
        return;
    }

    let checks = Pedagogical_Tag_Common_unitcore::collect_supported_doc_tag_checks(ctx);
    if checks.is_empty() {
        return;
    }

    let identity_path =
        Pedagogical_Tag_Common_unitcore::resolve_axiom_identity_path_or_fallback(ctx, "doc_tags");

    for func in ctx.functions.iter().filter(|func| func.kind == "function") {
        let mut missing = Vec::new();
        let mut failed_checks = Vec::new();
        let mut quick_fix_lines = Vec::new();
        let docstring_lower = func.docstring.to_ascii_lowercase();

        for tag_check in checks.iter() {
            let required_lower = tag_check.required_tag.to_ascii_lowercase();
            let suggested = tag_check
                .contract
                .info
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("...");
            quick_fix_lines.push(format!("/// {} {}", tag_check.required_tag, suggested));

            if !docstring_lower.contains(&required_lower) {
                missing.push(tag_check.required_tag.clone());
                continue;
            }

            let content = extract_tag_payload(&func.docstring, &tag_check.required_tag);
            for failure in run_internal_checklist(&content, &func.name, tag_check) {
                failed_checks.push(format!("{} {}", tag_check.required_tag, failure.detail));
            }
        }

        if missing.is_empty() && failed_checks.is_empty() {
            continue;
        }

        let mut details = Vec::new();
        if !missing.is_empty() {
            details.push(format!("missing {}", missing.join(", ")));
        }
        if !failed_checks.is_empty() {
            details.push(format!("failed {}", failed_checks.join(", ")));
        }

        ctx.violations.push(crate::structs::AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: func.start_line,
            rule_id: identity_path.clone(),
            message: format!(
                "Doc tags violation: function '{}' has incomplete pedagogical contract: {}.",
                func.name,
                details.join("; ")
            ),
            snippet: Some(func.declaration.to_string()),
            quick_fix: Some(quick_fix_lines.join("\n")),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: identity_path.clone(),
        });
    }
}

fn run_internal_checklist(
    content: &str,
    symbol_name: &str,
    tag_check: &Pedagogical_Tag_Common_unitcore::DocumentationTagCheck,
) -> Vec<DocTagCheckFailure> {
    let mut failures = Vec::new();

    if let Some(min_chars) = tag_check.contract.min_chars {
        let count = content.chars().count();
        if count < min_chars {
            failures.push(DocTagCheckFailure {
                kind: DocTagCheckKind::MinChars,
                detail: format!("is too short ({}/{})", count, min_chars),
            });
        }
    }

    if tag_check.contract.word_salad_enabled {
        if let Some((ratio, repeated)) = detect_word_salad(
            content,
            symbol_name,
            tag_check.contract.word_salad_threshold,
            tag_check.contract.word_salad_min_tokens,
        ) {
            failures.push(DocTagCheckFailure {
                kind: DocTagCheckKind::WordSalad,
                detail: format!(
                    "looks like word_salad (ratio {:.2}, repeated tokens: {})",
                    ratio,
                    repeated.join(", ")
                ),
            });
        }
    }

    failures
}

fn detect_word_salad(
    content: &str,
    symbol_name: &str,
    threshold: f32,
    min_tokens: usize,
) -> Option<(f32, Vec<String>)> {
    let doc_tokens = normalize_doc_tokens(content);
    if doc_tokens.len() < min_tokens {
        return None;
    }

    let symbol_tokens = normalize_symbol_tokens(symbol_name);
    if symbol_tokens.is_empty() {
        return None;
    }

    let mut repeated = Vec::new();
    let mut overlap_count = 0usize;
    for token in doc_tokens.iter() {
        if symbol_tokens.contains(token) {
            overlap_count += 1;
            if !repeated.iter().any(|existing| existing == token) {
                repeated.push(token.clone());
            }
        }
    }

    if overlap_count == 0 {
        return None;
    }

    let ratio = overlap_count as f32 / doc_tokens.len() as f32;
    if ratio > threshold {
        return Some((ratio, repeated));
    }

    None
}

fn normalize_doc_tokens(content: &str) -> Vec<String> {
    tokenize_words(content)
        .into_iter()
        .filter(|token| token.len() >= 3)
        .filter(|token| !is_stopword(token))
        .collect()
}

fn normalize_symbol_tokens(symbol_name: &str) -> Vec<String> {
    tokenize_words(&split_camel_case(symbol_name))
        .into_iter()
        .filter(|token| token.len() >= 3)
        .filter(|token| !is_symbol_noise(token))
        .collect()
}

fn tokenize_words(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn split_camel_case(input: &str) -> String {
    let mut out = String::new();
    let mut prev_is_lower = false;

    for ch in input.chars() {
        if ch == '_' || ch == '-' {
            out.push(' ');
            prev_is_lower = false;
            continue;
        }

        if ch.is_ascii_uppercase() && prev_is_lower {
            out.push(' ');
        }

        out.push(ch);
        prev_is_lower = ch.is_ascii_lowercase();
    }

    out
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "for"
            | "this"
            | "that"
            | "with"
            | "into"
            | "from"
            | "using"
            | "used"
            | "use"
            | "when"
            | "where"
            | "what"
            | "why"
            | "test"
            | "function"
            | "method"
    )
}

fn is_symbol_noise(token: &str) -> bool {
    matches!(
        token,
        "unit" | "unitcore" | "struct" | "structcore" | "macro" | "macrocore" | "test"
    )
}

fn extract_tag_payload(docstring: &str, required_tag: &str) -> String {
    for line in docstring.lines() {
        if let Some((_, payload)) = line.split_once(required_tag) {
            return payload.trim().to_string();
        }
    }

    String::new()
}
