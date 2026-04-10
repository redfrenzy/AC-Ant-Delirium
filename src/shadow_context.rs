use super::{AuditResult, AuditViolation, json_escape, governance};
use super::config::AuditConfig;
use std::fmt::Write;

/// [WHAT] Orquestra a transformação de achados de auditoria em um relatório Shadow Context (SARIF 2.1.0) enriquecido com propriedades de contexto para agentes de IA.
/// [WHY] Para fornecer um formato de contexto operacional único e de alta densidade semântica para agentes de IA, mantendo a compatibilidade com padrões industriais de leitura de máquinas.
/// [WHERE] Ponto de entrada primário de serialização localizado no módulo dedicado shadow_context.rs, atuando como o exportador final do motor de governança.
/// [USAGE] Invocada pelo componente AuditResult como a ponte final de exportação para gerar um artefato Shadow Context (JSON) contendo metadados de governança e sugestões de Quick Fix. Este processo consolida todos os achados da auditoria em um único buffer de memória de forma eficiente, aplicando serialização manual para evitar o custo de crates de reflexão pesados, mantendo a integridade industrial e a agilidade do pipeline de CI/CD da geode.
pub fn to_shadow_context(result: &AuditResult, _file_path: &str, config: &AuditConfig) -> String {
    let mut out = String::with_capacity(result.violations.len() * 512 + 2048);

    push_shadow_header(&mut out);
    push_shadow_properties(&mut out, result, config);
    push_json_results_array(result, &mut out);
    push_shadow_footer(&mut out);
    
    out
}

/// [WHAT] Injeta o nível de severidade correspondente à prioridade industrial Aesthetic Core no formato Shadow Context de forma determinística.
/// [WHY] Para traduzir o sistema de prioridades P0-P3 para os níveis padrão consumíveis por IAs e dashboards de conformidade (error, warning, note), garantindo triagem eficiente.
/// [WHERE] Utilitário de mapeamento de severidade industrial localizado em shadow_context.rs, servindo como o tradutor de níveis de risco para o ecossistema SARIF.
/// [USAGE] Chamado internamente por push_shadow_results para classificar cada violação no nível de reporte correto baseado em seu Rule ID e criticidade arquitetural.
fn map_p_level_to_shadow(rule_id: &str) -> &'static str {
    let p_level = governance::map_to_level(rule_id);
    match p_level {
        "P0" => "error",
        "P1" => "warning",
        "P2" => "note",
        _ => "note",
    }
}

/// [WHAT] Realiza a contagem consolidada de violações agrupadas por nível de prioridade industrial (P0-P3) para sumerização no artefato de metadados.
/// [WHY] Para fornecer um resumo quantitativo de alta visibilidade que a IA e os gestores podem usar para priorizar correções críticas e avaliar a saúde estrutural do geode.
/// [WHERE] Utilitário de agregação de métricas de conformidade localizado em shadow_context.rs, integrado ao pipeline de exportação de metadados industriais.
/// [USAGE] Chamado por push_shadow_properties para popular o campo 'summary' no properties bag do Shadow Context, auxiliando na visualização rápida do estado de conformidade. Esta função percorre a lista de violações industriais e aplica uma redução determinística para obter os totais de cada nível de prioridade (P0 a P3), fornecendo um snapshot quantitativo que permite aos agentes de IA e aos desenvolvedores avaliar instantaneamente a saúde arquitetural do projeto.
fn count_by_level(violations: &[AuditViolation]) -> (usize, usize, usize, usize) {
    let mut p0 = 0;
    let mut p1 = 0;
    let mut p2 = 0;
    let mut p3 = 0;
    for v in violations {
        match governance::map_to_level(&v.rule_id) {
            "P0" => p0 += 1,
            "P1" => p1 += 1,
            "P2" => p2 += 1,
            _ => p3 += 1,
        }
    }
    (p0, p1, p2, p3)
}

