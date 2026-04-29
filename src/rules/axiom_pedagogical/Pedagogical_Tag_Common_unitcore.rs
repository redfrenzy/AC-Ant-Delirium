#![allow(dead_code)]

use crate::structs::AuditContext;

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentationTagContract {
    pub documentation_path: String,
    pub tag_node_path: Option<String>,
    pub tag_name: String,
    pub min_chars: Option<usize>,
    pub info: Option<String>,
    pub word_salad_enabled: bool,
    pub word_salad_threshold: f32,
    pub word_salad_min_tokens: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DocumentationTagResolution {
    Active(DocumentationTagContract),
    DisabledByBoundary(String),
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentationTagCheck {
    pub tag_key: String,
    pub required_tag: String,
    pub human_name: String,
    pub contract: DocumentationTagContract,
}

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

pub fn resolve_doc_tags_aggregator_enabled(ctx: &AuditContext<'_>) -> Result<bool, String> {
    match ctx
        .config
        .hierarchy
        .resolve_taxonomy_parameter(&ctx.taxonomy_path, "doc_tags")
    {
        Ok((_, value)) => Ok(parse_bool_like(&value)),
        Err(_) => match ctx.config.hierarchy.resolve_parameter_binding("doc_tags") {
            Some((_, value)) => Ok(parse_bool_like(&value)),
            None => Ok(false),
        },
    }
}

pub fn resolve_rule_enabled(
    ctx: &AuditContext<'_>,
    axiom_id: &str,
    requirement_id: &str,
    documentation_key: &str,
) -> Result<bool, String> {
    match resolve_documentation_tag_resolution(ctx, documentation_key) {
        DocumentationTagResolution::Active(_) => return Ok(true),
        DocumentationTagResolution::DisabledByBoundary(_) => return Ok(false),
        DocumentationTagResolution::Missing => {}
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
    let tag_key = required_tag
        .trim_matches(|ch| ch == '[' || ch == ']')
        .to_ascii_lowercase();
    let contract = match resolve_documentation_tag_resolution(ctx, &tag_key) {
        DocumentationTagResolution::Active(contract) => Some(contract),
        DocumentationTagResolution::DisabledByBoundary(_) | DocumentationTagResolution::Missing => {
            None
        }
    };

    for func in ctx.functions.iter().filter(|func| func.kind == "function") {
        if func.docstring.to_ascii_lowercase().contains(&required) {
            if let Some(min_chars) = contract.as_ref().and_then(|contract| contract.min_chars) {
                let content = extract_tag_payload(&func.docstring, required_tag);
                if content.chars().count() >= min_chars {
                    continue;
                }

                let detail = contract
                    .as_ref()
                    .and_then(|contract| contract.info.as_deref())
                    .unwrap_or("Expand the pedagogical payload for this tag.");

                ctx.violations.push(crate::structs::AuditViolation {
                    file_path: ctx.file_path.to_string(),
                    line: func.start_line,
                    rule_id: identity_path.clone(),
                    message: format!(
                        "{} violation: function '{}' includes {}, but the tag content is too short (found {}, need {}). {}",
                        human_name,
                        func.name,
                        required_tag,
                        content.chars().count(),
                        min_chars,
                        detail
                    ),
                    snippet: Some(format!("{} {}", required_tag, content)),
                    quick_fix: Some(format!("/// {} {}", required_tag, detail)),
                    detected_dna: ctx.detected_dna.clone(),
                    taxonomy_path: identity_path.clone(),
                });
            }
            continue;
        }

        let quick_fix = contract
            .as_ref()
            .and_then(|contract| contract.info.as_deref())
            .map(|info| format!("/// {} {}", required_tag, info))
            .unwrap_or_else(|| format!("/// {} ...", required_tag));

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
            quick_fix: Some(quick_fix),
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

pub fn resolve_documentation_tag_resolution(
    ctx: &AuditContext<'_>,
    key: &str,
) -> DocumentationTagResolution {
    let mut current = ctx.taxonomy_path.trim().trim_matches('.').to_string();
    let normalized_key = key.trim().trim_matches('.').to_ascii_lowercase();
    let legacy_key = normalized_key.to_ascii_uppercase();

    while !current.is_empty() {
        let documentation_path = format!("{}.documentation", current);
        if let Some(documentation_node) = ctx.config.hierarchy.nodes.get(&documentation_path) {
            let tag_node_path = format!("{}.{}", documentation_path, normalized_key);
            if let Some(tag_node) = ctx.config.hierarchy.nodes.get(&tag_node_path) {
                let min_chars = tag_node
                    .properties
                    .get("min")
                    .and_then(|value| value.trim().parse::<usize>().ok());
                let info = tag_node.properties.get("info").cloned();
                let word_salad_enabled = tag_node
                    .properties
                    .get("word_salad")
                    .map(|value| parse_bool_like(value))
                    .unwrap_or(true);
                let word_salad_threshold = tag_node
                    .properties
                    .get("word_salad_threshold")
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .unwrap_or(0.8);
                let word_salad_min_tokens = tag_node
                    .properties
                    .get("word_salad_min_tokens")
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(4);
                return DocumentationTagResolution::Active(DocumentationTagContract {
                    documentation_path,
                    tag_node_path: Some(tag_node_path),
                    tag_name: legacy_key,
                    min_chars,
                    info,
                    word_salad_enabled,
                    word_salad_threshold,
                    word_salad_min_tokens,
                });
            }

            if let Some(value) = documentation_node.properties.get(&legacy_key) {
                if !parse_bool_like(value) {
                    return DocumentationTagResolution::DisabledByBoundary(documentation_path);
                }

                return DocumentationTagResolution::Active(DocumentationTagContract {
                    documentation_path,
                    tag_node_path: None,
                    tag_name: legacy_key,
                    min_chars: None,
                    info: None,
                    word_salad_enabled: true,
                    word_salad_threshold: 0.8,
                    word_salad_min_tokens: 4,
                });
            }

            if let Some(value) = documentation_node.properties.get(&normalized_key) {
                if !parse_bool_like(value) {
                    return DocumentationTagResolution::DisabledByBoundary(documentation_path);
                }

                return DocumentationTagResolution::Active(DocumentationTagContract {
                    documentation_path,
                    tag_node_path: None,
                    tag_name: legacy_key,
                    min_chars: None,
                    info: None,
                    word_salad_enabled: true,
                    word_salad_threshold: 0.8,
                    word_salad_min_tokens: 4,
                });
            }

            return DocumentationTagResolution::DisabledByBoundary(documentation_path);
        }

        match current.rsplit_once('.') {
            Some((parent, _)) => current = parent.to_string(),
            None => break,
        }
    }

    DocumentationTagResolution::Missing
}
fn extract_tag_payload(docstring: &str, required_tag: &str) -> String {
    for line in docstring.lines() {
        if let Some((_, payload)) = line.split_once(required_tag) {
            return payload.trim().to_string();
        }
    }

    String::new()
}

pub fn collect_supported_doc_tag_checks(ctx: &AuditContext<'_>) -> Vec<DocumentationTagCheck> {
    let mut checks = Vec::new();
    for (tag_key, required_tag, human_name) in [
        ("what", "[WHAT]", "What mission"),
        ("why", "[WHY]", "Why justification"),
        ("usage", "[USAGE]", "Usage example"),
        ("test", "[TEST]", "Test mapping"),
    ] {
        if let DocumentationTagResolution::Active(contract) =
            resolve_documentation_tag_resolution(ctx, tag_key)
        {
            checks.push(DocumentationTagCheck {
                tag_key: tag_key.to_string(),
                required_tag: required_tag.to_string(),
                human_name: human_name.to_string(),
                contract,
            });
        }
    }
    checks
}
