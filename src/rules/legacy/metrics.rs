use super::super::AuditViolation;
use super::super::config::{AuditConfig, LimitsConfig};
use super::super::rules_logs::messages;

/// [WHAT] Industrial metrics context used to group function metadata, structural limit configurations, and the violation capture repository.
/// [WHY] Reduces functional coupling (rule_params) by consolidating the audit state, allowing metrics rules to operate independently on massive geodes.
/// [WHERE] Data transport layer for code mass and logical density rule orchestration in metrics.rs.
/// [USAGE] Instantiated at the beginning of the metrics audit process to centralize the validation of line limits, parameters, and logical density per geode.
pub struct MetricsCtx<'a> {
    pub file_path: &'a str,
    pub source: &'a str,
    pub functions: &'a [super::super::parser::ScannedBlock<'a>],
    pub config: &'a AuditConfig,
    pub violations: &'a mut Vec<AuditViolation>,
    pub elite_ready: bool,
    pub detected_dna: String,
    pub taxonomy_path: String,
}

/// [WHAT] Industrial code mass and logical density analyzer that monitors the cyclomatic and structural complexity of each geode in Aesthetic Core.
/// [WHY] To ensure systemic maintainability through the atomization of functions and files, mitigating Cognitive Drift and ensuring code remains auditable by AI agents.
/// [WHERE] Core structural audit layer of the metrics engine in metrics.rs, acting as the primary productivity hygiene sensor for the industrial file tree.
/// [USAGE] Orchestrates the execution of clean line and instruction count rules via grouped context, evaluating whether the operational density of each component respects Elite limits.
pub fn verify(ctx: &mut MetricsCtx<'_>) {
    // [WHAT] Polymorphic and Authorized Resolution: 
    //        1. Attempts to detect DNA by content.
    //        2. Validates if the file is in authorized ground for that DNA.
    //        3. If authorized, applies specific limits. If not (Playground/Drift), falls back to Master Taxonomy Limits.
    let active_limits = ctx.config.hierarchy.resolve_authorized_node_limits(
        ctx.file_path, ctx.source, ctx.elite_ready
    );

    // 1. Function-level logical statement count (com limites polimórficos)
    check_function_metrics(ctx, &active_limits);
}


/// [WHAT] Orchestrates function-level density auditing for a collection of scanned functions discovered within a specific source file during the primary scan phase.
/// [WHY] To ensure every function remains atomistic and pedagogically pure, preventing the formation of large logic blocks that would be difficult to verify or maintain in an industrial environment.
/// [WHERE] Function-level audit stage of metrics rule in metrics.rs, providing a localized evaluation of logical statement counts and parameter coupling for each function signature.
/// [USAGE] Iterates through ScannedBlock objects and applies localized metric rules sequentially, generating diagnostic violations for any component that fails the logical density or parameter count gates.
fn check_function_metrics(ctx: &mut MetricsCtx<'_>, limits: &LimitsConfig) {
    for func in ctx.functions {
        verify_single_function_metrics(ctx, func, limits);
    }
}

/// [WHAT] Validates logical statement density for a specific function discovered by the scanner, calculating the real operational mass of the geode.
/// [WHY] Maintains parity with legacy 'rule_line_limit' logic, forcing atomization of complex functions to ensure Elite industrial auditability.
/// [WHERE] Internal metric checker per function in metrics.rs, acting as the final structural complexity sensor.
/// [USAGE] Invoked iteratively by check_function_metrics to apply logical density and parameter coupling restrictions defined in delirium.toml.
fn verify_single_function_metrics(
    ctx: &mut MetricsCtx<'_>,
    func: &super::super::parser::ScannedBlock<'_>, 
    limits: &LimitsConfig, 
) {
    // 1. Logical Density
    let logical_count = count_logical_statements(func);
    let logical_limit = limits.logical_density;

    if logical_limit > 0 && logical_count > logical_limit {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: func.start_line,
            rule_id: crate::governance::rule_id("hierarchy.metrics.complexity", &func.name),
            message: format!(
                "{} - {}/{} statements. Fragment this logic.",
                messages::LOGICAL_DENSITY, logical_count, logical_limit
            ),
            snippet: Some(func.declaration.to_string()),
            quick_fix: Some("// [REFAC] Fragment this function into smaller atomic units to reduce logical mass.".to_string()),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: format!("{}.limits.logical_density", ctx.taxonomy_path),
        });
    }

    // 2. Parameter Coupling
    let param_count = count_parameters(&func.declaration);
    let param_limit = limits.max_params;

    if param_count > param_limit && param_limit > 0 {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: func.start_line,
            rule_id: crate::governance::rule_id("hierarchy.metrics.params", &func.name),
            message: format!(
                "{} - {}/{} parameters. Group them into a Struct.",
                messages::PARAMS_TOO_MANY, param_count, param_limit
            ),
            snippet: Some(func.declaration.to_string()),
            quick_fix: Some("// [REFAC] Group these parameters into a dedicated Config/State Struct.".to_string()),
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: format!("{}.limits.max_params", ctx.taxonomy_path),
        });
    }

}