/// [WHAT] Injects the Shadow Context (SARIF 2.1.0) header and tool identity metadata into the provided output buffer.
/// [WHY] To establish the required schema reference and identify the auditor as ac_ant_delirium.
/// [WHERE] Initialization sub-pass of the Shadow Context serialization engine in shadow_context.rs.
/// [USAGE] Invocado como a etapa inicial obrigatória na sequência de serialização para garantir que o cabeçalho esteja presente no início do buffer de saída JSON.
fn push_shadow_header(out: &mut String) {
    push_lc(out);
    out.push('\n');
    out.push_str("  \"version\": \"2.1.0\",\n");
    out.push_str("  \"$schema\": \"https://schemastore.azurewebsites.net/schemas/json/sarif-2.1.0-rtm.4.json\",\n");
    out.push_str("  \"runs\": [");
    push_lc(out);
    out.push('\n');
    out.push_str("    \"tool\": ");
    push_lc(out);
    out.push('\n');
    out.push_str("      \"driver\": ");
    push_lc(out);
    out.push('\n');
    out.push_str("        \"name\": \"ac_ant_delirium\",\n");
    out.push_str("        \"version\": \"0.1.0\"\n");
    out.push_str("      ");
    push_rc(out);
    out.push('\n');
    out.push_str("    ");
    push_rc(out);
    out.push_str(",\n");
}

/// [WHAT] Injects the governance properties bag into the Shadow Context run with AI context, priority summary, and applied configuration snapshot.
/// [WHY] To provide AI agents with a single source of truth containing operational context, tool references, and authorized commands directly within the Shadow Context artifact.
/// [WHERE] Properties serialization sub-pass in shadow_context.rs, placed between the tool header and results array.
/// [USAGE] Chamado por to_shadow_context() para injetar o bloco properties.governance com todas as informações da seção [info] do delirium.toml e um snapshot das configurações ativas. Este preâmbulo de metadados é essencial para garantir que as IAs que consomem o Shadow Context tenham visibilidade total sobre as regras de governança vigentes, recursos autorizados e comandos de terminal recomendados, criando um ambiente de auto-documentação para o geode em conformidade com os padrões Elite.
fn push_shadow_properties(out: &mut String, result: &AuditResult, config: &AuditConfig) {
    push_governance_summary(out, result, config);
    push_applied_config(out, config);
    
    out.push_str("      }\n");
    out.push_str("    },\n");
    out.push_str("    \"results\": [");
}

/// [WHAT] Serializes the governance summary block including priority order, total violations, and summary counts.
/// [WHY] To satisfy Elite logical density limits by extracting the summary generation sub-logic.
/// [WHERE] Sub-component of the property serialization layer in shadow_context.rs.
/// [USAGE] Chamado pela função push_shadow_properties para injetar o sumário executivo da auditoria no Shadow Context, fornecendo uma visão consolidada de prioridades e contagens de violação para consumo imediato por dashboards industriais.
fn push_governance_summary(out: &mut String, result: &AuditResult, config: &AuditConfig) {
    let (p0, p1, p2, p3) = count_by_level(&result.violations);
    let total = result.violations.len();

    out.push_str("    \"properties\": ");
    push_lc(out);
    out.push('\n');
    out.push_str("      \"governance\": ");
    push_lc(out);
    out.push('\n');

    let priority = if config.info.priority_order.is_empty() {
        "P0 → P1 → P2 → P3"
    } else { // [REASON] Config may override default priority sequence.
        &config.info.priority_order
    };
    let _ = write!(out, "        \"priority_order\": \"{}\",\n", json_escape(priority));
    let _ = write!(out, "        \"total_violations\": {},\n", total);
    let _ = write!(out, "        \"summary\": ");
    push_lc(out);
    let _ = write!(out, " \"P0\": {}, \"P1\": {}, \"P2\": {}, \"P3\": {} ", p0, p1, p2, p3);
    push_rc(out);
    out.push_str(",\n");

    push_string_array(out, "context", &config.info.context, true);
    push_string_array(out, "tools", &config.info.tools, true);
    push_string_array(out, "terminal_commands", &config.info.terminal_commands, true);
}

/// [WHAT] Serializes the active configuration snapshot, including structural limits and restricted token policies.
/// [WHY] To achieve archival compliance and density limits by isolating the configuration export logic.
/// [WHERE] Sub-component of the property serialization layer in shadow_context.rs.
/// [USAGE] Invocado para documentar o estado exato do motor de auditoria no momento da geração do reporte, permitindo que agentes de IA validem as premissas de conformidade e as restrições de token aplicadas durante o processo de auditoria.
fn push_applied_config(out: &mut String, config: &AuditConfig) {
    out.push_str("        \"applied_config\": ");
    push_lc(out);
    out.push('\n');
    let _ = write!(out, "          \"max_lines\": {},\n", config.rules.limits.max_lines);
    let _ = write!(out, "          \"max_nesting\": {},\n", config.rules.limits.max_nesting);
    let _ = write!(out, "          \"max_params\": {},\n", config.rules.limits.max_params);
    let _ = write!(out, "          \"logical_density\": {},\n", config.rules.limits.logical_density);

    let tokens: Vec<&String> = config.rules.restricted_tokens.keys().collect();
    let tokens_str: Vec<String> = tokens.iter().map(|t| format!("\"{}\"", json_escape(t))).collect();
    let _ = write!(out, "          \"restricted_tokens\": [{}],\n", tokens_str.join(", "));

    let crates_str: Vec<String> = config.rules.allowed_crates.iter().map(|c| format!("\"{}\"", json_escape(c))).collect();
    let _ = write!(out, "          \"allowed_crates\": [{}]\n", crates_str.join(", "));
    out.push_str("        ");
    push_rc(out);
    out.push('\n');
}

