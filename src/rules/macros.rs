use crate::AuditViolation;
use crate::config::AuditConfig;
use crate::rules_logs::messages;

/// [WHAT] Verifies macro isolation and prevents complex logic within macros.
/// [WHY] To keep macros simple and avoid un-auditable opaque logic.
/// [WHERE] Called during the structural audit phase of the rule engine.
pub fn verify(source: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    let mut in_macro = false;
    let mut macro_start_line = 0;
    let mut macro_brace_depth = 0;
    let mut macro_lines = 0;
    let mut has_complex_logic = false;

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        
        // Detection
        if !in_macro && trimmed.contains("macro_rules!") {
            in_macro = true;
            macro_start_line = idx + 1;
            macro_brace_depth = 0;
            macro_lines = 0;
            has_complex_logic = false;
        }

        if !in_macro { continue; }
        
        process_macro_line(trimmed, config, &mut has_complex_logic, &mut macro_brace_depth, &mut macro_lines);

        if macro_brace_depth == 0 && trimmed.contains('}') {
            analyze_macro(macro_start_line, macro_lines, has_complex_logic, config, violations);
            in_macro = false;
        }
    }
}

/// [WHAT] Processes a single line within a macro block.
/// [WHY] To keep 'verify' nesting depth at 3.
/// [WHERE] Internal utility for structural macro auditing.
fn process_macro_line(trimmed: &str, config: &AuditConfig, complex: &mut bool, depth: &mut i32, lines: &mut usize) {
    *lines += 1;
    
    // Logic complexity check
    if !config.rules.macros.allow_complex_logic {
        check_complex_tokens(trimmed, complex);
    }

    // Structural tracking
    for c in trimmed.chars() {
        if c == '{' { *depth += 1; }
        if c == '}' && *depth > 0 { *depth -= 1; }
    }
}

/// [WHAT] Helper to check for forbidden logic tokens within a macro.
/// [WHY] To avoid deep nesting and maintain linear auditing flow.
/// [WHERE] Internal utility for structural macro auditing.
fn check_complex_tokens(trimmed: &str, complex: &mut bool) {
    let forbidden = ["if ", "match ", "while ", "for ", "loop "];
    for token in &forbidden {
        if trimmed.contains(token) {
            *complex = true;
        }
    }
}

/// [WHAT] Analyzes a completed macro block for violations.
/// [WHY] To keep logic flat and maintain 'Elite' standards.
/// [WHERE] Internal utility for structural macro auditing.
fn analyze_macro(start_line: usize, lines: usize, complex: bool, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    if lines > config.rules.macros.max_lines && config.rules.macros.max_lines > 0 {
        violations.push(AuditViolation {
            line: start_line,
            rule_id: "rule_macro_isolation",
            message: format!("{} (Lines: {}). Max: {}. Hint: {}", 
                             messages::MACRO_LENGTH, lines, config.rules.macros.max_lines, config.rules.macros.tip),
            snippet: None,
            quick_fix: None,
        });
    }

    if complex {
        violations.push(AuditViolation {
            line: start_line,
            rule_id: "rule_macro_isolation",
            message: format!("{}. Hint: {}", messages::MACRO_COMPLEXITY, config.rules.macros.tip),
            snippet: None,
            quick_fix: None,
        });
    }
}
