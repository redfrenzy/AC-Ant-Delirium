use super::{AuditResult, AuditViolation, json_escape, governance};
use super::config::AuditConfig;
use std::fmt::Write;

/// [WHAT] Orchestrates the transformation of audit findings into a Shadow Context (SARIF 2.1.0) report with AI context properties.
/// [WHY] To provide a unique, high-density operational context format for AI agents while maintaining industry-standard machine-readability.
/// [WHERE] Primary serialization entry point within the dedicated shadow_context.rs module.
/// [USAGE] Invocado pelo componente AuditResult como a ponte final de exportação para gerar um artefato Shadow Context (JSON) com properties de governança que a AI pode consumir diretamente.
pub fn to_shadow_context(result: &AuditResult, file_path: &str, config: &AuditConfig) -> String {
    let mut out = String::with_capacity(result.violations.len() * 512 + 2048);
    let relative_path = file_path.replace("\\", "/");

    push_shadow_header(&mut out);
    push_shadow_properties(&mut out, result, config);
    push_shadow_results(result, &mut out, &relative_path);
    push_shadow_footer(&mut out);
    
    out
}

/// [WHAT] Injeta o nível de severidade correspondente à prioridade industrial Aesthetic Core no formato Shadow Context.
/// [WHY] Para traduzir o sistema de prioridades P0-P3 para os níveis padrão consumíveis por IAs (error, warning, note).
/// [WHERE] Utilitário de mapeamento no shadow_context.rs.
/// [USAGE] Chamado internamente por push_shadow_results para classificar cada violação no nível correto.
fn map_p_level_to_shadow(rule_id: &str) -> &'static str {
    let p_level = governance::map_to_level(rule_id);
    match p_level {
        "P0" => "error",
        "P1" => "warning",
        "P2" => "note",
        _ => "note",
    }
}

/// [WHAT] Conta violações agrupadas por nível de prioridade industrial.
/// [WHY] Para fornecer um resumo quantitativo que a AI pode usar para priorizar correções.
/// [WHERE] Utilitário de contagem no shadow_context.rs.
/// [USAGE] Chamado por push_shadow_properties para popular o campo summary no properties bag.
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
}

/// [WHAT] Injects the governance properties bag into the Shadow Context run with AI context, priority summary, and applied configuration snapshot.
/// [WHY] To provide AI agents with a single source of truth containing operational context, tool references, and authorized commands directly within the Shadow Context artifact.
/// [WHERE] Properties serialization sub-pass in shadow_context.rs, placed between the tool header and results array.
/// [USAGE] Chamado por to_shadow_context() para injetar o bloco properties.governance com todas as informações da seção [info] do delirium.toml e um snapshot das configurações ativas.
fn push_shadow_properties(out: &mut String, result: &AuditResult, config: &AuditConfig) {
    let (p0, p1, p2, p3) = count_by_level(&result.violations);
    let total = result.violations.len();

    out.push_str("    \"properties\": {\n");
    out.push_str("      \"governance\": {\n");

    // Priority order
    let priority = if config.info.priority_order.is_empty() {
        "P0 → P1 → P2 → P3"
    } else {
        &config.info.priority_order // [REASON] Config may override default
    };
    let _ = write!(out, "        \"priority_order\": \"{}\",\n", json_escape(priority));

    // Summary counts
    let _ = write!(out, "        \"total_violations\": {},\n", total);
    let _ = write!(out, "        \"summary\": {{ \"P0\": {}, \"P1\": {}, \"P2\": {}, \"P3\": {} }},\n", p0, p1, p2, p3);

    // Context array from [info.context]
    push_string_array(out, "context", &config.info.context, true);

    // Tools array from [info.tools]
    push_string_array(out, "tools", &config.info.tools, true);

    // Terminal commands from [info.terminal_commands]
    push_string_array(out, "terminal_commands", &config.info.terminal_commands, true);

    // Applied config snapshot
    out.push_str("        \"applied_config\": {\n");
    let _ = write!(out, "          \"max_lines\": {},\n", config.rules.limits.max_lines);
    let _ = write!(out, "          \"max_nesting\": {},\n", config.rules.limits.max_nesting);
    let _ = write!(out, "          \"max_params\": {},\n", config.rules.limits.max_params);
    let _ = write!(out, "          \"logical_density\": {},\n", config.rules.limits.logical_density);

    // Restricted tokens
    let tokens: Vec<&String> = config.rules.restricted_tokens.keys().collect();
    let tokens_str: Vec<String> = tokens.iter().map(|t| format!("\"{}\"", json_escape(t))).collect();
    let _ = write!(out, "          \"restricted_tokens\": [{}],\n", tokens_str.join(", "));

    // Allowed crates
    let crates_str: Vec<String> = config.rules.allowed_crates.iter().map(|c| format!("\"{}\"", json_escape(c))).collect();
    let _ = write!(out, "          \"allowed_crates\": [{}]\n", crates_str.join(", "));

    out.push_str("        }\n");
    out.push_str("      }\n");
    out.push_str("    },\n");
    out.push_str("    \"results\": [");
}