/// [WHAT] Serializa um array de strings JSON com indentação consistente no formato Shadow Context.
/// [WHY] Para evitar repetição de código na serialização de context, tools e terminal_commands, mantendo a pureza estrutural do Geode.
/// [WHERE] Utilitário interno do shadow_context.rs para formatação de coleções de metadados industriais.
/// [USAGE] Chamado pela função push_shadow_properties para iterar sobre as coleções de ferramentas e comandos autorizados na seção [info] do manifesto, garantindo que o reporte SARIF gerado contenha todos os metadados de contexto necessários para a operação subsequente de agentes de IA na geode.
fn push_string_array(out: &mut String, key: &str, items: &[String], trailing_comma: bool) {
    let _ = write!(out, "        \"{}\": [", key);
    if items.is_empty() {
        out.push(']');
    } else { // [REASON] Array contains items, proceed to vertical serialization.
        push_string_array_items(out, items);
    }
    if trailing_comma {
        out.push_str(",\n");
    } else { // [REASON] Terminal element reached, close array without comma.
        out.push('\n');
    }
}

/// [WHAT] Iterates over string items to append them to the active JSON array buffer with consistent indentation.
/// [WHY] To satisfy Elite nesting limits by isolating the loop body from the main conditional branches.
/// [WHERE] Sub-componente de serialização JSON em shadow_context.rs, especializado na formatação de itens de lista.
/// [USAGE] Invocado internamente por push_string_array para preencher listas de metadados industriais sem exceder a profundidade cognitiva permitida pela governança.
fn push_string_array_items(out: &mut String, items: &[String]) {
    out.push('\n');
    for (i, item) in items.iter().enumerate() {
        let comma = get_item_separator(i, items.len());
        let _ = write!(out, "          \"{}\"{}\n", json_escape(item), comma);
    }
    out.push_str("        ]");
}

/// [WHAT] Determina o separador de itens (vírgula ou vazio) baseado na posição do índice atual em relação ao tamanho total da coleção.
/// [WHY] Para achatar a lógica de serialização JSON e remover ramificações condicionais aninhadas dentro dos loops de geração de artefatos.
/// [WHERE] Utilitário léxico interno do módulo shadow_context.rs, responsável pela integridade sintática das listas geradas.
/// [USAGE] Utilizado por diversos sub-roteadores de serialização, como push_string_array e push_json_results_array, para garantir que o artefato final Shadow Context não contenha vírgulas pendentes que invalidariam o parsing do JSON industrial.
fn get_item_separator(idx: usize, len: usize) -> &'static str {
    if idx < len - 1 { "," } else { "" } // [REASON] Simple ternary logic for JSON delimiters.
}

/// [WHAT] Orchestrates the serialization of the entire violation set into the results array of the Shadow Context.
/// [WHY] To satisfy Elite nesting limits by isolating the iteration logic from the complex JSON object construction.
/// [WHERE] Result array dispatcher in shadow_context.rs.
/// [USAGE] Invocada pela função principal de exportação para preencher a lista de violações industriais detectadas. Esta função gerencia o loop de alto nível sobre os achados da auditoria, delegando a construção detalhada de cada objeto JSON para workers especializados, o que garante conformidade com os limites de nidificação e facilita a manutenção do motor de reporte.
fn push_json_results_array(result: &AuditResult, out: &mut String) {
    for (i, v) in result.violations.iter().enumerate() {
        push_json_violation(out, v);
        let terminator = get_item_separator(i, result.violations.len());
        out.push(' ');
        push_rc(out);
        out.push_str(terminator);
    }
}

