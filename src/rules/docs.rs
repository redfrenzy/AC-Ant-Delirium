use super::super::AuditViolation;
use super::super::config::AuditConfig;
use super::super::rules_logs::messages;

/// [WHAT] Contexto de auditoria de documentação para redução de acoplamento funcional (rule_params).
pub struct DocsAuditCtx<'a> {
    pub file_path: &'a str,
    pub functions: &'a [super::super::parser::ScannedFunction<'a>],
    pub config: &'a AuditConfig,
    pub violations: &'a mut Vec<AuditViolation>,
}

/// [WHAT] Orchestrates the mandatory tag validation pass for all function docstrings discovered in the source tree to ensure compliance with the Elite Pedagogical standard.
/// [WHY] To ensure that architectural justifications are present and auditable across all components, preventing the accumulation of opaque code that lacks semantic context for AI agents.
/// [WHERE] Semantic audit phase entry point in docs.rs, acting as the primary controller for docstring validation logic within the governance pipeline of the project.
/// [USAGE] Called by audit_pedagogical to analyze the docstrings of all scanned functions sequentially, performing a deep search for required metadata tags as defined in the manifest.
pub fn verify(ctx: &mut DocsAuditCtx<'_>) {
    for func in ctx.functions {
        check_function_docs(ctx, func);
    }
}

/// [WHAT] Individual function pass for mandatory tag detection and density verification that evaluates the pedagogical quality of a specific documentation block discovered by the scanner.
/// [WHY] To ensure that architectural justifications are present and satisfy pedagogical density limits, preventing developers from bypassing the audit via empty or trivial placeholders.
/// [WHERE] Semantic decision layer within the documentation audit engine in docs.rs, providing localized validation for each function declaration encountered during the audit cycle.
/// [USAGE] Invocada pelo motor principal de auditoria pedagógica para validar a conformidade de cada função individualmente, iterando sobre as tags obrigatórias configuradas no delirium.toml.
fn check_function_docs(ctx: &mut DocsAuditCtx<'_>, func: &super::super::parser::ScannedFunction<'_>) {
    for (tag, tag_config) in &ctx.config.rules.doc_tags {
        validate_tag_in_docstring(ctx, func, tag, tag_config);
    }
}

/// [WHAT] Validates the presence and length of a specific tag in a function docstring.
/// [WHY] To enforce pedagogical depth as defined in the governance configuration.
/// [WHERE] Leaf-level verification logic for the documentation engine.
/// [USAGE] Called by check_function_docs for each mandatory tag.
fn validate_tag_in_docstring(
    ctx: &mut DocsAuditCtx<'_>,
    func: &super::super::parser::ScannedFunction<'_>, 
    tag: &str, 
    tag_config: &super::super::config::TagConfig
) {
    let tag_pattern = format!("[{}]", tag);
    
    if !func.docstring.contains(&tag_pattern) {
        let fix_str = format!("/// [WHAT] Write a 256 to 512 character comment IN ENGLISH explaining what the function from line {} to {} does\n/// [WHY] Write a 256 to 512 character comment IN ENGLISH explaining why we created the function from line {} to {}\n/// [WHERE] Write a 256 to 512 character comment IN ENGLISH explaining where or in what situation to use the function from line {} to {}\n", func.start_line, func.end_line, func.start_line, func.end_line, func.start_line, func.end_line);
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: func.start_line,
            rule_id: "rule_doc_tags",
            message: format!("Function '{}' {}: [{}]. Hint: {}", func.name, messages::DOC_TAG_MISSING, tag, tag_config.hint),
            snippet: Some(format!("fn {}(", func.name)), // Simple snippet for signature
            quick_fix: Some(fix_str),
        });
        return;
    }

    check_tag_length(ctx, func, tag, &tag_pattern, tag_config);
}




/// [WHAT] Analyzes the character count of a specific documentation tag.
/// [WHY] To prevent "Elite" documentation from being bypassed with empty or trivial placeholders.
/// [WHERE] Final verification step for a validly identified doc tag.
/// [USAGE] Called by validate_tag_in_docstring after verifying tag presence.
fn check_tag_length(
    ctx: &mut DocsAuditCtx<'_>,
    func: &super::super::parser::ScannedFunction<'_>, 
    tag: &str, 
    pattern: &str, 
    tag_config: &super::super::config::TagConfig
) {
    let content = match func.docstring.split(pattern).last() {
        Some(c) => c,
        None => "",
    };
    let line_part = match content.split('\n').next() {
        Some(l) => l,
        None => "",
    };
    let final_content = line_part.trim();

    if final_content.len() < tag_config.min_chars {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: func.start_line,
            rule_id: "rule_doc_tags",
            message: format!("Function '{}' {}: [{}]. (Found {}, Need {}). Hint: {}", 
                             func.name, messages::DOC_TAG_TOO_SHORT, tag, final_content.len(), tag_config.min_chars, tag_config.hint),
            snippet: Some(format!("/// [{}] {}", tag, final_content)),
            quick_fix: None,
        });
    }
}
