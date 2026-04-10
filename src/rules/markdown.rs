use super::super::AuditViolation;
use super::super::config::AuditConfig;
use super::super::rules_logs::messages;
use std::path::Path;

/// [WHAT] Contexto de auditoria de documentação markdown utilizado para transportar metadados de arquivos pareados, configurações de governança e repositório de captura de violações industriais.
/// [WHY] Permite que as regras de arquivamento (P3) operem sob limites industriais de acoplamento rigorosos (rule_params), consolidando o estado da sincronia documental entre o código-fonte e os arquivos .md.
/// [WHERE] Camada de transporte de dados do motor de arquivamento pedagógico de alta disponibilidade localizada no arquivo markdown.rs.
/// [USAGE] Instanciado durante a fase de auditoria de ecossistema para centralizar a validação de existência, densidade e obsolescência de arquivos de documentação externa pertencentes ao geode.
pub struct MarkdownAuditCtx<'a> {
    pub file_path: &'a str,
    pub config: &'a AuditConfig,
    pub violations: &'a mut Vec<AuditViolation>,
}

/// [WHAT] Verificador de existência e qualidade de documentação pareada markdown que sincroniza o estado narrativo do geode com seu código-fonte para fins de arquivamento P3.
/// [WHY] Para assegurar que cada geode possua documentação técnica durável fora do código-fonte, facilitando a indexação por sistemas de conhecimento e mitigando a perda de contexto.
/// [WHERE] Ponto de entrada da auditoria P3-Archival em markdown.rs, integrado à fase de ecossistema para validação robusta de ativos pedagógicos externos e sua integridade temporal.
/// [USAGE] Chamado durante a fase de auditoria de ecossistema para validar o pareamento .rs -> .md, garantindo que nenhum projeto industrial opere sem seu correspondente descritivo obrigatório.
pub fn verify(ctx: &mut MarkdownAuditCtx<'_>) {
    if ctx.file_path.ends_with(".md") { return; }

    let md_path_str = ctx.file_path.replace(".rs", ".md");
    let md_path = Path::new(&md_path_str);
    let rs_path = Path::new(ctx.file_path);

    if !md_path.exists() {
        push_archival_violation(ctx, messages::MD_PAIRING_MISSING);
        return;
    }

    validate_markdown_content(ctx, md_path);
    check_staleness(ctx, rs_path, md_path);
}

/// [WHAT] Validates the content length of a markdown file to ensure pedagogical substance and prevent the creation of empty or low-value documentation files that bypass the scanner.
/// [WHY] Para assegurar que a documentação externa não seja apenas um placeholder, mas sim um ativo pedagógico valioso que contenha informações técnicas reais sobre o componente auditado.
/// [WHERE] Quality control stage of the archival audit in markdown.rs, providing a density-aware filter for all discovered .md assets within the geode's directory structure.
/// [USAGE] Called by verify() after confirming the existence of a .md companion to ensure that the file contains sufficient semantic information to satisfy Elite standards.
fn validate_markdown_content(ctx: &mut MarkdownAuditCtx<'_>, path: &Path) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    if content.trim().len() < 256 {
        push_archival_violation(ctx, messages::EMPTY_DOCUMENTATION);
    }
}

/// [WHAT] Persists a discovered archival non-conformity into the global violation buffer, tagging it with the correct line and rule identifier for reporting.
/// [WHY] To centralize diagnostic reporting for modular P3-Archival checks while maintaining logical separation between the detection logic and the error reporting infrastructure.
/// [WHERE] Diagnostic reporting bridge in markdown.rs, serving as the egress point for all archival-related non-compliance events discovered during the scan phase.
/// [USAGE] Invocada pelos validadores de arquivamento para registrar pares de documentação ausentes ou obsoletos, garantindo que o relatório final contenha referências de caminho precisas.
fn push_archival_violation(ctx: &mut MarkdownAuditCtx<'_>, message: &str) {
    ctx.violations.push(AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: crate::governance::rule_id_atomic("rule_md_pairing"), // Unified rule_id for consistency
        message: message.to_string(),
        snippet: None,
        quick_fix: None,
    });
}

/// [WHAT] Detects modification drift between source code and its markdown documentation companion by comparing the last modification timestamps of both files in the geode tree.
/// [WHY] Para impedir a degradação da fidelidade documental, forçando uma sincronia constante entre o código industrial e seu par narrativo dentro da janela de conformidade de 48 horas.
/// [WHERE] Final stage of the P3-Archival verification process in markdown.rs, providing a temporal synchronization check for all paired pedagogical assets.
/// [USAGE] Invocada pelo método verify() para garantir que a documentação externa reflita as alterações mais recentes do código-fonte, disparando violações caso o desvio temporal seja excessivo.
fn check_staleness(ctx: &mut MarkdownAuditCtx<'_>, rs_path: &Path, md_path: &Path) {
    let rs_meta = match rs_path.metadata() { Ok(m) => m, Err(_) => return };
    let md_meta = match md_path.metadata() { Ok(m) => m, Err(_) => return };
    
    let rs_time = match rs_meta.modified() { Ok(t) => t, Err(_) => return };
    let md_time = match md_meta.modified() { Ok(t) => t, Err(_) => return };

    let drift = match rs_time.duration_since(md_time) {
        Ok(d) => d,
        Err(_) => return, // Documentation is newer or same age
    };

    // 48-hour rule (172,800 seconds)
    if drift.as_secs() > 172_800 {
        push_archival_violation(ctx, messages::STALE_DOCUMENTATION);
    }
}