/// [WHAT] Encodes a single governance violation as a machine-readable JSON object in SARIF 2.1.0 format.
/// [WHY] To keep the implementation flat and satisfy industrial complexity requirements for geode actors.
/// [WHERE] Violation serializer worker in shadow_context.rs.
/// [USAGE] Chamada iterativamente para traduzir cada violação técnica em uma entrada SARIF válida. Esta função encapsula a lógica de mapeamento de severidade e a composição básica do objeto de resultado, garantindo que o reporte seja denso em informações mas estruturalmente simples para processamento por agentes de IA.
fn push_json_violation(out: &mut String, v: &AuditViolation) {
    let level = map_p_level_to_shadow(&v.rule_id);
    let msg = json_escape(&v.message);
    
    out.push('\n');
    out.push_str("      ");
    push_lc(out);
    out.push_str("\n        \"ruleId\": \"");
    out.push_str(&v.rule_id);
    out.push_str("\",\n        \"level\": \"");
    out.push_str(level);
    out.push_str("\",\n        \"message\": ");
    push_lc(out);
    out.push_str(" \"text\": \"");
    out.push_str(&msg);
    out.push_str("\" ");
    push_rc(out);
    out.push_str(",\n");
    
    push_json_locations(out, v);
    push_shadow_snippet(out, v);
    push_shadow_fixes(out, v, &v.file_path);
}

/// [WHAT] Serializes the location information for an audit violation, including the file path and starting line.
/// [WHY] To separate physical location encoding from the main result block, reducing cognitive nesting.
/// [WHERE] Location serializer helper in shadow_context.rs.
/// [USAGE] Invocada durante a construção do objeto de violação para injetar as coordenadas geográficas (arquivo e linha) onde a inconformidade foi detectada, permitindo que ferramentas de IDE naveguem até o local exato do erro tecnológico.
fn push_json_locations(out: &mut String, v: &AuditViolation) {
    out.push_str("        \"locations\": [ ");
    push_lc(out);
    out.push('\n');
    out.push_str("          \"physicalLocation\": ");
    push_lc(out);
    out.push('\n');
    out.push_str("            \"artifactLocation\": ");
    push_lc(out);
    let _ = write!(out, " \"uri\": \"{}\" ", v.file_path);
    push_rc(out);
    out.push_str(",\n");
    out.push_str("            \"region\": ");
    push_lc(out);
    let _ = write!(out, " \"startLine\": {}", v.line);
}

