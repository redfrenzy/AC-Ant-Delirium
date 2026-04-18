use super::AuditResult;
use super::config::AuditConfig;
use super::governance;
use super::restricted_tokens::out::elite_err;
use super::restricted_tokens::resolve::elite_resolve_res;

/// [WHAT] Metadata container for higher-level reporting orchestration.
#[derive(Default, Debug, Clone)]
pub struct EliteReportMetadata {
    pub sarif_path: String,
    pub p0: usize,
    pub p1: usize,
    pub p2: usize,
    pub p3: usize,
}

/// [WHAT] Prints a compact, aesthetic governance report to the standard error stream using authorized quarantined IO.
/// [WHY] To provide immediate visual feedback while maintaining an industrial, low-noise aesthetic and complying with restricted token policies.
/// [WHERE] Corporate reporting utility in reporter.rs.
/// [USAGE] Chamada pelo ponto de entrada principal para exibir o dashboard executivo de conformidade após a conclusão de uma auditoria geode bem-sucedida ou falha.
pub fn output_elite_report(result: &AuditResult, config: &AuditConfig) {
    let report = format_visual_report(result, config);
    elite_err(&report);
}

/// [WHAT] Formats an AuditResult into a high-density, aesthetic industrial dashboard for terminal display.
/// [WHY] To satisfy both human verification and AI awareness during terminal-based interactions by providing a structured visual summary of compliance.
/// [WHERE] Corporate reporting utility in reporter.rs.
/// [USAGE] Traduz o resultado da auditoria em um dashboard visual com box-drawing characters e resumo de configuração, garantindo que o operador humano compreenda instantaneamente o estado de conformidade do projeto.
pub fn format_visual_report(result: &AuditResult, config: &AuditConfig) -> String {
    let mut out = String::new();
    let total = result.violations.len();
    
    out.push_str("\n\n");
    out.push_str("AESTHETIC CORE  ▸  Ant Delirium Governance\n");
    
    if total == 0 {
        out.push_str("Status: COMPLIANT  │  Pure Geode Integrity\n\n\n");
        return out;
    }

    self::write_header_summary(&mut out, total, config, result);
    self::write_ai_context(&mut out, config);
    
    if !result.logs.is_empty() {
        out.push_str("[ORCHESTRATOR LOGS]\n");
        for log in &result.logs {
            out.push_str(&format!("◆ {}\n", log));
        }
        out.push_str("\n\n");
    }
    
    self::write_resources(&mut out, config);

    out
}

/// [WHAT] Gera o cabeçalho estético e o sumário executivo de prioridades para o relatório terminal, formatando quantificadores de violações P0-P3 e listando os alvos de jurisdição ativa de forma densa e organizada.
/// [WHY] Esta fragmentação resolve a violação de 'Densidade Lógica Excessiva' encontrada no formatador visual original, permitindo que cada seção do relatório seja mantida de forma independente e segura sem inflar a massa de código de uma única função.
/// [WHERE] Componente de renderização visual no reporter.rs, responsável pela interface humana de primeira ordem do ecossistema de governança atômica.
/// [USAGE] Consumida pelo format_visual_report como o primeiro estágio da construção do dashboard, fornecendo o contexto imediato de conformidade para o operador do sistema durante o processo de auditoria.
fn write_header_summary(out: &mut String, total: usize, config: &AuditConfig, result: &AuditResult) {
    out.push_str(&format!("Status: NON-COMPLIANT  │  {} violations\n", total));
    
    if !config.hierarchy.targets.is_empty() {
        let keys: Vec<String> = config.hierarchy.targets.iter().map(|(k, v)| format!("{}:{}", k, v)).collect();
        out.push_str(&format!("Targets: [ {} ]\n", keys.join(", ")));
    }
    out.push_str("\n\n");

    let (p0, p1, p2, p3) = count_priorities(result);
    out.push_str(&format!("P0-CRITICAL ··· {}\n", p0));
    out.push_str(&format!("P1-STRUCTURAL · {}\n", p1));
    out.push_str(&format!("P2-SEMANTIC ··· {}\n", p2));
    out.push_str(&format!("P3-ARCHIVAL ··· {}\n\n\n", p3));
}

