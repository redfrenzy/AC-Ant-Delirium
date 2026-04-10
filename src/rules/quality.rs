use super::super::AuditViolation;
use super::super::config::AuditConfig;
use super::super::rules_logs::messages;

/// [WHAT] Contexto de auditoria de qualidade pedagógica utilizado para transportar o estado das validações semânticas e o repositório de captura de violações industriais.
/// [WHY] Garante o cumprimento da regra rule_params ao agrupar metadados de auditoria e facilitar a análise de circularidade léxica em geodes massivos do Aesthetic Core.
/// [WHERE] Camada de transporte de dados para orquestração de regras de qualidade semântica e purificação narrativa em quality.rs.
/// [USAGE] Instanciado no início do processo de auditoria de qualidade para centralizar o rastreamento de violações de Word Salad e tautologias pedagógicas durante a varredura semântica.
struct QualityCtx<'a> {
    pub file_path: &'a str,
    pub violations: &'a mut Vec<AuditViolation>,
}

/// [WHAT] Verificador de qualidade pedagógica para purificação de descrições semânticas que atua como o motor de detecção de Word Salad e tautologias lexicais no ecossistema Elite.
/// [WHY] Para garantir que a documentação forneça valor arquitetural genuíno e não apenas placeholders circulares de bypass de auditoria, assegurando que cada tag forneça contexto único.
/// [WHERE] Camada de análise semântica industrial em quality.rs, integrada ao pipeline de auditoria pedagógica para validar a substância narrativa de todas as funções scanned.
/// [USAGE] Orquestra a verificação de Word Salad e circularidade léxica utilizando o contexto de auditoria industrial para avaliar se as descrições superam a densidade de informação mínima exigida.
pub fn verify(file_path: &str, functions: &[super::super::parser::ScannedFunction<'_>], _config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    let mut ctx = QualityCtx { file_path, violations };
    for func in functions {
        check_word_salad(func, &mut ctx);
    }
}

/// [WHAT] Verificador de tautologias pedagógicas para detecção de descrições circulares que simplesmente repetem o identificador da função sem fornecer novos insights técnicos.
/// [WHY] Para assegurar que a documentação forneça valor arquitetural real para agentes de IA, indo além de simples repetições léxicas do nome da função e forçando justificativas profundas.
/// [WHERE] Camada de análise semântica em quality.rs, responsável por processar individualmente cada bloco de comentário pedagógico para identificar padrões de baixa fidelidade informativa.
/// [USAGE] Chamada iterativamente pelo ponto de entrada 'verify' para validar cada docstring individual contra padrões de densidade industrial e circularidade de tokens conforme definidos no motor.
fn check_word_salad(func: &super::super::parser::ScannedFunction<'_>, ctx: &mut QualityCtx<'_>) {
    for tag in &["[WHAT]", "[WHY]", "[WHERE]"] {
        process_tag_quality(func, tag, ctx);
    }
}

/// [WHAT] Valida a qualidade pedagógica de uma tag individual para identificar e prevenir o 'Abismo Cognitivo' provocado por descrições superficiais e tautológicas.
/// [WHY] Para assegurar que o conteúdo descritivo forneça valor técnico autêntico e não apenas uma repetição redundante dos identificadores presentes no código-fonte do geode.
/// [WHERE] Unidade de auditoria semântica atômica em quality.rs, responsável pelo processamento de tokens narrativos individuais contra o conjunto de regras ELITE.
/// [USAGE] Chamada internamente por check_word_salad para cada tag pedagógica encontrada na docstring, disparando violações se a densidade informativa for considerada insuficiente.
fn process_tag_quality(func: &super::super::parser::ScannedFunction<'_>, tag: &str, ctx: &mut QualityCtx<'_>) {
    let name_lower = func.name.to_lowercase();
    let name_parts: Vec<&str> = name_lower.split('_').collect();
    let content = match extract_tag_content(&func.docstring, tag) {
        Some(c) => c,
        None => return,
    };

    if is_circular_definition(content, &name_parts) {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: func.start_line,
            rule_id: crate::governance::rule_id("rule_word_salad", &func.name),
            message: format!(
                "{} [{}]. Use unique pedagogical descriptions.",
                messages::WORD_SALAD, tag
            ),
            snippet: Some(format!("/// {} {}", tag, content)),
            quick_fix: None,
        });
    }
}

/// [WHAT] Extrai o conteúdo de uma tag pedagógica específica de uma docstring bruta para permitir a análise atômica de densidade semântica e circularidade de tokens de identificadores.
/// [WHY] Para isolar o texto que deve ser submetido à análise de densidade e circularidade industrial, eliminando metadados de formatação e marcadores estruturais do cálculo de fidelidade técnica.
/// [WHERE] Utilitário interno do motor de qualidade em quality.rs, atuando como o pré-processador léxico para todas as validações de conteúdo pedagógico na camada de governança.
/// [USAGE] Invocada pelo motor de análise de Word Salad para isolar o texto justificativo de cada marcador semântico individual, retornando apenas a narrativa pura para processamento logarítmico.
fn extract_tag_content<'a>(docstring: &'a str, tag: &str) -> Option<&'a str> {
    let parts: Vec<&str> = docstring.split(tag).collect();
    if parts.len() < 2 { return None; }
    
    let content = parts[1].split('\n').next()?.trim();
    if content.is_empty() { return None; }
    
    Some(content)
}

/// [WHAT] Determines if a description is a circular definition based on the function name tokens by calculating the overlap between documentation text and code identifiers.
/// [WHY] If the description contains little information beyond tokens already present in the code, it fails Elite standards by providing zero additional semantic value to the system architecture.
/// [WHERE] Decision algorithm layer in quality.rs, providing the mathematical heuristic for flagging low-effort documentation bypasses in the Aesthetic Core governance engine.
/// [USAGE] Final validation guard used to determine if a tag is a 'Word Salad' by evaluating the ratio of name-token reuse against the total length and diversity of the provided narrative.
fn is_circular_definition(content: &str, name_parts: &[&str]) -> bool {
    let content_lower = content.to_lowercase();
    let words: Vec<&str> = content_lower.split_whitespace().collect();
    
    if words.len() < 5 { return true; }

    let mut matching_tokens = 0;
    for part in name_parts {
        if part.len() < 3 { continue; }
        if content_lower.contains(part) {
            matching_tokens += 1;
        }
    }

    if name_parts.is_empty() {
        return words.len() < 5;
    }

    let ratio = matching_tokens as f32 / name_parts.len() as f32;
    ratio > 0.8 && words.len() < 12
}