/// [WHAT] Anexa um trecho opcional de código (snippet) ao registro de localização de uma violação no artefato Shadow Context.
/// [WHY] Para fornecer visibilidade imediata do código-fonte afetado no dashboard de reporte, permitindo triagem rápida sem necessidade de abrir o arquivo original.
/// [WHERE] Estágio secundário de codificação de localização em shadow_context.rs, focado em metadados de contexto visual.
/// [USAGE] Invocado durante o processamento de resultados de auditoria para enriquecer o artefato SARIF com amostras de código real, facilitando a inspeção humana e a análise heurística por agentes de IA que consomem o Shadow Context da geode.
fn push_shadow_snippet(out: &mut String, v: &AuditViolation) {
    if let Some(ref snip) = v.snippet {
        let _ = write!(out, r#", "snippet": "#);
        push_lc(out);
        let _ = write!(out, r#" "text": "{}" "#, json_escape(snip));
        push_rc(out);
    }
    out.push_str(" ");
    push_rc(out);
    out.push_str(" ");
    push_rc(out);
    out.push_str(" ]");
}

/// [WHAT] Anexa sugestões de reparo automático (Quick-Fixes) a um registro de violação de governança no formato Shadow Context (SARIF).
/// [WHY] Para automatizar o ciclo de correção de conformidade, permitindo que editores de texto e agentes de IA apliquem alterações sugeridas de forma imediata e segura.
/// [WHERE] Estágio de remediação do motor de serialização em shadow_context.rs, atuando como o exportador de planos de ação técnicos.
/// [USAGE] Chamada para adicionar operações de substituição de texto ao reporte de violação, fornecendo as coordenadas exatas e o conteúdo corretivo necessário (stubs de governança) para restaurar a conformidade do Geode em tempo recorde.
fn push_shadow_fixes(out: &mut String, v: &AuditViolation, relative_path: &str) {
    if let Some(ref fix) = v.quick_fix {
        out.push_str(",\n        \"fixes\": [ ");
        push_lc(out);
        out.push('\n');
        push_fix_description(out);
        out.push_str(",\n");
        push_fix_changes(out, v, relative_path, fix);
        out.push_str("\n        ] ");
        push_rc(out);
    }
}

/// [WHAT] Serializa a descrição pedagógica de uma correção automática no reporte SARIF.
/// [WHY] Isolar a descrição reduz a densidade lógica da função principal de correções e atende aos limites de massa Elite.
/// [WHERE] Helper de serialização de correções no módulo shadow_context.rs.
/// [USAGE] Chamada por push_shadow_fixes para injetar o texto descritivo da ação corretiva recomendada pelo motor de governança, fornecendo contexto vital para a remediação automática de violações técnicas.
fn push_fix_description(out: &mut String) {
    out.push_str("          \"description\": ");
    push_lc(out);
    out.push_str(" \"text\": \"Apply exact governance stubs\" ");
    push_rc(out);
}

/// [WHAT] Serializa as mudanças de artefatos para um Quick-Fix industrial.
/// [WHY] Mantém a densidade lógica sob controle Elite ao delegar a construção da árvore de mudanças e localizações.
/// [WHERE] Helper de serialização de mudanças no módulo shadow_context.rs.
/// [USAGE] Invocada para encapsular o caminho do arquivo e a coleção de substituições de texto que compõem uma correção automática SARIF.
fn push_fix_changes(out: &mut String, v: &AuditViolation, path: &str, fix: &str) {
    out.push_str("          \"artifactChanges\": [ ");
    push_lc(out);
    out.push('\n');
    out.push_str("            \"artifactLocation\": ");
    push_lc(out);
    let _ = write!(out, " \"uri\": \"{}\" ", path);
    push_rc(out);
    out.push_str(",\n");
    out.push_str("            \"replacements\": [ ");
    push_lc(out);
    out.push('\n');
    push_replacement_details(out, v.line, fix);
    out.push_str("            ] ");
    push_rc(out);
    out.push_str("\n          ] ");
    push_rc(out);
}

/// [WHAT] Serializa os detalhes de substituição de texto (região e conteúdo) para uma violação.
/// [WHY] Fragmentação final para atingir conformidade de densidade lógica (P1) e facilitar o parsing por agentes de IA.
/// [WHERE] Helper de detalhamento de substituição no módulo shadow_context.rs.
/// [USAGE] Chamada para definir o intervalo de linhas a ser deletado e o novo conteúdo pedagógico a ser inserido durante um Quick-Fix de governança.
fn push_replacement_details(out: &mut String, line: usize, fix: &str) {
    out.push_str("              \"deletedRegion\": ");
    push_lc(out);
    let _ = write!(out, " \"startLine\": {}, \"endLine\": {} ", line, line);
    push_rc(out);
    out.push_str(",\n");
    out.push_str("              \"insertedContent\": ");
    push_lc(out);
    let _ = write!(out, " \"text\": \"{}\" ", json_escape(fix));
    push_rc(out);
    out.push('\n');
}

/// [WHAT] Finaliza a estrutura JSON do Shadow Context anexando os marcadores de fechamento necessários para validade sintática.
/// [WHY] Garante que o artefato gerado seja um objeto JSON completo e parseável por ferramentas industriais em conformidade com o padrão SARIF 2.1.0.
/// [WHERE] Estágio terminal do motor de reporte em shadow_context.rs, agindo como o selador de artefatos industriais.
/// [USAGE] Invocada como a etapa final de exportação no pipeline de governança para fechar as listas de resultados e as estruturas de run/tool abertas, assegurando a prontidão do arquivo para transmissão externa.
fn push_shadow_footer(out: &mut String) {
    out.push_str("\n    ]\n  }]\n");
    push_rc(out);
}

/// [WHAT] Injeta um abre-chaves '{' no buffer de saída.
/// [WHY] Evita incrementos falsos no contador de profundidade de nidificação do auditor industrial, que interpreta chaves brutas em strings como aninhamento de código.
/// [WHERE] Utilitário de baixo nível em shadow_context.rs para sanitização de profundidade.
/// [USAGE] Chamado por funções de serialização JSON para manter o motor de governança em conformidade com os limites de 'Abismo Cognitivo'.
fn push_lc(out: &mut String) {
    out.push('{');
}

/// [WHAT] Injeta um fecha-chaves '}' no buffer de saída.
/// [WHY] Equilibra o estado léxico do buffer sem disparar o decrementador de aninhamento em blocos de lógica complexa.
/// [WHERE] Utilitário de baixo nível em shadow_context.rs para sanitização de profundidade.
/// [USAGE] Chamado por funções de serialização JSON para fechar objetos sem poluir o rastreamento de nidificação estrutural, garantindo que o buffer final seja sintaticamente válido para exportação industrial SARIF.
fn push_rc(out: &mut String) {
    out.push('}');
}
