use super::super::AuditViolation;
use super::super::config::AuditConfig;
use super::super::rules_logs::messages;

/// [WHAT] Contexto de métricas industriais para redução de acoplamento.
pub struct MetricsCtx<'a> {
    pub file_path: &'a str,
    pub source: &'a str,
    pub functions: &'a [super::super::parser::ScannedFunction<'a>],
    pub config: &'a AuditConfig,
    pub violations: &'a mut Vec<AuditViolation>,
}

/// [WHAT] Analisador industrial de limites de massa de código e densidade lógica que monitora a complexidade ciclomática e estrutural de cada geode no Aesthetic Core.
/// [WHY] Para garantir a manutenibilidade sistêmica através da atomização de funções e arquivos, mitigando o Cognitive Drift e assegurando que o código permaneça auditável por agentes de IA.
/// [WHERE] Camada de auditoria estrutural core do motor de métricas em metrics.rs, atuando como o sensor primário de higiene produtiva para a árvore de arquivos industriais.
/// [USAGE] Orquestra a execução das regras de contagem de linhas limpas e instruções por via de contexto agrupado, avaliando se a densidade operacional de cada componente respeita os limites Elite.
pub fn verify(ctx: &mut MetricsCtx<'_>) {
    // 1. File-level clean line count
    check_file_limits(ctx.file_path, ctx.source, ctx.config, ctx.violations);

    // 2. Function-level logical statement count
    check_function_metrics(ctx.file_path, ctx.functions, ctx.config, ctx.violations);
}

/// [WHAT] Verificador de limites de linhas limpas para conformidade de massa de código que avalia o tamanho físico dos arquivos .rs ignorando blocos de comentários e espaços brancos.
/// [WHY] Para prevenir módulos massivos que degradam a eficiência de auditoria e navegação cognitiva de ferramentas de IA, forçando a modularização de componentes complexos conforme as normas Elite.
/// [WHERE] Estágio de auditoria de nível de arquivo em metrics.rs, integrado ao pipeline de métricas funcionais para garantir que a carga cognitiva por arquivo permaneça sob controle industrial.
/// [USAGE] Invocada pelo orquestrador de métricas (verify) para assegurar que a massa total do arquivo não exceda os limites de auditabilidade industrial definidos no manifesto de governança delirium.toml.
fn check_file_limits(file_path: &str, source: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    let clean_lines = count_clean_lines(source);
    if clean_lines > config.rules.limits.max_lines && config.rules.limits.max_lines > 0 {
        violations.push(AuditViolation {
            file_path: file_path.to_string(),
            line: 0,
            rule_id: crate::governance::rule_id_atomic("rule_line_limit"),
            message: format!("{} (Found {} clean lines)", messages::LINE_LIMIT_EXCEEDED, clean_lines),
            snippet: None,
            quick_fix: None,
        });
    }
}

/// [WHAT] Orchestrates function-level density auditing for a collection of scanned functions discovered within a specific source file during the primary scan phase.
/// [WHY] To ensure every function remains atomistic and pedagogically pure, preventing the formation of large logic blocks that would be difficult to verify or maintain in an industrial environment.
/// [WHERE] Function-level audit stage of metrics rule in metrics.rs, providing a localized evaluation of logical statement counts and parameter coupling for each function signature.
/// [USAGE] Iterates through ScannedFunction objects and applies localized metric rules sequentially, generating diagnostic violations for any component that fails the logical density or parameter count gates.
fn check_function_metrics(file_path: &str, functions: &[super::super::parser::ScannedFunction<'_>], config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    for func in functions {
        verify_single_function_metrics(file_path, func, config, violations);
    }
}

/// [WHAT] Validates logical statement density for a specific function.
/// [WHY] Parity with legacy rule_line_limit logic for Elite standard enforcement.
/// [WHERE] Internal checker for per-function metrics.
fn verify_single_function_metrics(
    file_path: &str,
    func: &super::super::parser::ScannedFunction<'_>, 
    config: &AuditConfig, 
    violations: &mut Vec<AuditViolation>
) {
    // 1. Logical Density
    let logical_count = count_logical_statements(func);
    let logical_limit = config.rules.limits.logical_density;

    if logical_limit > 0 && logical_count > logical_limit {
        violations.push(AuditViolation {
            file_path: file_path.to_string(),
            line: func.start_line,
            rule_id: crate::governance::rule_id("rule_logical_density", &func.name),
            message: format!(
                "{} - {}/{} statements. Fragment this logic.",
                messages::LOGICAL_DENSITY, logical_count, logical_limit
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
            file_path: file_path.to_string(),
            line: func.start_line,
            rule_id: crate::governance::rule_id("rule_params", &func.name),
            message: format!(
                "{} - {}/{} parâmetros. Agrupe-os em uma Struct.",
                messages::PARAMS_TOO_MANY, param_count, param_limit
            ),
            snippet: Some(func.declaration.to_string()),
            quick_fix: Some("// [REFAC] Group these parameters into a dedicated Config/State Struct.".to_string()),
        });
    }
}

/// [WHAT] Algoritmo de normalização de massa funcional baseado em linhas de código operacionais para conformidade Elite Aesthetic Core.
/// [WHY] Para fornecer uma métrica de complexidade agnóstica a estilos de formatação e blocos de comentários puramente pedagógicos, assegurando objetividade na auditoria.
/// [WHERE] Utilitário central de cálculo de densidade industrial no projeto ac_ant_delirium controlado pelo motor de métricas estruturais.
/// [USAGE] Utilizada pela regra rule_line_limit e pelos geradores de manifestos semânticos para determinar a carga funcional real e a densidade operacional de todo o geode.
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

/// [WHAT] Analisador de estado de linha para filtragem de comentários e espaços funcionais que monitora transições de blocos lexicais durante a auditoria.
/// [WHY] Para garantir que o cálculo de massa de código ignore artefatos não-operacionais sem violar restrições de nidificação ou o rigor do gatekeeper industrial.
/// [WHERE] Componente atômico do pipeline de métricas em metrics.rs, responsável pela higienização léxica de cada linha de código scanned no sistema.
/// [USAGE] Invocado de forma iterativa pelo motor de contagem para atualizar o estado de varredura e o contador de densidade real, garantindo a integridade dos dados coletados.
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

/// [WHAT] Algoritmo de extração de densidade lógica baseado em tokens de terminação e escopo.
/// [WHY] Porque a densidade de instruções é um indicador de carga cognitiva mais preciso do que a contagem pura de linhas.
/// [WHERE] Implementação núcleo de métricas funcionais em metrics.rs.
/// [USAGE] Consultada pelo verificador de densidade para validar se uma função deve ser fragmentada por complexidade baseada na contagem de instruções lógicas e fluxos de controle estrutural.
fn count_logical_statements(func: &super::super::parser::ScannedFunction<'_>) -> usize {
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

/// [WHAT] Counts the number of parameters in a function declaration string using a depth-aware scanner.
/// [WHY] To enforce architectural constraints on function coupling and nesting.
/// [WHERE] Function-level audit stage of metrics rule.
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
