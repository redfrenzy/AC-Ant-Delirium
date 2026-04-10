use crate::config::AuditConfig;

/// [WHAT] Construtor de IDs de regra hierárquicos (dotted paths) para suporte a grão fino.
/// [WHY] Permite a organização de regras em namespaces lógicos facilitando a filtragem e jurisdição granular no manifest.
/// [WHERE] Utilitário fundamental de roteamento no módulo governance.rs.
/// [USAGE] Invocado para gerar IDs como rule_poison_tokens.println de forma padronizada e segura. Esta função é a base para a criação de namespaces de regras no Geode, permitindo que o auditor identifique violações com grão fino e que o operador do projeto aplique silenciamentos ou exceções específicas através do manifesto de governança, garantindo um controle granular sobre a conformidade arquitetural.
pub fn rule_id(base: &str, sub: &str) -> String {
    format!("{}.{}", base, sub)
}

/// [WHAT] Construtor de IDs de regra atômicos para identificação única de componentes de governança.
/// [WHY] Simplifica a criação de strings de ID e garante consistência na arquitetura de namespaces do motor.
/// [WHERE] Utilitário fundamental de roteamento no módulo governance.rs.
/// [USAGE] Chamado internamente por diversas regras para normalizar a representação de violações em relatórios industriais. Esta normalização é vital para garantir que os IDs de regras atômicos sejam consistentes e fáceis de filtrar em dashboards externos, permitindo uma triagem eficiente de falhas de conformidade que não requerem sub-especificações de grão fino para serem compreendidas e remediadas.
pub fn rule_id_atomic(base: &str) -> String {
    base.to_string()
}

/// [WHAT] Verifica se um rule_id específico casa com um padrão definido no manifesto de governança.
/// [WHY] Suporta matching por prefixo para permitir silenciamento granular (ex: rule_poison_tokens silencia rule_poison_tokens.println), garantindo flexibilidade na jurisdição.
/// [WHERE] Lógica de matching centralizada no módulo governance.rs.
/// [USAGE] Chamada por is_rule_ignored e outros filtros globais para validar a aplicabilidade de uma regra contra o conjunto de regras permitidas ou ignoradas no arquivo de configuração. Este motor de correspondência permite uma flexibilidade total nas listas de jurisdição, suportando tanto seleções de regras individuais quanto namespaces inteiros, o que é essencial para gerenciar a governança em geodes complexos com múltiplas zonas de autoridade.
pub fn matches(rule_id: &str, pattern: &str) -> bool {
    rule_id == pattern || rule_id.starts_with(&format!("{}.", pattern))
}

/// [WHAT] Verifica se um rule_id casa com qualquer padrão em uma lista de strings.
/// [WHY] Permite o uso de listas de silenciamento ou permissões complexas, simplificando a lógica de jurisdição.
/// [WHERE] Utilitário central de matching no módulo governance.rs.
/// [USAGE] Chamado por is_rule_ignored para validar a conformidade de uma regra contra múltiplas exceções, garantindo que o motor de auditoria possa processar listas densas de silenciamento ou permissões granulares definidas no manifesto de governança sem comprometer a integridade do gatekeeper industrial.
pub fn matches_any(rule_id: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| matches(rule_id, p))
}

/// [WHAT] Determina se uma regra específica deve ser ignorada para um dado arquivo ou contexto com base nas listas de jurisdição.
/// [WHY] Centraliza a lógica de jurisdição (exclusive_to / ignore_to) com suporte a IDs de grão fino, garantindo que exceções sejam aplicadas de forma determinística e auditável.
/// [WHERE] Lógica de filtragem primária no módulo governance.rs do motor de auditoria.
/// [USAGE] Chamado por audit_code_to_result para filtrar violações antes de disparar o Memory Decay ou gerar o artefato Shadow Context final para exportação industrial. Esta função é o portão final que decide se uma descoberta técnica deve ser reportada ao desenvolvedor ou se ela está protegida por uma exceção de jurisdição (Safe Harbor), garantindo que o Geode mantenha o foco em violações reais e autorizadas conforme definido no delirium.toml.
pub fn is_rule_ignored(path: &str, rule_id: &str, config: &AuditConfig) -> bool {
    let normalized_path = path.trim_matches(|c| c == '.' || c == '/');

    if check_exclusive(normalized_path, rule_id, config) {
        return true;
    }

    if check_ignore(normalized_path, rule_id, config) {
        return true;
    }

    false
}

