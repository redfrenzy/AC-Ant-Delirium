use crate::governance;
use crate::structs::{AuditContext, AuditViolation};

pub fn patrol(ctx: &mut AuditContext<'_>) {
    if should_skip_type_logic(ctx.file_path, &ctx.detected_dna) {
        return;
    }

    let requirement = match ctx.require_named_taxonomy_axiom_parameter("type_logic", "type_logic") {
        Ok(value) => value,
        Err(message) => {
            push_resolution_violation(ctx, "type_logic", message);
            return;
        }
    };
    let resolved_from = requirement.resolved_path;
    let resolved_value = requirement.value;

    let allowed_logic = parse_industrial_array(&resolved_value);
    if allowed_logic.is_empty() {
        push_resolution_violation(
            ctx,
            "type_logic",
            "type_logic resolved to an empty allow-list.".to_string(),
        );
        return;
    }

    let allowed_profiles = normalize_allowed_profiles(&allowed_logic);
    if allowed_profiles.is_empty() {
        push_resolution_violation(
            ctx,
            "type_logic",
            format!(
                "type_logic resolved at '{}' but none of the profiles {:?} are recognized.",
                resolved_from, allowed_logic
            ),
        );
        return;
    }

    for block in ctx.functions {
        if block.kind == "_module" {
            continue;
        }

        let detected_profile = classify_logic_profile(block.declaration, &block.kind);
        if allowed_profiles.contains(&detected_profile) {
            continue;
        }

        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: block.start_line,
            rule_id: governance::rule_id(&requirement.identity_path, &block.name),
            message: format!(
                "Type Logic Violation: DNA '{}' only allows {:?}, but found '{}' in declaration.",
                ctx.detected_dna,
                allowed_profiles,
                detected_profile
            ),
            snippet: Some(block.declaration.trim().to_string()),
            quick_fix: Some(format!(
                "// [TODO] Align this declaration with the '{}' type_logic profile resolved from {}.",
                ctx.detected_dna,
                resolved_from
            )),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: format!("{}.{}", resolved_from, requirement.requirement_id),
        });
    }
}

fn should_skip_type_logic(file_path: &str, detected_dna: &str) -> bool {
    if !file_path.ends_with(".rs") {
        return true;
    }

    detected_dna.trim().is_empty()
}

fn parse_industrial_array(input: &str) -> Vec<String> {
    let trimmed = input.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed[1..trimmed.len() - 1]
            .split(',')
            .map(|value| value.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|value| !value.is_empty())
            .collect()
    } else {
        vec![trimmed.to_string()]
    }
}

fn normalize_allowed_profiles(allowed_logic: &[String]) -> Vec<String> {
    let mut profiles = Vec::new();
    for entry in allowed_logic {
        let normalized = normalize_profile(entry);
        if !normalized.is_empty() && !profiles.contains(&normalized) {
            profiles.push(normalized);
        }
    }
    profiles
}

fn normalize_profile(entry: &str) -> String {
    let normalized = entry.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "#[test]" | "#[tokio::test]" | "#[cfg(test)]" => "test_fn".to_string(),
        "pub(crate) async fn" => "crate_async_fn".to_string(),
        "pub(crate) const fn" => "crate_const_fn".to_string(),
        "pub(crate) unsafe fn" => "crate_unsafe_fn".to_string(),
        "pub(crate) fn" => "crate_fn".to_string(),
        "pub async fn" => "public_async_fn".to_string(),
        "pub const fn" => "public_const_fn".to_string(),
        "pub unsafe fn" => "public_unsafe_fn".to_string(),
        "pub fn" => "public_fn".to_string(),
        "async fn" => "private_async_fn".to_string(),
        "const fn" => "private_const_fn".to_string(),
        "unsafe fn" => "private_unsafe_fn".to_string(),
        "fn" => "private_fn".to_string(),
        "pub struct" => "public_struct".to_string(),
        "struct" => "struct".to_string(),
        "pub unit struct" => "public_unit_struct".to_string(),
        "unit struct" => "unit_struct".to_string(),
        "pub tuple struct" => "public_tuple_struct".to_string(),
        "tuple struct" => "tuple_struct".to_string(),
        "pub nested struct" => "public_nested_struct".to_string(),
        "nested struct" => "nested_struct".to_string(),
        "pub inline generic struct" => "public_inline_generic_struct".to_string(),
        "inline generic struct" => "inline_generic_struct".to_string(),
        "pub enum" => "public_enum".to_string(),
        "enum" => "enum".to_string(),
        "trait" => "trait".to_string(),
        "impl" => "impl".to_string(),
        "macro_rules!" => "macro_rules".to_string(),
        "pub macro" => "public_macro".to_string(),
        _ => String::new(),
    }
}

