use super::AuditResult;
use super::config::AuditConfig;
use super::governance;

/// [WHAT] Metadata container for higher-level reporting orchestration.
#[derive(Default, Debug, Clone)]
pub struct EliteReportMetadata {
    pub sarif_path: String,
    pub p0: usize,
    pub p1: usize,
    pub p2: usize,
    pub p3: usize,
}

/// [WHAT] Prints a compact, aesthetic governance report to the standard error stream.
/// [WHY] To provide immediate visual feedback while maintaining an industrial, low-noise aesthetic.
/// [WHERE] Corporate reporting utility in reporter.rs.
/// [USAGE] Chamada pelo ponto de entrada principal para exibir o dashboard executivo de conformidade.
pub fn print_elite_report(result: &AuditResult, config: &AuditConfig) {
    let report = format_visual_report(result, config);
    eprintln!("{}", report);
}

/// [WHAT] Formats an AuditResult into a high-density, aesthetic industrial dashboard.
/// [WHY] To satisfy both human verification and AI awareness during terminal-based interactions.
/// [WHERE] Corporate reporting utility in reporter.rs.
/// [USAGE] Traduz o resultado da auditoria em um dashboard visual com box-drawing characters e resumo de configuração.
pub fn format_visual_report(result: &AuditResult, config: &AuditConfig) -> String {
    let mut out = String::new();
    let total = result.violations.len();
    
    // 1. Header (Flush left, 2 lines of space above)
    out.push_str("\n\n");
    out.push_str("AESTHETIC CORE  ▸  Ant Delirium Governance\n");
    
    if total == 0 {
        out.push_str("Status: COMPLIANT  │  Pure Geode Integrity\n\n\n");
        return out;
    }

    out.push_str(&format!("Status: NON-COMPLIANT  │  {} violations\n\n\n", total));

    // 2. Priority Summary (Vertical list)
    let (p0, p1, p2, p3) = count_priorities(result);
    out.push_str(&format!("P0-CRITICAL ··· {}\n", p0));
    out.push_str(&format!("P1-STRUCTURAL · {}\n", p1));
    out.push_str(&format!("P2-SEMANTIC ··· {}\n", p2));
    out.push_str(&format!("P3-ARCHIVAL ··· {}\n\n\n", p3));

    // 3. (Applied Config Snapshot removed from terminal - remains in SARIF)

    // 4. AI Strategic context (Dynamic from TOML + Mandatory briefings)
    if true { // Always show AI Context header if we have shadow config
        out.push_str("[AI CONTEXT]\n");
        
        // Mandatory briefing 1: Absolute path pointer for zero-latency AI discovery
        let abs_path = std::env::current_dir()
            .map(|d| d.join(&config.shadow.output).to_string_lossy().into_owned())
            .unwrap_or_else(|_| config.shadow.output.clone());

        out.push_str(&format!("▸ Check the '{}' file for full Shadow Context details.\n", abs_path));
        
        // Mandatory briefing 2: Efficiency justification
        out.push_str("▸ Shadow Context (JSON) is optimized for AI agents to parse violations faster than terminal text.\n");

        for line in &config.info.context {
            out.push_str(&format!("▸ {}\n", line));
        }
        out.push_str("\n\n\n");
    }

    if !config.info.tools.is_empty() || !config.info.terminal_commands.is_empty() {
        out.push_str("[RESOURCES & COMMANDS]\n");
        for tool in &config.info.tools {
            out.push_str(&format!("▸ {}\n", tool));
        }
        for cmd in &config.info.terminal_commands {
            out.push_str(&format!("▸ {}\n", cmd));
        }
        out.push_str("\n\n");
    } else {
        out.push_str("AI: Read .shadow.json and *_manifest.json for full context.\n\n\n");
    }

    out
}

/// [WHAT] Counts violations by priority level for the visual dashboard.
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
