use crate::AuditViolation;
use crate::config::AuditConfig;
use crate::rules_logs::messages;

/// [WHAT] Verifies documentation tags for every function in the source code.
/// [WHY] To ensure that architectural reasoning ([WHY]) and situational usage ([WHERE]) 
///      are documented for every code block, not just for the module.
/// [WHERE] Primary stage of the semantic documentation audit phase.
pub fn verify(functions: &[crate::parser::ScannedFunction<'_>], config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    for func in functions {
        check_function_docs(func, config, violations);
    }
}

/// [WHAT] Orchestrates the tag validation for a specific scanned function.
/// [WHY] To isolate individual function context and provide precise error reporting.
/// [WHERE] Sub-stage of the function-level audit process.
fn check_function_docs(func: &crate::parser::ScannedFunction<'_>, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    for (tag, tag_config) in &config.rules.doc_tags {
        validate_tag_in_docstring(func, tag, tag_config, violations);
    }
}

/// [WHAT] Validates the presence and length of a specific tag in a function docstring.
/// [WHY] To enforce pedagogical depth as defined in the governance configuration.
/// [WHERE] Leaf-level verification logic for the documentation engine.
fn validate_tag_in_docstring(
    func: &crate::parser::ScannedFunction<'_>, 
    tag: &str, 
    tag_config: &crate::config::TagConfig, 
    violations: &mut Vec<AuditViolation>
) {
    let tag_pattern = format!("[{}]", tag);
    
    if !func.docstring.contains(&tag_pattern) {
        let fix_str = format!("/// [WHAT] Write a 256 to 512 character comment IN ENGLISH explaining what the function from line {} to {} does\n/// [WHY] Write a 256 to 512 character comment IN ENGLISH explaining why we created the function from line {} to {}\n/// [WHERE] Write a 256 to 512 character comment IN ENGLISH explaining where or in what situation to use the function from line {} to {}\n", func.start_line, func.end_line, func.start_line, func.end_line, func.start_line, func.end_line);
        violations.push(AuditViolation {
            line: func.start_line,
            rule_id: "rule_doc_tags",
            message: format!("Function '{}' {}: [{}]. Hint: {}", func.name, messages::DOC_TAG_MISSING, tag, tag_config.hint),
            snippet: Some(format!("fn {}(", func.name)), // Simple snippet for signature
            quick_fix: Some(fix_str),
        });
        return;
    }

    check_tag_length(func, tag, &tag_pattern, tag_config, violations);
}




/// [WHAT] Analyzes the character count of a specific documentation tag.
/// [WHY] To prevent "Elite" documentation from being bypassed with empty or trivial placeholders.
/// [WHERE] Final verification step for a validly identified doc tag.
fn check_tag_length(
    func: &crate::parser::ScannedFunction<'_>, 
    tag: &str, 
    pattern: &str, 
    tag_config: &crate::config::TagConfig, 
    violations: &mut Vec<AuditViolation>
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
        violations.push(AuditViolation {
            line: func.start_line,
            rule_id: "rule_doc_tags",
            message: format!("Doc tag too short in '{}' [{}] (Found {}, Need {}). Hint: {}", 
                             func.name, tag, final_content.len(), tag_config.min_chars, tag_config.hint),
            snippet: Some(format!("/// [{}] {}", tag, final_content)),
            quick_fix: None,
        });
    }
}
