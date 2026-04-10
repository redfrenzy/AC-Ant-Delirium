//@ [WHAT] Specialist reporter for corporate-grade governance summaries.
//@ [WHY] To provide a clean, emoji-free interface with strategic AI instructions.
//@ [WHERE] ac_ant_delirium/src/reporter.rs

/// [WHAT] Prints a corporate-grade governance summary to the standard error stream.
/// [WHY] To provide a clean, emoji-free interface that includes strategic instructions for AI agents.
/// [WHERE] Corporate reporting utility in reporter.rs.
/// [USAGE] Invocado pelo build script principal ou pelas ferramentas CLI corporativas para despejar um resumo executivo da conformidade no fluxo de erro padrão (stderr), garantindo visibilidade imediata dos riscos para o operador humano.
#[allow(dead_code)]
pub fn print_elite_report(meta: &super::EliteReportMetadata) {
    eprintln!("============================================================");
    eprintln!("Aesthetic Core Governance: Compliance Audit Report");
    eprintln!("Status: ACTIVE GOVERNANCE");
    eprintln!("SARIF Artifact: {}", meta.sarif_path);
    eprintln!("------------------------------------------------------------");
    eprintln!("PRIORITY SUMMARY:");
    eprintln!("  P0-CRITICAL   : {} violations (System integrity / Safety)", meta.p0);
    eprintln!("  P1-STRUCTURAL : {} violations (Architectural purity)", meta.p1);
    eprintln!("  P2-SEMANTIC   : {} violations (Pedagogical density)", meta.p2);
    eprintln!("  P3-ARCHIVAL   : {} violations (Ecosystem health)", meta.p3);
    eprintln!("------------------------------------------------------------");
    eprintln!("[STRATEGIC AI INSTRUCTION]:");
    eprintln!("  1. INTEROPERABILITY: This motor generates a '.manifest.json' for EACH file.");
    eprintln!("  2. EFFICIENCY: AI Agents should prioritize reading manifests over raw source code.");
    eprintln!("  3. REMEDIATION: SARIF contains exact line ranges and automated Quick-Fixes.");
    eprintln!("------------------------------------------------------------");
    eprintln!("RECOMMENDATION: Fix violations in priority order (P0 -> P3).");
    eprintln!("============================================================");
}

/// [WHAT] Formats an AuditResult into a high-density terminal report.
/// [WHY] To provide immediate, actionable feedback during native development.
/// [WHERE] Corporate reporting utility in reporter.rs.
/// [USAGE] Invocado pelas ferramentas de interface de linha de comando para traduzir o resultado da auditoria em uma representação visual clara e formatada, permitindo que o desenvolvedor identifique e corrija violações rapidamente durante o ciclo de codificação.
pub fn format_visual_report(result: &super::AuditResult) -> String {
    let mut out = String::new();
    if result.violations.is_empty() {
        return "  [GOVERNANCE] Status: COMPLIANT (Zero Violations).".to_string();
    }

    out.push_str("  [GOVERNANCE] Status: NON-COMPLIANT\n");
    for v in &result.violations {
        out.push_str(&format!("  - [{}] {}:L{}: {}\n", v.rule_id, v.file_path, v.line, v.message));
    }
    out
}