/// [WHAT] Verifica a jurisdição de exclusividade para um caminho e regra específicos.
/// [WHY] Auxilia na redução do Abismo Cognitivo (nesting) ao isolar a lógica de iteração e matching de exclusividade.
/// [WHERE] Função privada de auxílio no módulo governance.rs.
/// [USAGE] Invocada internamente por is_rule_ignored para validar se uma regra está contida na lista de permissões exclusivas do arquivo. Através desta verificação, o sistema garante que certas regras críticas sejam aplicadas apenas onde explicitamente permitido, reforçando a integridade arquitetural do Geode ao impedir que exceções acidentais vazem para zonas de código onde o rigor máximo é mandatório.
fn check_exclusive(normalized_path: &str, rule_id: &str, config: &AuditConfig) -> bool {
    for (exclusive_path, allowed_rules) in &config.jurisdiction.exclusive_to {
        let normalized_exclusive = exclusive_path.trim_matches(|c| c == '.' || c == '/');
        let as_part = format!("/{}", normalized_exclusive);
        let is_match = normalized_path.ends_with(&as_part)
            || normalized_path.contains(&format!("{}/", as_part))
            || normalized_path == normalized_exclusive
            || normalized_path.starts_with(&(normalized_exclusive.to_string() + "/"));
        
        if is_match && !matches_any(rule_id, allowed_rules) {
            return true;
        }
    }
    false
}

/// [WHAT] Verifica se uma regra foi explicitamente ignorada para um dado caminho no manifesto.
/// [WHY] Isola a lógica de iteração de ignore_to para cumprir os limites de aninhamento de governança.
/// [WHERE] Função privada de auxílio no módulo governance.rs.
/// [USAGE] Invocada internamente por is_rule_ignored para processar a lista de rejeição explícita definida pelo usuário no delirium.toml. Este mecanismo permite que o operador do projeto silencie avisos de governança em arquivos específicos de forma justificada, garantindo que o reporte final de conformidade seja livre de falsos positivos ou ruídos em casos onde a violação é intencional e devidamente documentada no manifesto.
fn check_ignore(normalized_path: &str, rule_id: &str, config: &AuditConfig) -> bool {
    for (ignore_path, ignored_rules) in &config.jurisdiction.ignore_to {
        let normalized_ignore = ignore_path.trim_matches(|c| c == '.' || c == '/');
        let as_part = format!("/{}", normalized_ignore);
        let is_match = normalized_path.ends_with(&as_part)
            || normalized_path.contains(&format!("{}/", as_part))
            || normalized_path == normalized_ignore
            || normalized_path.starts_with(&(normalized_ignore.to_string() + "/"));
        
        if is_match && (ignored_rules.iter().any(|r| r == "all") || matches_any(rule_id, ignored_rules)) {
            return true;
        }
    }
    false
}

/// [WHAT] Classifica a severidade de uma violação para ativação do mecanismo corretivo de Memory Decay.
/// [WHY] Distingue entre falhas cosméticas e ameaças estruturais à integridade do sistema, permitindo reações automáticas graduais.
/// [WHERE] Critério de decisão de severidade no módulo governance.rs.
/// [USAGE] Chamado por trigger_memory_decay para decidir se a documentação de uma função deve ser purgada após falhas graves de conformidade. Ao identificar violações "severas", o geode aciona seu mecanismo de punição arquitetural, forçando o desenvolvedor a corrigir a estrutura fundamental do código antes de permitir que ele recupere seus preâmbulos pedagógicos, garantindo que a qualidade técnica sempre preceda a documentação no Aesthetic Core.
pub fn is_severe(rule_id: &str) -> bool {
    let severe_bases = [
        "rule_else_free", 
        "rule_nesting", 
        "rule_poison_tokens", 
        "rule_logical_density",
    ];
    
    severe_bases.iter().any(|&base| matches(rule_id, base))
}

/// [WHAT] Mapeia um ID de regra para seu nível de prioridade industrial Aesthetic Core (P0-P3).
/// [WHY] Traduz regras internas em categorias de risco acionáveis por IAs e dashboards executivos de governança.
/// [WHERE] Tabela de mapeamento de prioridade no módulo governance.rs.
/// [USAGE] Vital para a geração do artefato Shadow Context, permitindo que ferramentas externas priorizem a correção de violações baseadas em sua gravidade técnica. Este mapeamento traduz a complexidade interna do motor de auditoria em níveis de prioridade P0-P3 de fácil compreensão para humanos e agentes de IA, facilitando a triagem industrial e a automação de reparos conforme a criticidade de cada achado de governança no projeto.
pub fn map_to_level(rule_id: &str) -> &'static str {
    if matches(rule_id, "rule_poison_tokens") || 
       matches(rule_id, "rule_deps") || 
       matches(rule_id, "rule_layering") {
        "P0"
    } else if matches(rule_id, "rule_else_free") || // [REASON] Priorities P1 satisfy structural and architectural constraints.
              matches(rule_id, "rule_nesting") || 
              matches(rule_id, "rule_macro_isolation") || 
              matches(rule_id, "rule_logical_density") || 
              matches(rule_id, "rule_params") || 
              matches(rule_id, "rule_deps_purity") {
        "P1"
    } else if matches(rule_id, "rule_doc_tags") || // [REASON] P2 focuses on documentation and pedagogical quality metadata.
              matches(rule_id, "rule_word_salad") {
        "P2"
    } else { // [REASON] Base default to P3 for rules not explicitly categorized in higher priority tiers.
        "P3"
    }
}