/// [WHAT] Functional mass normalization algorithm based on operational lines of code for Aesthetic Core Elite compliance.
/// [WHY] To provide a complexity metric agnostic to formatting styles and purely pedagogical comment blocks, ensuring audit objectivity.
/// [WHERE] Central industrial density calculation utility in the ac_ant_delirium project controlled by the structural metrics engine.
/// [USAGE] Used by the rule_line_limit and by semantic manifesto generators to determine the real functional load and operational density of the entire geode.
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

/// [WHAT] Line state analyzer for comment and functional space filtering that monitors lexical scope transitions during audit.
/// [WHY] To ensure that code mass calculation ignores non-operational artifacts without violating nesting restrictions or industrial gatekeeper rigor.
/// [WHERE] Atomic component of the metrics pipeline in metrics.rs, responsible for lexical sanitization of each scanned line in the system.
/// [USAGE] Iteratively invoked by the counting engine to update the scan state and real density counter, ensuring integrity of collected data.
fn process_line_metric(trimmed: &str, in_block: &mut bool, count: &mut usize) {
    if check_in_block(trimmed, in_block) { return; }
    
    if trimmed.starts_with("//") { return; }
    if trimmed.starts_with("/*") {
        *in_block = !trimmed.contains("*/");
        return;
    }
    
    *count += 1;
}

/// [WHAT] Evaluates if the scanner is currently inside a block comment and updates the state if a terminator is found.
/// [WHY] To satisfy Elite nesting limits by isolating state transitions for multi-line comments.
/// [WHERE] Lexical utility of the metrics engine in metrics.rs.
/// [USAGE] Internally called by process_line_metric to validate the audit cursor state relative to comment blocks, ensuring that the logical density counter ignores non-operational segments without exceeding cognitive complexity.
fn check_in_block(trimmed: &str, in_block: &mut bool) -> bool {
    if !*in_block { return false; }
    if trimmed.contains("*/") { *in_block = false; }
    true
}

/// [WHAT] Logical statement extraction algorithm based on termination and scope tokens.
/// [WHY] Because statement density is a more accurate indicator of cognitive load than pure line count.
/// [WHERE] Core implementation of functional metrics in metrics.rs.
/// [USAGE] Consulted by the density checker to validate whether a function should be fragmented by complexity based on logical instruction counts and structural control flows.
fn count_logical_statements(func: &super::super::parser::ScannedBlock<'_>) -> usize {
    let mut count = 0;
    // We scan the body lines.
    for line in func.body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.is_empty() { continue; }
        
        count += trimmed.matches(';').count();
        
        // Count open braces that are not function declarations
        if trimmed.contains('\x7b') && !trimmed.contains(';') && !trimmed.starts_with("fn ") {
            count += 1;
        }
    }
    count + 1
}

/// [WHAT] Counts the number of parameters in a function declaration string using a depth-aware scanner.
/// [WHY] To enforce architectural constraints on function coupling and nesting.
/// [WHERE] Function-level audit stage of metrics rule in metrics.rs.
/// [USAGE] Called by verify_single_function_metrics for signature analysis to ensure function coupling stays within industrial limits of parameters and architectural complexity.
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

/// [WHAT] Legacy sentinel function intentionally exceeding parameter limits for governance validation.
/// [WHY] Provides a stable fixture so the new MaxParamsAxiom can prove it detects excessive coupling in legacy modules.
/// [WHERE] metrics.rs legacy rule module, outside the new axiom folders.
/// [USAGE] Exercised indirectly by structural test sweeps to confirm parameter-count violations are surfaced.
fn legacy_parameter_overload_probe(
    alpha: usize,
    beta: usize,
    gamma: usize,
    delta: usize,
    epsilon: usize,
    zeta: usize,
    eta: usize,
    theta: usize,
    iota: usize,
    kappa: usize,
) -> usize {
    alpha + beta + gamma + delta + epsilon + zeta + eta + theta + iota + kappa
}
