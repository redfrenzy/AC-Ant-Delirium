use crate::AuditViolation;
use crate::config::AuditConfig;

/// [WHAT] Verifies structural constraints such as nesting depth and else-free logic.
/// [WHY] To maintain cognitive clarity and linear code flow.
/// [WHERE] Called during the structural audit phase of the rule engine.
pub fn verify(_path: &str, source: &str, functions: &[crate::parser::ScannedFunction<'_>], config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    for (idx, line) in source.lines().enumerate() {
        process_structure(idx, line, config, violations);
    }
    check_params_on_sigs(functions, config, violations);
}

/// [WHAT] Processes structural rules for a single line of code.
/// [WHY] To keep nesting depth low and maintain 'Elite' standards.
/// [WHERE] Internal utility for structural code auditing.
fn process_structure(idx: usize, line: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    let trimmed = line.trim();
    if trimmed.is_empty() { return; }

    // 1. Else-free check (Brute Parity)
    let clean_line = strip_strings(trimmed);
    
    if contains_word(&clean_line, "else") {
        check_else_bypass(idx, trimmed, violations);
    }

    // 2. Abismo Cognitivo check (Nesting Parity)
    check_nesting_depth(idx, line, config, violations);
}

/// [WHAT] Verifies the presence of a legitimate bypass tag for an else block.
/// [WHY] To allow for explicit, reasoned exceptions to the else-free mandate.
/// [WHERE] Internal guard for the else-free auditing rule.
fn check_else_bypass(idx: usize, trimmed: &str, violations: &mut Vec<AuditViolation>) {
    if trimmed.contains("// [REASON]") || trimmed.contains("// [") {
        return;
    }

    violations.push(AuditViolation {
        line: idx + 1,
        rule_id: "rule_else_free",
        message: "Else block detected. Use early returns or match for linear flow. Bypass with '// [REASON]' if necessary.".to_string(),
        snippet: Some(trimmed.to_string()),
        quick_fix: Some("// [REASON] Justify this else block here...".to_string()),
    });
}

/// [WHAT] Implements the 'Abismo Cognitivo' check based on indentation level.
/// [WHY] To satisfy structural governance rules and prevent logic spaghetti nesting.
/// [WHERE] Internal utility for structural code auditing.
fn check_nesting_depth(idx: usize, line: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") { return; }

    // 1. Keyword Filter (Only logical statements trigger depth)
    if !is_logical_stmt(line) { return; }

    // 2. Indentation Logic (Level = Indent / 4)
    let indent = line.len() - line.trim_start().len();
    let level = indent / 4;
    let limit = config.rules.limits.max_nesting;

    if level > limit && limit > 0 {
        violations.push(AuditViolation {
            line: idx + 1,
            rule_id: "rule_nesting",
            message: format!("Abismo Cognitivo: Profundidade de nidificação excessiva (Nível {} > {}). Extraia sub-blocos.", level, limit),
            snippet: Some(line.to_string()),
            quick_fix: Some("// [REFAC] Extract this nested block into a private helper function.".to_string()),
        });
    }
}

/// [WHAT] Identifies lines that should trigger a nesting depth check.
/// [WHY] To avoid punishing formatting or long strings.
/// [WHERE] Diagnostic filter for the nesting rule.
fn is_logical_stmt(line: &str) -> bool {
    line.contains("if ") || 
    line.contains("else {") || 
    line.contains("for ") || 
    line.contains("match ") || 
    line.contains("let ")
}

/// [WHAT] Performs a global scan of the source code to identify and validate function signatures against parameter count limits. It utilizes a specialized parsing logic that identifies the 'fn' keyword, extracts the parameter list contained within parentheses, and calculates the count while accounting for complex generic types.
/// [WHY] To decouple parameter validation from line-by-line structural checks, allowing for a more robust analysis of multi-line function signatures. By enforcing a strict limit on the number of parameters, the engine prevents 'Excessive Coupling' and encourages the use of structured data architectures, which is essential for maintaining the clean, modular interfaces required in Elite geode development.
/// [WHERE] Executed as a secondary pass within the `verify` method of `structure.rs` after the initial line-level structural auditing is complete.
fn check_params_on_sigs(functions: &[crate::parser::ScannedFunction<'_>], config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    let limit = config.rules.limits.max_params;
    if limit == 0 { return; }

    for func in functions {
        let param_count = count_params_brute(func.declaration);
        if param_count > limit {
            violations.push(AuditViolation {
                line: func.start_line,
                rule_id: "rule_params",
                message: format!("Acoplamento Excessivo: Função possui {} parâmetros (limite: {}). Agrupe em uma Struct.", param_count, limit),
                snippet: Some(func.declaration.to_string()),
                quick_fix: Some("// [REFAC] Group these parameters into a dedicated Config/State Struct.".to_string()),
            });
        }
    }
}

/// [WHAT] Manually calculates the number of parameters in a raw function signature string by parsing the content within the first set of parentheses. It handles comma-separated entries while correctly ignoring commas nested within generic type declarations (e.g., `<K, V>`) to ensure an accurate count of top-level function arguments.
/// [WHY] To provide a reliable, zero-dependency mechanism for measuring interface complexity that is compatible with the project's 'brute-force' parsing philosophy. This approach avoids the need for external AST parsers while providing the level of accuracy required for architectural governance in a dependency-free environment.
/// [WHERE] Internal helper utility used by the parameter validation pass to process individual function signature lines.
fn count_params_brute(signature: &str) -> usize {
    let start = match signature.find('(') { Some(i) => i, None => return 0 };
    let end = match signature[start..].find(')') { Some(i) => start + i, None => return 0 };
    
    let inner = signature[start + 1 .. end].trim();
    if inner.is_empty() { return 0; }

    let mut count = 1;
    let mut depth = 0;
    for c in inner.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

/// [WHAT] Parses away quotation marks dynamically without else blocks.
/// [WHY] To prevent false positives for 'else' keywords inside strings.
/// [WHERE] Pre-processor for structural rule evaluation.
fn strip_strings(s: &str) -> String {
    let mut result = String::new();
    let mut in_string = false;
    
    for c in s.chars() {
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        
        if !in_string {
            result.push(c);
        }
    }
    result
}

/// [WHAT] Checks strict word constraints without regex.
/// [WHY] To ensure lexical precision without external dependencies.
/// [WHERE] Alphanumeric filtering utility for keywords.
fn contains_word(s: &str, word: &str) -> bool {
    s.split(|c: char| !c.is_alphanumeric() && c != '_')
     .any(|p| p == word)
}
