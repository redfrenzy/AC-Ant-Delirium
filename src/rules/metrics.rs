use crate::AuditViolation;
use crate::config::AuditConfig;

/// [WHAT] Verifies line count limits for the geode.
/// [WHY] To keep files atomistic and maintainable.
/// [WHERE] Called during the metrics audit phase of the rule engine.
pub fn verify(source: &str, functions: &[crate::parser::ScannedFunction<'_>], config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    // 1. File-level clean line count
    check_file_limits(source, config, violations);

    // 2. Function-level logical statement count
    check_function_metrics(functions, config, violations);
}

/// [WHAT] Validates the total physical code lines in a file.
/// [WHY] To prevent massive modules that hinder cognitive navigation.
/// [WHERE] File-level audit stage of metrics rule.
fn check_file_limits(source: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    let clean_lines = count_clean_lines(source);
    if clean_lines > config.rules.limits.max_lines && config.rules.limits.max_lines > 0 {
        violations.push(AuditViolation {
            line: 0,
            rule_id: "rule_line_limit",
            message: format!("{} (Found {} clean lines)", config.rules.limits.tip_lines, clean_lines),
            snippet: None,
            quick_fix: None,
        });
    }
}

/// [WHAT] Orchestrates function-level density auditing using CodeScanner.
/// [WHY] To ensure every function remains atomistic and pedagogically pure.
/// [WHERE] Function-level audit stage of metrics rule.
fn check_function_metrics(functions: &[crate::parser::ScannedFunction<'_>], config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    for func in functions {
        verify_single_function_metrics(func, config, violations);
    }
}

/// [WHAT] Validates logical statement density for a specific function.
/// [WHY] Parity with legacy rule_line_limit logic for Elite standard enforcement.
/// [WHERE] Internal checker for per-function metrics.
fn verify_single_function_metrics(
    func: &crate::parser::ScannedFunction<'_>, 
    config: &AuditConfig, 
    violations: &mut Vec<AuditViolation>
) {
    // 1. Logical Density
    let logical_count = count_logical_statements(func);
    let logical_limit = config.rules.limits.logical_density;

    if logical_limit > 0 && logical_count > logical_limit {
        violations.push(AuditViolation {
            line: func.start_line,
            rule_id: "rule_logical_density",
            message: format!(
                "Excessive Logical Density: '{}' has {}/{} statements. Fragment this logic.",
                func.name, logical_count, logical_limit
            ),
            snippet: Some(func.declaration.to_string()),
            quick_fix: Some("// [REFAC] Fragment this function into smaller atomic units to reduce logical mass.".to_string()),
        });
    }

    // 2. Parameter Coupling
    let param_count = count_parameters(&func.declaration);
    let param_limit = config.rules.limits.max_params;

    if param_count > param_limit && param_limit > 0 {
        violations.push(AuditViolation {
            line: func.start_line,
            rule_id: "rule_params",
            message: format!(
                "Acoplamento Excessivo: '{}' tem {}/{} parâmetros. Agrupe-os em uma Struct.",
                func.name, param_count, param_limit
            ),
            snippet: Some(func.declaration.to_string()),
            quick_fix: Some("// [REFAC] Group these parameters into a dedicated Config/State Struct.".to_string()),
        });
    }
}

/// [WHAT] Counts clean code lines (ignoring comments and blanks).
/// [WHY] To provide a normalized complexity metric for Elite geodes.
/// [WHERE] Core utility for metrics calculation and archival health checks.
pub fn count_clean_lines(source: &str) -> usize {
    let mut count = 0;
    let mut in_block_comment = false;
    
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        
        process_line_metric(trimmed, &mut in_block_comment, &mut count);
    }
    count
}

/// [WHAT] Internal helper to process a single line for metrics calculation.
/// [WHY] To keep nesting complexity low and avoid else blocks.
/// [WHERE] Internal metric processing pipeline.
fn process_line_metric(trimmed: &str, in_block: &mut bool, count: &mut usize) {
    if *in_block {
        if trimmed.contains("*/") { *in_block = false; }
        return;
    }
    
    if trimmed.starts_with("//") { return; }
    if trimmed.starts_with("/*") {
        if !trimmed.contains("*/") { *in_block = true; }
        return;
    }
    
    *count += 1;
}

/// [WHAT] Counts logical statements (semicolons and open braces) in a function.
/// [WHY] Logic density is a better complexity metric than physical lines.
/// [WHERE] Core algorithm for function-level metrics.
fn count_logical_statements(func: &crate::parser::ScannedFunction<'_>) -> usize {
    let mut count = 0;
    // We scan the body lines.
    for line in func.body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.is_empty() { continue; }
        
        count += trimmed.matches(';').count();
        
        // Count open braces that are not function declarations
        if trimmed.contains('{') && !trimmed.contains(';') && !trimmed.starts_with("fn ") {
            count += 1;
        }
    }
    count
}

/// [WHAT] Counts the number of parameters in a function declaration.
/// [WHY] Parity with legacy rule_params logic to prevent excessive coupling.
/// [WHERE] Function-level audit stage of metrics rule.
fn count_parameters(declaration: &str) -> usize {
    let (start_idx, end_idx) = match (declaration.find('('), declaration.rfind(')')) {
        (Some(s), Some(e)) => (s, e),
        _ => return 0,
    };

    if end_idx <= start_idx { return 0; }
    let params_str = declaration[start_idx + 1 .. end_idx].trim();
    if params_str.is_empty() { return 0; }

    let mut count = 0;
    let mut depth = 0;
    for c in params_str.chars() {
        if c == '<' { depth += 1; }
        if c == '>' { depth -= 1; }
        if c == ',' && depth == 0 { count += 1; }
    }
    count + 1
}

/// [WHAT] Strips comments from a line to isolate functional code.
/// [WHY] To analyze core code logic without stylistic noise.
/// [WHERE] Utility for various structural and semantic auditing rules.
pub fn stripped_comment(line: &str) -> &str {
    let idx = match line.find("//") {
        Some(i) => i,
        None => return line.trim(),
    };
    &line[..idx].trim()
}
