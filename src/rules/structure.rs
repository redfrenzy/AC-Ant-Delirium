use super::super::AuditViolation;
use super::super::config::AuditConfig;
use super::super::rules_logs::messages;
use super::super::parser::ScannedFunction;

/// [WHAT] Contexto de auditoria estrutural para redução de acoplamento funcional (rule_params).
/// [WHY] Para agrupar metadados de auditoria e garantir que as funções de verificação permaneçam abaixo do limite de 4 parâmetros.
/// [WHERE] Camada de dados para orquestração de regras estruturais em structure.rs.
pub struct StructureCtx<'a> {
    pub file_path: &'a str,
    pub source: &'a str,
    pub functions: &'a [ScannedFunction<'a>],
    pub config: &'a AuditConfig,
    pub violations: &'a mut Vec<AuditViolation>,
    pub current_nesting: usize,
}

/// [WHAT] Ponto de entrada para a auditoria de integridade estrutural e limites de nidificação que analisa o código fonte em busca de padrões de fluxo não-lineares indesejados.
/// [WHY] Para garantir a clareza cognitiva e o fluxo linear de execução, eliminando o Abismo Cognitivo em geodes industriais e assegurando que o código seja legível por humanos e auditável por IA.
/// [WHERE] Camada de auditoria estrutural core em structure.rs, servindo como o scanner primário de higiene lógica para todos os arquivos .rs do projeto.
/// [USAGE] Analisa o código fonte em busca de nidificações excessivas e blocos else proibidos conforme a política Elite, disparando violações granulares com sugestões de refatoração para funções atômicas.
pub fn verify(ctx: &mut StructureCtx<'_>) {
    ctx.current_nesting = 0;
    for (idx, line) in ctx.source.lines().enumerate() {
        process_structure(ctx, idx, line);
    }
    check_params_on_sigs(ctx);
}

/// [WHAT] Processes structural rules for a single line of code discovered by the primary scanner, evaluating keywords and indentation levels for governance compliance.
/// [WHY] To keep the main verify loop shallow and maintain Elite cognitive load standards by decoupling line-level token analysis from the orchestration of the file scan.
/// [WHERE] Line-level processing utility for the structural audit engine in structure.rs, providing localized validation for each functional statement in the source.
/// [USAGE] Called by verify() during the line-by-line scanning of source files to identify and validate structural markers such as braces and restricted control flow keywords.
fn process_structure(ctx: &mut StructureCtx<'_>, idx: usize, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() { return; }

    let clean_line = strip_strings(trimmed);
    
    if !super::super::is_rule_ignored(ctx.file_path, "rule_else_free", ctx.config) {
        if contains_word(&clean_line, "else") {
            check_else_bypass(ctx, idx, trimmed);
        }
    }

    if !super::super::is_rule_ignored(ctx.file_path, "rule_nesting", ctx.config) {
        check_nesting_depth(ctx, idx, line);
    }
}

/// [WHAT] Verifies the presence of a legitimate bypass tag for an else block to ensure that any deviation from the else-free policy is architecturally justified and documented.
/// [WHY] Para permitir exceções justificadas ao rigor do 'else-free', garantindo que cada desvio seja documentado e auditável por agentes de governança para evitar o acúmulo de débito técnico.
/// [WHERE] Internal guard for the else-free auditing rule in structure.rs, acting as the decision point for flagging prohibited control flow branching without proper authorization.
/// [USAGE] Executed when the 'else' keyword is detected on a functional line of code, checking the trailing comment for the mandatory [REASON] or [ tag to authorize the block.
fn check_else_bypass(ctx: &mut StructureCtx<'_>, idx: usize, trimmed: &str) {
    if trimmed.contains("// [REASON]") || trimmed.contains("// [") {
        return;
    }

    ctx.violations.push(AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: idx + 1,
        rule_id: "rule_else_free",
        message: messages::ELSE_BLOCK.to_string(),
        snippet: Some(trimmed.to_string()),
        quick_fix: Some("// [REASON] Justify this else block here...".to_string()),
    });
}

