use super::super::AuditViolation;
use super::super::config::AuditConfig;
use super::super::rules_logs::messages;
use crate::restricted_tokens::resolve::elite_resolve_or;

/// [WHAT] Verificador de isolamento e complexidade de macros que impede a presença de lógica opaca.
/// [WHY] Garante que as macros permaneçam simples e puramente estruturais para facilitar a auditoria industrial do geode.
/// [WHERE] Camada de análise estrutural no módulo macros.rs.
/// [USAGE] Orquestra a detecção de blocos macro e a análise de conformidade, produzindo violações quando os limites definidos em delirium.toml são excedidos.
/// [WHAT] Verificador industrial de isolamento de macros e complexidade lógica que impede a infiltração de lógica opaca e descontrolada no binário final do Aesthetic Core.
/// [WHY] Para garantir que a lógica de business permaneça auditável em funções puras, evitando que macros se tornem contêineres lógicos complexos de difícil manutenção.
/// [WHERE] Camada de auditoria estrutural core do motor de macros localizada no arquivo macros.rs para orquestração de regras de integridade.
/// [USAGE] Orquestra o ciclo de detecção e análise de blocos macro operacionais para conformidade arquitetural, garantindo que cada definição seja rasa e puramente estrutural em sua natureza.
/// [WHAT] Contexto de auditoria de macros para redução de acoplamento funcional (rule_params).
pub struct MacroAuditCtx<'a> {
    pub file_path: &'a str,
    pub source: &'a str,
    pub config: &'a AuditConfig,
    pub violations: &'a mut Vec<AuditViolation>,
}

struct MacroState {
    pub complex: bool,
    pub depth: i32,
    pub lines: usize,
    pub start_line: usize,
    pub name: String,
}

/// [WHAT] Ponto de entrada para o verificador industrial de isolamento de macros que analisa o código em busca de definições de macros para garantir que sigam os limites industriais.
/// [WHY] Para assegurar que as definições de macros operacionais não ocultem lógica perigosa ou excessivamente longa, mantendo a árvore de dependências lógica rasa e fácil de auditar por agentes de IA.
/// [WHERE] Camada de análise léxica para macros no motor estrutural em macros.rs, atuando como o gatekeeper primário para abstrações metaprogramáveis no Aesthetic Core.
/// [USAGE] Orquestra o ciclo de varredura e análise de blocos macro em busca de isolamento arquitetural, utilizando um scanner de estado para rastrear profundidade e complexidade em tempo real.
pub fn verify(ctx: &mut MacroAuditCtx<'_>) {
    let mut in_macro = false;
    let mut state = MacroState { complex: false, depth: 0, lines: 0, start_line: 0, name: String::new() };

    for (idx, line) in ctx.source.lines().enumerate() {
        let trimmed = line.trim();
        let comment_start = match trimmed.find("//") {
            Some(c_idx) => c_idx,
            None => trimmed.len(),
        };
        let code_buffer = &trimmed[..comment_start];
        
        let target = format!("{}{}", "macro", "_rules!");
        if !in_macro && code_buffer.contains(&target) {
            in_macro = true;
            state = init_macro_state(idx, trimmed);
        }

        if !in_macro { continue; }
        
        process_macro_line(ctx.config, trimmed, &mut state);

        if state.depth == 0 && trimmed.contains('}') {
            analyze_macro(ctx, &state);
            in_macro = false;
        }
    }
}

/// [WHAT] Processes an individual source line within an active macro block to evaluate the accumulation of lines and the structural depth of the definition.
/// [WHY] To keep the main verify logic shallow and maintain the cognitive load standards of the Elite governance engine by decoupling line-level scanning from orchestration.
/// [WHERE] Internal utility function for the structural macro audit engine, providing localized analysis of tokens and brace balancing within macros.rs.
/// [USAGE] Invocada iterativamente para cada linha descoberta dentro de uma definição macro operacional com o objetivo de sincronizar o estado acumulado de complexidade estrutural e contagem de massa operacional.
fn process_macro_line(config: &AuditConfig, trimmed: &str, state: &mut MacroState) {
    state.lines += 1;
    
    // Logic complexity check
    if !config.rules.macros.allow_complex_logic {
        check_complex_tokens(trimmed, &mut state.complex);
    }

    // Structural tracking
    for c in trimmed.chars() {
        if c == '{' { state.depth += 1; }
        if c == '}' && state.depth > 0 { state.depth -= 1; }
    }
}

