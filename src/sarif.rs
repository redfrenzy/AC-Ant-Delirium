use super::{AuditResult, AuditViolation, map_rule_to_level, json_escape};
use std::fmt::Write;

/// [WHAT] Orchestrates the transformation of audit findings into a SARIF 2.1.0 report.
/// [WHY] To provide a standardized, machine-readable format for external CI/CD integration and dashboarding.
/// [WHERE] Primary serialization entry point within the dedicated sarif.rs module.
/// [USAGE] Invocado pelo componente AuditResult como a ponte final de exportação para gerar um artefato SARIF JSON de alta fidelidade que descreve todas as violações descobertas de forma legível por máquinas para integração em pipelines de CI/CD.
pub fn to_sarif(result: &AuditResult, file_path: &str) -> String {
    let mut out = String::with_capacity(result.violations.len() * 512 + 1024);
    let relative_path = file_path.replace("\\", "/");

    push_sarif_header(&mut out);
    push_sarif_results(result, &mut out, &relative_path);
    push_sarif_footer(&mut out);
    
    out
}

/// [WHAT] Injects the SARIF 2.1.0 header and tool identity metadata into the provided output buffer.
/// [WHY] To establish the required schema reference and identify the auditor as ac_ant_delirium.
/// [WHERE] Initialization sub-pass of the SARIF serialization engine in sarif.rs.
/// [USAGE] Invocado como a etapa inicial obrigatória na sequência de serialização do motor SARIF para garantir que o cabeçalho do log e a identificação da ferramenta ac_ant_delirium estejam presentes no início do buffer de saída JSON.
fn push_sarif_header(out: &mut String) {
    out.push_str("{\n");
    out.push_str("  \"version\": \"2.1.0\",\n");
    out.push_str("  \"$schema\": \"https://schemastore.azurewebsites.net/schemas/json/sarif-2.1.0-rtm.4.json\",\n");
    out.push_str("  \"runs\": [{\n");
    out.push_str("    \"tool\": {\n");
    out.push_str("      \"driver\": {\n");
    out.push_str("        \"name\": \"ac_ant_delirium\",\n");
    out.push_str("        \"version\": \"0.1.0\"\n");
    out.push_str("      }\n");
    out.push_str("    },\n");
    out.push_str("    \"results\": [");
}

/// [WHAT] Iterates through the violation set and encodes each entry as a machine-readable SARIF result.
/// [WHY] To translate the geode's internal audit findings into a standardized format for external consumption.
/// [WHERE] Primary data processing sub-pass of the SARIF engine located in sarif.rs.
/// [USAGE] Invocado pela função orquestradora to_sarif() para iterar sobre a coleção completa de registros AuditViolation e traduzi-los em entradas de resultados SARIF individuais, preservando o nível de severidade e o contexto da violação.
fn push_sarif_results(result: &AuditResult, out: &mut String, relative_path: &str) {
    for (i, v) in result.violations.iter().enumerate() {
        let level = map_rule_to_level(v.rule_id);
        let msg = json_escape(&v.message);
        
        let _ = write!(out, r#"
      {{
        "ruleId": "{}",
        "level": "{}",
        "message": {{ "text": "{}" }},
        "locations": [{{
          "physicalLocation": {{
            "artifactLocation": {{ "uri": "{}" }},
            "region": {{ "startLine": {}"#, v.rule_id, level, msg, relative_path, v.line);

        push_sarif_snippet(out, v);
        push_sarif_fixes(out, v, relative_path);

        let separator = match i < result.violations.len() - 1 {
            true => ",",
            false => "",
        };
        let _ = write!(out, " }}{}", separator);
    }
}

/// [WHAT] Appends an optional code snippet context to a specific SARIF result location entry.
/// [WHY] To provide immediate source code visibility for the violation within the reporting dashboard.
/// [WHERE] Secondary location encoding pass for individual results in sarif.rs.
/// [USAGE] Invocado durante o processamento de resultados individuais (push_sarif_results) para anexar o conteúdo bruto do trecho de código afetado pela violação, fornecendo contexto visual imediato para o desenvolvedor final no visualizador de diagnóstico.
fn push_sarif_snippet(out: &mut String, v: &AuditViolation) {
    if let Some(ref snip) = v.snippet {
        let _ = write!(out, r#", "snippet": {{ "text": "{}" }}"#, json_escape(snip));
    }
    out.push_str(" } } }]");
}

/// [WHAT] Appends automated Quick-Fix suggestions to a machine-readable SARIF violation record.
/// [WHY] To facilitate rapid remediation of governance findings through automated editor integrations.
/// [WHERE] Remediation encoding pass for individual results in sarif.rs.
/// [USAGE] Invocado pela lógica de serialização de resultados para anexar possíveis sugestões de reparo automático (Quick-Fixes) ao registro SARIF, permitindo que IDEs compatíveis ofereçam correções automáticas baseadas nas regras de governança Elite.
fn push_sarif_fixes(out: &mut String, v: &AuditViolation, relative_path: &str) {
    if let Some(ref fix) = v.quick_fix {
        let _ = write!(out, r#",
        "fixes": [{{
          "description": {{ "text": "Apply exact governance stubs" }},
          "artifactChanges": [{{
            "artifactLocation": {{ "uri": "{}" }},
            "replacements": [{{
              "deletedRegion": {{ "startLine": {}, "endLine": {} }},
              "insertedContent": {{ "text": "{}" }}
            }}]
          }}]
        }}]"#, relative_path, v.line, v.line, json_escape(fix));
    }
}

/// [WHAT] Finalizes the SARIF JSON structure by appending the required closing bracket markers.
/// [WHY] To ensure that the generated report is syntactically valid and complies with JSON standards.
/// [WHERE] Termination sub-pass of the SARIF serialization engine in sarif.rs.
/// [USAGE] Invocado como a etapa terminal no pipeline de serialização SARIF para fechar as estruturas JSON abertas e garantir que o documento final seja sintaticamente válido e esteja pronto para persistência ou transmissão para o host.
fn push_sarif_footer(out: &mut String) {
    out.push_str("\n    ]\n  }]\n}");
}
