use crate::config::AuditConfig;

/// [WHAT] Construtor de IDs de regra hierárquicos (dotted paths).
pub fn rule_id(base: &str, sub: &str) -> String {
    format!("{}.{}", base, sub)
}

/// [WHAT] Construtor de IDs de regra atômicos.
pub fn rule_id_atomic(base: &str) -> String {
    base.to_string()
}

/// [WHAT] Verifica se um rule_id específico casa com um padrão definido no manifesto.
/// [WHY] Suporta matching por prefixo para permitir silenciamento granular (ex: rule_poison_tokens silencia rule_poison_tokens.println).
pub fn matches(rule_id: &str, pattern: &str) -> bool {
    rule_id == pattern || rule_id.starts_with(&format!("{}.", pattern))
}

/// [WHAT] Verifica se um rule_id casa com qualquer padrão em uma lista de strings.
pub fn matches_any(rule_id: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| matches(rule_id, p))
}

/// [WHAT] Determina se uma regra específica deve ser ignorada para um dado arquivo.
/// [WHY] Centraliza a lógica de jurisdição (exclusive_to / ignore_to) com suporte a IDs de grão fino.
pub fn is_rule_ignored(path: &str, rule_id: &str, config: &AuditConfig) -> bool {
    let normalized_path = path.trim_matches(|c| c == '.' || c == '/');

    // JURISDIÇÃO 1: Isolação Atômica Sênior (exclusive_to)
    for (exclusive_path, allowed_rules) in &config.jurisdiction.exclusive_to {
        let normalized_exclusive = exclusive_path.trim_matches(|c| c == '.' || c == '/');
        if normalized_path == normalized_exclusive || normalized_path.starts_with(&(normalized_exclusive.to_string() + "/")) {
            // Se a regra NÃO casa com nenhum dos padrões permitidos, ela deve ser IGNORADA
            if !matches_any(rule_id, allowed_rules) {
                return true;
            }
        }
    }

    // JURISDIÇÃO 2: Rejeição Explícita (ignore_to)
    for (ignore_path, ignored_rules) in &config.jurisdiction.ignore_to {
        let normalized_ignore = ignore_path.trim_matches(|c| c == '.' || c == '/');
        if normalized_path == normalized_ignore || normalized_path.starts_with(&(normalized_ignore.to_string() + "/")) {
            if ignored_rules.iter().any(|r| r == "all") || matches_any(rule_id, ignored_rules) {
                return true;
            }
        }
    }

    false
}

/// [WHAT] Classifica a severidade de uma violação para ativação do Memory Decay.
pub fn is_severe(rule_id: &str) -> bool {
    let severe_bases = [
        "rule_else_free", 
        "rule_nesting", 
        "rule_poison_tokens", 
        "rule_logical_density",
    ];
    
    severe_bases.iter().any(|&base| matches(rule_id, base))
}

/// [WHAT] Mapeia um ID de regra (plano ou hierárquico) para seu nível de prioridade industrial.
pub fn map_to_level(rule_id: &str) -> &'static str {
    if matches(rule_id, "rule_poison_tokens") || 
       matches(rule_id, "rule_deps") || 
       matches(rule_id, "rule_layering") {
        "P0"
    } else if matches(rule_id, "rule_else_free") || 
              matches(rule_id, "rule_nesting") || 
              matches(rule_id, "rule_macro_isolation") || 
              matches(rule_id, "rule_logical_density") || 
              matches(rule_id, "rule_params") || 
              matches(rule_id, "rule_deps_purity") {
        "P1"
    } else if matches(rule_id, "rule_doc_tags") || 
              matches(rule_id, "rule_word_salad") {
        "P2"
    } else {
        "P3"
    }
}