/// [WHAT] Serializa um array de strings JSON com indentação consistente no formato Shadow Context.
/// [WHY] Para evitar repetição de código na serialização de context, tools e terminal_commands.
/// [WHERE] Utilitário interno do shadow_context.rs.
/// [USAGE] Chamado por push_shadow_properties para cada array de strings da seção [info].
fn push_string_array(out: &mut String, key: &str, items: &[String], trailing_comma: bool) {
    let _ = write!(out, "        \"{}\": [", key);
    if items.is_empty() {
        out.push(']');
    } else { // [REASON] Non-empty arrays need multi-line formatting
        out.push('\n');
        for (i, item) in items.iter().enumerate() {
            let comma = if i < items.len() - 1 { "," } else { "" }; // [REASON] Last item has no comma
            let _ = write!(out, "          \"{}\"{}\n", json_escape(item), comma);
        }
        out.push_str("        ]");
    }
    if trailing_comma {
        out.push_str(",\n");
    } else {
        out.push('\n');
    }
}

/// [WHAT] Iterates through the violation set and encodes each entry as a machine-readable Shadow Context result.
/// [WHY] To translate the geode's internal audit findings into a standardized format for external consumption.
/// [WHERE] Primary data processing sub-pass of the Shadow Context engine in shadow_context.rs.
/// [USAGE] Invocado pela função orquestradora to_shadow_context() para iterar sobre violações e traduzi-las em resultados individuais.
fn push_shadow_results(result: &AuditResult, out: &mut String, _relative_path: &str) {
    for (i, v) in result.violations.iter().enumerate() {
        let level = map_p_level_to_shadow(&v.rule_id);
        let msg = json_escape(&v.message);
        
        let _ = write!(out, r#"
      {{
        "ruleId": "{}",
        "level": "{}",
        "message": {{ "text": "{}" }},
        "locations": [{{
          "physicalLocation": {{
            "artifactLocation": {{ "uri": "{}" }},
            "region": {{ "startLine": {}"#, v.rule_id, level, msg, v.file_path, v.line);

        push_shadow_snippet(out, v);
        push_shadow_fixes(out, v, &v.file_path);

        let separator = match i < result.violations.len() - 1 {
            true => ",",
            false => "",
        };
        let _ = write!(out, " }}{}", separator);
    }
}

/// [WHAT] Appends an optional code snippet context to a specific Shadow Context result location entry.
/// [WHY] To provide immediate source code visibility for the violation within the reporting dashboard.
/// [WHERE] Secondary location encoding pass for individual results in shadow_context.rs.
/// [USAGE] Invocado durante o processamento de resultados para anexar o conteúdo do trecho de código afetado.
fn push_shadow_snippet(out: &mut String, v: &AuditViolation) {
    if let Some(ref snip) = v.snippet {
        let _ = write!(out, r#", "snippet": {{ "text": "{}" }}"#, json_escape(snip));
    }
    out.push_str(" } } }]");
}

/// [WHAT] Appends automated Quick-Fix suggestions to a machine-readable Shadow Context violation record.
/// [WHY] To facilitate rapid remediation of governance findings through automated editor integrations.
/// [WHERE] Remediation encoding pass for individual results in shadow_context.rs.
/// [USAGE] Invocado para anexar sugestões de reparo automático ao registro de violação.
fn push_shadow_fixes(out: &mut String, v: &AuditViolation, relative_path: &str) {
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

/// [WHAT] Finalizes the Shadow Context JSON structure by appending the required closing bracket markers.
/// [WHY] To ensure that the generated report is syntactically valid and complies with industry standards.
/// [WHERE] Termination sub-pass of the Shadow Context serialization engine in shadow_context.rs.
/// [USAGE] Invocado como etapa terminal no pipeline para fechar as estruturas JSON abertas.
fn push_shadow_footer(out: &mut String) {
    out.push_str("\n    ]\n  }]\n}");
}