fn classify_logic_profile(declaration: &str, kind: &str) -> String {
    let normalized = declaration.trim().to_ascii_lowercase();

    if normalized.contains("#[test]") || normalized.contains("#[tokio::test]") || normalized.contains("#[cfg(test)]") {
        return "test_fn".to_string();
    }

    match kind {
        "function" => classify_function_profile(&normalized),
        "struct" => classify_struct_profile(declaration, &normalized),
        "enum" => {
            if normalized.starts_with("pub enum ") {
                "public_enum".to_string()
            } else {
                "enum".to_string()
            }
        }
        "trait" => "trait".to_string(),
        "impl" => "impl".to_string(),
        "macro" => {
            if normalized.starts_with("pub macro ") {
                "public_macro".to_string()
            } else {
                "macro_rules".to_string()
            }
        }
        _ => {
            if normalized.starts_with("impl ") || normalized.starts_with("pub impl ") {
                "impl".to_string()
            } else {
                "unknown".to_string()
            }
        }
    }
}

fn classify_struct_profile(raw_declaration: &str, normalized: &str) -> String {
    let is_public = normalized.starts_with("pub struct ");
    let is_nested = raw_declaration.starts_with(' ') || raw_declaration.starts_with('\t');
    let is_tuple = normalized.ends_with(';') && normalized.contains('(') && normalized.contains(')') && !normalized.contains('{');
    let is_unit = normalized.ends_with(';') && !normalized.contains('(') && !normalized.contains('{');
    let is_inline_generic = normalized.contains('<')
        && normalized.contains(':')
        && normalized.contains('>')
        && !normalized.contains(" where ")
        && !normalized.contains("const ");

    if is_nested {
        return if is_public {
            "public_nested_struct".to_string()
        } else {
            "nested_struct".to_string()
        };
    }

    if is_tuple {
        return if is_public {
            "public_tuple_struct".to_string()
        } else {
            "tuple_struct".to_string()
        };
    }

    if is_unit {
        return if is_public {
            "public_unit_struct".to_string()
        } else {
            "unit_struct".to_string()
        };
    }

    if is_inline_generic {
        return if is_public {
            "public_inline_generic_struct".to_string()
        } else {
            "inline_generic_struct".to_string()
        };
    }

    if is_public {
        "public_struct".to_string()
    } else {
        "struct".to_string()
    }
}

fn classify_function_profile(normalized: &str) -> String {
    if normalized.starts_with("pub(crate) async fn ") {
        return "crate_async_fn".to_string();
    }
    if normalized.starts_with("pub(crate) const fn ") {
        return "crate_const_fn".to_string();
    }
    if normalized.starts_with("pub(crate) unsafe fn ") {
        return "crate_unsafe_fn".to_string();
    }
    if normalized.starts_with("pub(crate) fn ") {
        return "crate_fn".to_string();
    }
    if normalized.starts_with("pub async fn ") {
        return "public_async_fn".to_string();
    }
    if normalized.starts_with("pub const fn ") {
        return "public_const_fn".to_string();
    }
    if normalized.starts_with("pub unsafe fn ") {
        return "public_unsafe_fn".to_string();
    }
    if normalized.starts_with("pub fn ") {
        return "public_fn".to_string();
    }
    if normalized.starts_with("async fn ") {
        return "private_async_fn".to_string();
    }
    if normalized.starts_with("const fn ") {
        return "private_const_fn".to_string();
    }
    if normalized.starts_with("unsafe fn ") {
        return "private_unsafe_fn".to_string();
    }
    if normalized.starts_with("fn ") {
        return "private_fn".to_string();
    }
    "unknown".to_string()
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