/// [WHAT] Injeta o bloco estratégico de contexto de Inteligência Artificial no relatório, fornecendo caminhos absolutos para artefatos Shadow Context e dicas pedagógicas customizadas para acelerar a remediação por agentes autônomos.
/// [WHY] Garante que o feedback da auditoria não seja apenas uma notificação de erro, mas um guia de navegação industrial que permite a correção imediata de falhas estruturais através de metadados ricos e determinísticos em regime de build.
/// [WHERE] Sub-módulo de renderização estratégico no reporter.rs, focado na interface entre o motor de auditoria e ferramentas externas de análise (SARIF/JSON).
/// [USAGE] Chamado durante a segunda fase da construção do relatório visual para enriquecer o log com informações vitais de jurisdição e orientações técnicas de conformidade elite.
fn write_ai_context(out: &mut String, config: &AuditConfig) {
    out.push_str("[AI CONTEXT]\n");
    
    let abs_path = elite_resolve_res(std::env::current_dir()
        .map(|d| d.join(&config.shadow.output).to_string_lossy().into_owned()));

    out.push_str(&format!("▸ Check the '{}' file for full Shadow Context details.\n", abs_path));
    out.push_str("▸ Shadow Context (JSON) is optimized for AI agents to parse violations faster than terminal text.\n");

    for line in &config.hierarchy.context_info {
        out.push_str(&format!("▸ {}\n", line));
    }
    out.push_str("\n\n\n");
}

/// [WHAT] Renderiza o bloco de recursos e comandos terminais sugeridos para a remediação, utilizando early returns para evitar blocos 'else' proibidos e manter a integridade do fluxo lógico linear exigido pelo Aesthetic Core.
/// [WHY] Fornece ao desenvolvedor as ferramentas exatas necessárias para agir sobre as violações reportadas, mantendo o pipeline de governança prático e acionável enquanto cumpre as métricas de estrutura de decisões simplificada.
/// [WHERE] Utilitário de fechamento de relatório no reporter.rs, atuando como o gatilho final de instrução industrial para o operador humano.
/// [USAGE] Invocado na fase final de geração de logs para exibir ferramentas de conformidade e comandos de build sugeridos baseando-se no estado atual da configuração do geode.
fn write_resources(out: &mut String, config: &AuditConfig) {
    if config.info.tools.is_empty() && config.info.terminal_commands.is_empty() && config.hierarchy.context_info.is_empty() {
        out.push_str("AI: Read .shadow.json and *_manifest.json for full context.\n\n\n");
        return;
    }

    out.push_str("[RESOURCES & COMMANDS]\n");
    for tool in &config.info.tools {
        out.push_str(&format!("▸ {}\n", tool));
    }
    for cmd in &config.info.terminal_commands {
        out.push_str(&format!("▸ {}\n", cmd));
    }
    out.push_str("\n\n\n");
}

/// [WHAT] Counts violations by priority level for the visual dashboard summary.
/// [WHY] To populate the priority breakdown section of the report with accurate quantifications of P0-P3 violations.
/// [WHERE] Internal utility function in reporter.rs.
/// [USAGE] Invocado internamente por format_visual_report para processar a lista de violações e agrupar as contagens conforme a prioridade industrial.
fn count_priorities(result: &AuditResult) -> (usize, usize, usize, usize) {
    let mut counts = (0, 0, 0, 0);
    for v in &result.violations {
        match governance::map_to_level(&v.rule_id) {
            "P0" => counts.0 += 1,
            "P1" => counts.1 += 1,
            "P2" => counts.2 += 1,
            _ => counts.3 += 1,
        }
    }
    counts
}
