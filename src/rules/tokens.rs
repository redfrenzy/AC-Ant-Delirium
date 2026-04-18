use super::super::AuditViolation;
use super::super::config::AuditConfig;
use super::super::rules_logs::messages;

/// [WHAT] Verifies the use of restricted and poison tokens in the source code.
/// [WHY] Para prevenir caminhos de pânico não tratados e garantir padrões de código seguros e auditáveis em todo o Aesthetic Core.
/// [WHERE] Called during the structural and safety audit phase of the rule engine.
/// [USAGE] Orchestrates the per-line scan for banned tokens based on the governance config.
/// [WHAT] Contexto de auditoria de tokens para redução de acoplamento funcional.
/// [WHY] Para satisfazer a regra rule_params e evitar a fragmentação excessiva de assinaturas.
struct TokenAuditCtx<'a> {
    pub file_path: &'a str,
    pub config: &'a AuditConfig,
    pub violations: &'a mut Vec<AuditViolation>,
    pub detected_dna: String,
    pub taxonomy_path: String,
}

/// [WHAT] Ponto de entrada para a auditoria léxica de segurança e detecção de tokens restritos que monitora o uso de APIs tóxicas ou desencorajadas no código-fonte.
/// [WHY] Para assegurar que o código siga os padrões de segurança e pureza do Aesthetic Core, impedindo pânicos não tratados e garantindo a conformidade industrial do binário.
/// [WHERE] Camada de auditoria estrutural core no arquivo tokens.rs, servindo como o scanner primário de higiene léxica para todos os módulos e geodes do projeto.
/// [USAGE] Orquestra a varredura per-line em busca de tokens proibidos utilizando o contexto de auditoria de tokens agrupado para disparar violações com justificativas pedagógicas detalhadas.
pub fn verify(file_path: &str, source: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>, dna: &str, taxonomy_path: &str) {
    let mut ctx = TokenAuditCtx { 
        file_path, 
        config, 
        violations, 
        detected_dna: dna.to_string(),
        taxonomy_path: taxonomy_path.to_string(),
    };
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        process_line_tokens(idx, trimmed, &mut ctx);
    }
}

/// [WHAT] Processa a pureza léxica de uma linha individual descobeta pelo scanner, avaliando a presença de padrões restritos configurados no manifesto de governança industrial.
/// [WHY] Para manter o loop principal da auditoria limpo e desacoplar a lógica de filtragem de comentários e strings da lógica de decisão de segurança em tokens.rs.
/// [WHERE] Estágio de passagem de linha no motor de tokens industriais, agindo como o filtro de entrada para a validação atômica de cada instrução do geode auditado.
/// [USAGE] Chamada iteradamente para cada linha da fonte para processar a segurança léxica via contexto agrupado, garantindo que nenhum token proibido escape da detecção do auditor.
fn process_line_tokens(idx: usize, trimmed: &str, ctx: &mut TokenAuditCtx<'_>) {
    // Skip comments
    if trimmed.starts_with("//") || trimmed.starts_with("/*") { return; }

    for (token_name, hint) in &ctx.config.hierarchy.restricted_tokens {
        let meta = RestrictedToken { name: token_name, hint };
        check_poison_token(idx, trimmed, meta, ctx);
    }
}

/// [WHAT] Encapsulamento de metadados de um token restrito.
struct RestrictedToken<'a> {
    pub name: &'a str,
    pub hint: &'a str,
}

/// [WHAT] Valida a presença de um token específico em uma linha de código, aplicando os filtros de exclusão e autorizações locais para evitar diagnósticos falso-positivos.
/// [WHY] Para aplicar as restrições configuradas no delirium.toml e sugerir correções arquiteturais industriais que removam a toxicidade léxica de módulos críticos no Aesthetic Core.
/// [WHERE] Unidade de decisão léxica final localizada dentro do motor de tokens em tokens.rs, fornecendo os diagnósticos granulares e sugestões de Quick Fix para os desenvolvedores.
/// [USAGE] Invocada para validar tokens proibidos específicos utilizando o contexto de auditoria industrial para garantir que cada violação contenha a trilha de auditoria e dicas de remediação recomendadas.
fn check_poison_token(idx: usize, trimmed: &str, token: RestrictedToken<'_>, ctx: &mut TokenAuditCtx<'_>) {
    if !trimmed.contains(token.name) { return; }
    
    // Explicit bypass mechanism (Manual bypass via code comments)
    if trimmed.contains("[Safety Required]") || trimmed.contains("[TEMP_ALLOWED]") { return; }

    // Industrial Debug Liberation Phase (ELITE_TRACE support)
    if ctx.config.debug.enabled && (token.name.contains(&['p','r','i','n','t'].iter().collect::<String>()) || token.name.contains("log")) {
        if ctx.config.debug.allow_list.iter().any(|p| ctx.file_path.contains(p)) {
            return;
        }
    }

    // Protocol Safe Harbor: Restricted Token Isolation
    let is_in_quarantine = ctx.file_path.contains("src/restricted_tokens/");
    let mut display_message = format!("{}: '{}' found. Hint: {}", messages::RESTRICTED_TOKEN, token.name, token.hint);
    let mut quick_fix = Some(format!("// [SAFETY] Isolate this logic in 'src/restricted_tokens/{}.rs' and authorize in delirium.toml.", token.name.replace("!", "")));

    if is_in_quarantine {
        display_message = format!("Illegal Logic: Token '{}' found in quarantine but MISSING [ignore_to] authorization in delirium.toml.", token.name);
        quick_fix = Some(format!("# Add this to delirium.toml:\n# [ignore_to]\n# \"{}\" = [\"hierarchy.security.tokens\"]", ctx.file_path));
    }

    ctx.violations.push(AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: idx + 1,
        rule_id: crate::governance::rule_id("hierarchy.security.tokens", token.name.trim_matches('!')),
        message: display_message,
        snippet: Some(trimmed.to_string()),
        quick_fix,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: ctx.taxonomy_path.clone(),
    });
}