/// [WHAT] Implements the 'Abismo Cognitivo' check based on physical indentation level and brace depth to prevent the formation of deeply nested logic blocks.
/// [WHY] Para prevenir o encadeamento excessivo de blocos lógicos que degrada a capacidade de análise semântica da IA e torna a lógica do programa opaca para revisores humanos.
/// [WHERE] Internal utility for structural code auditing in structure.rs, providing real-time feedback on logical nesting violations during the primary scan phase.
/// [USAGE] Called by process_structure to evaluate the cognitive load of a specific line by counting the cumulative brace depth and comparing it against the industrial limit defined in config.
fn check_nesting_depth(ctx: &mut StructureCtx<'_>, idx: usize, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") { return; }

    for c in line.chars() {
        if c == '{' { ctx.current_nesting += 1; }
        if c == '}' && ctx.current_nesting > 0 { ctx.current_nesting -= 1; }
    }

    let limit = ctx.config.rules.limits.max_nesting;

    if ctx.current_nesting > limit && limit > 0 {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: idx + 1,
            rule_id: "rule_nesting",
            message: messages::NESTING_TOO_DEEP.to_string(),
            snippet: Some(line.to_string()),
            quick_fix: Some("// [REFAC] Extract this nested block into a private helper function.".to_string()),
        });
    }
}

/// [WHAT] Performs a global scan of the source code to identify and validate function signatures against parameter count limits for industrial coupling control.
/// [WHY] Para desacoplar a validação de acoplamento de funções das varreduras estruturais de linha, permitindo uma análise robusta e isolada das assinaturas de interface do geode.
/// [WHERE] Executed as a secondary pass within the `verify` method of `structure.rs`, focusing exclusively on the relational complexity of function declarations.
/// [USAGE] Called after the primary structural loop to evaluate function-level coupling, ensuring that no interface exceeds the allowed parameter count defined in the governance manifest.
fn check_params_on_sigs(ctx: &mut StructureCtx<'_>) {
    if super::super::is_rule_ignored(ctx.file_path, "rule_params", ctx.config) {
        return;
    }
    let limit = ctx.config.rules.limits.max_params;
    if limit == 0 { return; }

    for func in ctx.functions {
        let param_count = count_params_brute(func.declaration);
        if param_count > limit {
            ctx.violations.push(AuditViolation {
                file_path: ctx.file_path.to_string(),
                line: func.start_line,
                rule_id: "rule_params",
                message: messages::PARAMS_TOO_MANY.to_string(),
                snippet: Some(func.declaration.to_string()),
                quick_fix: Some("// [REFAC] Group these parameters into a dedicated Config/State Struct.".to_string()),
            });
        }
    }
}

/// [WHAT] Manually calculates the number of parameters in a raw function signature string by parsing tokens within the signature parentheses while respecting nesting.
/// [WHY] Para fornecer um mecanismo robusto e independente de dependências para medir a complexidade de interfaces industriais, evitando falsos positivos com tipos genéricos ou aninhados.
/// [WHERE] Internal helper utility used by the parameter validation pass in structure.rs to extract and count arguments from raw source code strings accurately.
/// [USAGE] Called by check_params_on_sigs to process individual function signature lines, returning an integer count of distinct parameters found within the primary argument list.
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

/// [WHAT] Parses away quotation marks dynamically without using prohibited 'else' blocks to ensure that string literals do not interfere with structural analysis.
/// [WHY] Para sanitizar as linhas de código, eliminando ruídos de strings literais que podem conter chaves ou palavras-chave que confundiriam o motor de auditoria estrutural simplificado.
/// [WHERE] Pre-processor utility for structural rule evaluation in structure.rs, acting as a sanitization layer for raw source lines before they are submitted to token scanners.
/// [USAGE] Called by process_structure to sanitize code lines before keyword matching, ensuring that only operant code tokens are analyzed for nesting and control flow compliance.
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

/// [WHAT] Checks strict word constraints without using heavy regex engines to identify the presence of specific keywords within a sanitized code segment.
/// [WHY] Para assegurar a precisão léxica na detecção de tokens proibidos sem introduzir o custo de motores de regex externos e mantendo a arquitetura zero-dependency do projeto.
/// [WHERE] Alphanumeric filtering utility for keywords in structure.rs, providing a fast and reliable way to detect control flow markers in the source code.
/// [USAGE] Utilizada pelo motor de regras estruturais para identificar palavras reservadas como 'else' em linhas de código sanitizadas para garantir a conformidade com a política Elite.
fn contains_word(s: &str, word: &str) -> bool {
    s.split(|c: char| !c.is_alphanumeric() && c != '_')
     .any(|p| p == word)
}
