use crate::AuditViolation;
use crate::config::AuditConfig;

/// [WHAT] Analisa a qualidade pedagógica da documentação para evitar "Word Salads" (descrições circulares ou triviais).
/// [WHY] Para garantir que a documentação agregue valor arquitetural real e não seja apenas um placeholder para passar nos testes.
/// [WHERE] Chamado durante a fase de auditoria semântica.
pub fn verify(functions: &[crate::parser::ScannedFunction<'_>], _config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    for func in functions {
        check_word_salad(func, violations);
    }
}

/// [WHAT] Verifica se os blocos [WHAT], [WHY] ou [WHERE] são cópias triviais do nome da função.
/// [WHY] Definições circulares (ex: 'fn init() -> [WHAT] Inicializa o sistema') são consideradas débito pedagógico.
/// [WHERE] Lógica interna de quality.rs.
fn check_word_salad(func: &crate::parser::ScannedFunction<'_>, violations: &mut Vec<AuditViolation>) {
    let name_lower = func.name.to_lowercase();
    let name_parts: Vec<&str> = name_lower.split('_').collect();

    for tag in &["[WHAT]", "[WHY]", "[WHERE]"] {
        if let Some(content) = extract_tag_content(&func.docstring, tag) {
            if is_circular_definition(content, &name_parts) {
                violations.push(AuditViolation {
                    line: func.start_line,
                    rule_id: "rule_word_salad",
                    message: format!(
                        "Word Salad Detectada em '{}' {}: A descrição é circular ou repete apenas o nome da função.",
                        func.name, tag
                    ),
                    snippet: Some(format!("/// {} {}", tag, content)),
                    quick_fix: None,
                });
            }
        }
    }
}

/// [WHAT] Extrai o conteúdo de uma tag específica dentro de uma docstring.
/// [WHY] Para isolar o texto justificativo para análise de densidade.
/// [WHERE] Helper interno de quality.rs.
fn extract_tag_content<'a>(docstring: &'a str, tag: &str) -> Option<&'a str> {
    let parts: Vec<&str> = docstring.split(tag).collect();
    if parts.len() < 2 { return None; }
    
    let content = parts[1].split('\n').next()?.trim();
    if content.is_empty() { return None; }
    
    Some(content)
}
/// [WHAT] Determina se uma descrição é uma definição circular baseada no nome da função.
/// [WHY] Se a descrição contém pouca informação além dos tokens já presentes no código, ela falha no critério de Elite Governance.
/// [WHERE] Algoritmo de decisão de quality.rs.
fn is_circular_definition(content: &str, name_parts: &[&str]) -> bool {
    let content_lower = content.to_lowercase();
    let words: Vec<&str> = content_lower.split_whitespace().collect();
    
    if words.len() < 5 { return true; } // Muito curto é tipicamente trivial

    // Conta quantos tokens do nome estão na descrição
    let mut matching_tokens = 0;
    for part in name_parts {
        if part.len() < 3 { continue; } // Ignora 'a', 'to', etc
        if content_lower.contains(part) {
            matching_tokens += 1;
        }
    }

    // Se mais de 70% das palavras do nome estão lá e a frase é curta, é circular
    if name_parts.is_empty() {
        return words.len() < 5;
    }

    let ratio = matching_tokens as f32 / name_parts.len() as f32;
    ratio > 0.8 && words.len() < 12
}
