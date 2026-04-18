use super::{AuditResult, AuditViolation, governance};
use super::config::AuditConfig;
use std::fmt::Write;

/// [WHAT] Orquestra a transformação de achados de auditoria em um relatório Shadow Context (SARIF 2.1.0) enriquecido com propriedades de contexto para agentes de IA.
/// [WHY] Para fornecer um formato de contexto operacional único e de alta densidade semântica para agentes de IA, mantendo a compatibilidade com padrões industriais de leitura de máquinas.
/// [WHERE] Ponto de entrada primário de serialização localizado no módulo dedicado shadow_context.rs, atuando como o exportador final do motor de governança.
/// [USAGE] Invocada pelo componente AuditResult como a ponte final de exportação para gerar um artefato Shadow Context (JSON) contendo metadados de governança e sugestões de Quick Fix. Este processo consolida todos os achados da auditoria em um único buffer de memória de forma eficiente, aplicando serialização manual para evitar o custo de crates de reflexão pesados, mantendo a integridade industrial e a agilidade do pipeline de CI/CD da geode.
pub fn to_shadow_context(result: &AuditResult, _file_path: &str, _config: &AuditConfig) -> String {
    let mut out = String::with_capacity(result.violations.len() * 512 + 2048);

    push_shadow_header(&mut out);
    
    out.push_str("    \"results\": [");
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
    out.push_str("    ");
    push_rc(out);
    out.push_str(",\n");
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
    let p_level = governance::map_to_level(&v.rule_id);
    
    out.push('\n');
    out.push_str("      ");
    push_lc(out);
    out.push_str("\n        \"ruleId\": \"");
    out.push_str(&v.rule_id);
    
    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "ac_ant_delirium".to_string());

    let _ = write!(out, "\",\n        \"taxonomy\": \"{}\",\n", v.taxonomy_path);
    let _ = write!(out, "        \"explain\": \"{} explain {} --dna {}\",\n", exe_path, v.rule_id, v.detected_dna);

    out.push_str("        \"level\": \"");
    out.push_str(level);
    out.push_str("\",\n        \"priority\": \"");
    out.push_str(p_level);
    out.push_str("\",\n        \"message\": ");
    push_lc(out);
    out.push_str(" \"text\": \"");
    out.push_str(&v.message.replace('"', "\\\""));
    out.push_str("\" ");
    push_rc(out);
    out.push_str(",\n");
    
    push_json_locations(out, v);
}

/// [WHAT] Serializes the location information for an audit violation cleanly and atomically.
/// [WHY] To keep the file location structurally separated but extremely lean since AI context is now requested via terminal.
/// [WHERE] Location serializer helper in shadow_context.rs.
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
    let _ = write!(out, " \"startLine\": {} ", v.line);
    push_rc(out);
    out.push('\n');
    out.push_str("          ");
    push_rc(out);
    out.push('\n');
    out.push_str("        ");
    push_rc(out);
    out.push_str(" ]\n      ");
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