/// [WHAT] Lexical scanner that analyzes a sequence of tokens to identify keywords that indicate complex business logic (e.g., if, match) within a macro definition.
/// [WHY] To satisfy elite architectural standards which mandate that macros must remain simple structural expansions, pushing complex logic into typed, auditable Rust functions.
/// [WHERE] Internal lexical validation phase within the macro audit engine in macros.rs, providing the decision logic for the rule_macro_isolation governance gate.
/// [USAGE] Called by process_macro_line to evaluate the complexity state of a macro in real-time, flagging any tokens that deviate from the pure structural expansion model.
fn check_complex_tokens(trimmed: &str, complex: &mut bool) {
    if contains_forbidden_logic(trimmed) {
        *complex = true;
    }
}

/// [WHAT] Alphanumeric predicate that determines if a specific line of code contains keywords or symbols (match, if, loop) that are forbidden within shallow macro definitions.
/// [WHY] To keep macros thin and maintainable by ensuring that any sophisticated logic is implemented in standard Rust functions where it can be properly typed and verified.
/// [WHERE] Predicate logic layer for complex token detection in macros.rs, acting as a low-level filter for the logical density audit rules.
/// [USAGE] Utilizada pelo scanner de tokens para identificar palavras-chave de controle de fluxo que comprometeriam a transparência arquitetural da macro caso fossem permitidas em sua definição.
fn contains_forbidden_logic(trimmed: &str) -> bool {
    let forbidden = ["if ", "match ", "while ", "for ", "loop "];
    forbidden.iter().any(|t| trimmed.contains(t))
}

/// [WHAT] Evaluates functional constraints on a completed macro definition.
/// [WHY] To ensure that architectural justifications for macro usage are documented and within limits.
/// [WHERE] Diagnostic reporting phase for macro rules in macros.rs.
/// [USAGE] Invoked when the end of a macro definition is reached to perform final validation of the accumulated logical state and line count.
fn analyze_macro(ctx: &mut MacroAuditCtx<'_>, state: &MacroState) {
    if state.lines > ctx.config.rules.macros.max_lines && ctx.config.rules.macros.max_lines > 0 {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: state.start_line,
            rule_id: crate::governance::rule_id("rule_macro_isolation", &state.name),
            message: format!("{} (Lines: {}). Max: {}. Hint: {}", 
                             messages::MACRO_LENGTH, state.lines, ctx.config.rules.macros.max_lines, ctx.config.rules.macros.tip),
            snippet: None,
            quick_fix: None,
        });
    }

    if state.complex {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: state.start_line,
            rule_id: crate::governance::rule_id("rule_macro_isolation", &state.name),
            message: format!("{}. Hint: {}", messages::MACRO_COMPLEXITY, ctx.config.rules.macros.tip),
            snippet: None,
            quick_fix: None,
        });
    }
}

/// [WHAT] Inicializa um novo MacroState extraindo o nome e as coordenadas de linha de uma declaração de macro corporativa.
/// [WHY] Mantém o loop principal de auditoria raso e cumpre os requisitos de aninhamento cognitivo industrial.
/// [WHERE] Fábrica de estado interna no módulo macros.rs.
/// [USAGE] Chamada no início da detecção de uma macro para preparar o contexto de análise de complexidade e massa, garantindo que o motor de auditoria tenha visibilidade total sobre o nome e a localização do artefato.
fn init_macro_state(idx: usize, trimmed: &str) -> MacroState {
    let name = elite_resolve_or(elite_resolve_or(trimmed.split('!')
        .nth(1), "")
        .trim()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .map(|s| s.to_string()), "unknown".to_string());

    MacroState { 
        complex: false, 
        depth: 0, 
        lines: 0,
        start_line: idx + 1,
        name 
    }
}
