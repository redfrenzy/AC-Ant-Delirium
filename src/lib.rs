pub mod config;
pub mod rules;
pub mod rules_logs;
pub mod parser;
pub mod reporter;

use config::AuditConfig;

#[derive(Default, Debug, Clone)]
pub struct AuditViolation {
    pub line: usize,
    pub rule_id: &'static str,
    pub message: String,
    pub snippet: Option<String>,
    pub quick_fix: Option<String>,
}

#[derive(Default, Debug, Clone)]
pub struct AuditResult {
    pub violations: Vec<AuditViolation>,
    pub logs: Vec<String>,
}

impl AuditResult {
    /// [WHAT] Formats the entire audit collection into a standardized SARIF 2.1.0 compliant JSON string using a high-performance, single-buffer serialization engine.
    /// [WHY] To minimize heap allocation pressure and fragmentation by avoiding temporary `format!` results and string concatenations. This "zero-trash" approach is critical for maintaining real-time LSP responsiveness in large-scale Aesthetic Core deployments.
    /// [WHERE] Primary serialization method in `lib.rs`.
    pub fn to_sarif(&self, file_path: &str) -> String {
        use std::fmt::Write;
        
        // Pre-allocate buffer based on rule of thumb (512 bytes per violation + boilerplate)
        let mut out = String::with_capacity(self.violations.len() * 512 + 1024);
        let relative_path = file_path.replace("\\", "/");

        let _ = write!(out, r#"{{
  "version": "2.1.0",
  "$schema": "https://schemastore.azurewebsites.net/schemas/json/sarif-2.1.0-rtm.4.json",
  "runs": [{{
    "tool": {{
      "driver": {{
        "name": "ac_ant_delirium",
        "version": "0.1.0"
      }}
    }},
    "results": ["#);

        for (i, v) in self.violations.iter().enumerate() {
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

            if let Some(ref snip) = v.snippet {
                let _ = write!(out, r#", "snippet": {{ "text": "{}" }}"#, json_escape(snip));
            }
            let _ = write!(out, " }} }} }}]");

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

            let _ = write!(out, " }}{}", if i < self.violations.len() - 1 { "," } else { "" });
        }

        let _ = write!(out, r#"
    ]
  }}]
}}"#);
        out
    }
}

/// [WHAT] Maps an internal governance rule identifier to its corresponding SARIF severity level based on the rule's architectural impact and safety criticality across the Aesthetic Core ecosystem. This function ensures that high-priority violations like layering breaches or poison token usage are correctly categorized as 'error', while structural concerns are marked as 'warning' and semantic or archival issues are assigned the 'note' level for appropriate downstream processing by security tools.
/// [WHY] To provide a standardized and deterministic mapping between the geode's proprietary priority system and the industry-standard SARIF severity levels. This consistency is vital for maintaining a high signal-to-noise ratio in professional development environments, allowing AI agents and human developers to prioritize critical safety remediations over non-blocking stylistic or documentation-based improvements while ensuring full compatibility with automated compliance dashboards.
/// [WHERE] Invoked as a core categorization utility within the `to_sarif` serialization method in `lib.rs`, which is responsible for transforming raw audit findings into standardized diagnostic report entries for external consumption.
fn map_rule_to_level(rule_id: &str) -> &str {
    if rule_id == "rule_deps" || rule_id == "rule_poison_tokens" || rule_id == "rule_no_raw_print" || rule_id == "rule_layering" {
        return "error";
    }
    if rule_id == "rule_else_free" || rule_id == "rule_nesting" || rule_id == "rule_logical_density" {
        return "warning";
    }
    "note"
}

/// [WHAT] Implements a robust, character-by-character JSON escaping engine that ensures full compliance with RFC 8259 without external dependencies. This function iterates through each character of the input string and applies specific escape sequences for backslashes, quotes, and common control characters.
/// [WHY] To provide industrial-grade sanitization of source code snippets and audit messages, ensuring that the generated SARIF reports are always syntactically valid and machine-readable.
/// [WHERE] Primary string sanitization utility used throughout the `to_sarif` serialization pipeline in `lib.rs`.
fn json_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        if !apply_escape(c, &mut escaped) {
            escaped.push(c);
        }
    }
    escaped
}

/// [WHAT] Applies a JSON-specific escape sequence for a single character if required by RFC 8259. It handles standard escapes like quotes and backslashes, and delegates Unicode encoding for control characters.
/// [WHY] To keep the primary character-walking loop flat and comply with architectural nesting limits while ensuring full industrial coverage.
/// [WHERE] Helper function for `json_escape` in `lib.rs`.
fn apply_escape(c: char, out: &mut String) -> bool {
    match c {
        '\\' => out.push_str("\\\\"),
        '"' => out.push_str("\\\""),
        '\n' => out.push_str("\\n"),
        '\r' => {}, // Normalized by removal
        '\t' => out.push_str("\\t"),
        '\x08' => out.push_str("\\b"),
        '\x0c' => out.push_str("\\f"),
        c if c < '\x20' => push_unicode_escape(c, out),
        _ => return false,
    }
    true
}

/// [WHAT] Encodes a low-level ASCII control character into a standard JSON `\uXXXX` Unicode escape sequence.
/// [WHY] To provide a robust and surgical encoding mechanism that is both flat and zero-dependency.
/// [WHERE] Specialist helper for `apply_escape` in `lib.rs`.
fn push_unicode_escape(c: char, out: &mut String) {
    let code = c as u32;
    out.push_str("\\u00");
    let hex = b"0123456789abcdef";
    out.push(hex[(code >> 4) as usize] as char);
    out.push(hex[(code & 0xf) as usize] as char);
}
/// [WHAT] The main audit entry point for WASM-based governance.
/// [WHY] Orchestrates all governance rules in a single dependency-free call for AI compliance.
/// [WHERE] Primary bridge between the Host Orchestrator and the Geode.
/// [USAGE] Called by the WASM host with source code, file path, and configuration TOML strings.
pub fn audit_code(source: &str, file_path: &str, config_toml: &str) -> String {
    let config = config::AuditConfig::parse_brute(config_toml);
    let mut result = AuditResult::default();
    
    // Centralized Scanning (Audit 4 Optimization)
    let functions = parser::CodeScanner::scan_functions(source);

    // Stage 1: Structure & Safety
    audit_structural(source, file_path, &config, &functions, &mut result);

    // Stage 2: Documentation & Quality
    audit_pedagogical(file_path, &config, &functions, &mut result);

    // Stage 3: Ecosystem & Health (MD pairing, deps, manifest)
    audit_ecosystem(source, file_path, &config, &functions, &mut result);

    // Stage 4: Memory Decay (Final Mutation)
    if let Some(mutated_source) = trigger_memory_decay(source, &functions, &result.violations) {
        let _ = std::fs::write(file_path, mutated_source);
        result.logs.push(format!("[LOG] Memory Decay triggered for {}. Doc tags wiped due to severe violations.", file_path));
    }

    result.to_sarif(file_path)
}

/// [WHAT] Executa a fase de auditoria estrutural e de segurança.
/// [WHY] Isola verificações P0/P1 para manter a densidade lógica do orquestrador sob controle.
/// [WHERE] Camada interna de processamento do lib.rs.
fn audit_structural(source: &str, file_path: &str, config: &AuditConfig, functions: &[parser::ScannedFunction], result: &mut AuditResult) {
    if !is_rule_ignored(file_path, "rule_nesting", config) {
        rules::structure::verify(file_path, source, functions, config, &mut result.violations);
    }
    if !is_rule_ignored(file_path, "rule_poison_tokens", config) {
        rules::tokens::verify(file_path, source, config, &mut result.violations);
    }
    if !is_rule_ignored(file_path, "rule_macro_isolation", config) {
        rules::macros::verify(source, config, &mut result.violations);
    }
}

/// [WHAT] Executa a fase de auditoria pedagógica e semântica.
/// [WHY] Garante que a qualidade da documentação e justificativas atenda aos critérios P2.
/// [WHERE] Camada interna de processamento do lib.rs.
fn audit_pedagogical(file_path: &str, config: &AuditConfig, functions: &[parser::ScannedFunction], result: &mut AuditResult) {
    if !is_rule_ignored(file_path, "rule_doc_tags", config) {
        rules::docs::verify(functions, config, &mut result.violations);
        rules::quality::verify(functions, config, &mut result.violations);
    }
}

/// [WHAT] Executa a fase de auditoria de ecossistema e saúde archival.
/// [WHY] Valida o pareamento de arquivos MD, dependências do Cargo e gera o manifesto para IA.
/// [WHERE] Camada de fechamento do pipeline no lib.rs.
fn audit_ecosystem(source: &str, file_path: &str, config: &AuditConfig, functions: &[parser::ScannedFunction], result: &mut AuditResult) {
    if !is_rule_ignored(file_path, "rule_line_limit", config) {
        rules::metrics::verify(source, functions, config, &mut result.violations);
    }
    if !is_rule_ignored(file_path, "rule_md_pairing", config) {
        rules::markdown::verify(file_path, source, config, &mut result.violations);
    }
    if file_path.ends_with("Cargo.toml") && !is_rule_ignored(file_path, "rule_deps", config) {
        rules::deps::verify(file_path, source, config, &mut result.violations);
    }
    if config.manifest.enabled {
        result.logs.push(format!("[LOG] AI Manifest generated for {}", file_path));
        let manifest = rules::manifest::generate(source, config);
        export_manifest(file_path, &manifest);
    }
}

/// [WHAT] Exporta fisicamente o Manifesto Semântico em formato JSON escapado.
/// [WHY] Fornece contexto estruturado para agentes externos e IAs consumirem a arquitetura.
/// [WHERE] Função utilitária de saída em lib.rs.
fn export_manifest(file_path: &str, manifest: &[rules::manifest::SymbolInfo]) {
    let manifest_path = format!("{}.manifest.json", file_path);
    let mut json_out = String::from("[\n");
    for (idx, sym) in manifest.iter().enumerate() {
        let mut comma = ",";
        if idx == manifest.len() - 1 { comma = ""; }
        let escaped_narrative = json_escape(&sym.narrative);
        let _ = std::fmt::write(&mut json_out, format_args!(
            "  {{ \"name\": \"{}\", \"kind\": \"{}\", \"range\": [{}, {}], \"complexity\": {}, \"narrative\": \"{}\" }}{}\n",
            sym.name, sym.kind, sym.range.0, sym.range.1, sym.complexity, escaped_narrative, comma
        ));
    }
    json_out.push_str("]");
    let _ = std::fs::write(manifest_path, json_out);
}

/// [WHAT] Verifies if a path is eligible for auditing.
/// [WHY] To support granular directory-based governance and filtered audits.
/// [WHERE] Called during the configuration discovery and execution phase.
pub fn is_path_active(path: &str, config: &AuditConfig) -> bool {
    if config.active_to.is_empty() { return true; }
    for active_path in config.active_to.keys() {
        if path.contains(active_path) { return true; } 
    }
    false
}

/// [WHAT] Checks if a specific rule should be skipped for a file.
/// [WHY] To allow localized exceptions for legacy or boilerplate code in Elite geodes.
/// [WHERE] Called before each rule execution in the audit pipeline.
pub fn is_rule_ignored(path: &str, rule_id: &str, config: &AuditConfig) -> bool {
    if let Some(ignored_rules) = config.ignore_to.get(path) {
        if ignored_rules.contains(&"all".to_string()) || ignored_rules.contains(&rule_id.to_string()) {
            return true;
        }
    }
    false
}



/// [WHAT] Implements the architectural 'Memory Decay' mechanism designed to handle severe structural and safety violations by intentionally purging pedagogical documentation. This function iterates through all identified code blocks, correlates them with severe violation reports, and determines which functions require a documentation reset. It acts as a corrective mutation layer that ensures that code with critical flaws is not allowed to persist with stale or misleading explanations, effectively forcing a human or AI operator to re-verify the logic after the primary structural issues have been remediated.
/// [WHY] To prevent the accumulation of documentation staleness in Elite geodes where implementation integrity is paramount. By wiping the [WHAT], [WHY], and [WHERE] tags, the engine creates a localized 'semantic vacuum' that prevents the project from reaching a release-ready state until the architectural reasoning is re-documented against the fixed implementation. This reinforces the project's commitment to pedagogical transparency and ensures that every line of code is justified by a corresponding, up-to-date documentation block, maintaining the high P3-Archival standards required for long-term maintenance.
/// [WHERE] Executed as the final post-audit mutation stage within the primary `audit_code` entry point. It runs after all individual rule auditors have finalized their violation lists, allowing for a comprehensive assessment of the geode's structural health before any mutation is committed to the underlying source file.
fn trigger_memory_decay(source: &str, functions: &[parser::ScannedFunction<'_>], violations: &[AuditViolation]) -> Option<String> {
    let severe_rules = [
        "rule_else_free", "rule_nesting", "rule_poison_tokens", 
        "rule_no_raw_print", "rule_logical_density", "rule_excessive_coupling"
    ];

    let has_severe = violations.iter().any(|v| severe_rules.contains(&v.rule_id));
    if !has_severe { return None; }

    let mut lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();
    let mut modified = false;

    for func in functions {
        // Check if THIS function has severe violations
        let func_has_severe = violations.iter().any(|v| {
            v.line >= func.start_line && v.line <= func.end_line && severe_rules.contains(&v.rule_id)
        });

        if func_has_severe {
            modified |= wipe_function_tags(&mut lines, func.start_line);
        }
    }

    if !modified { return None; }
    Some(lines.join("\n") + "\n")
}





/// [WHAT] Realiza o 'wiping' físico das tags de documentação pedagógica acima de uma assinatura de função.
/// [WHY] Para garantir que código com violações graves não mantenha justificativas obsoletas ou enganosas.
/// [WHERE] Chamado pelo trigger_memory_decay durante a fase de mutação pós-auditoria.
fn wipe_function_tags(lines: &mut Vec<String>, start_line: usize) -> bool {
    let mut modified = false;
    let mut walk = start_line.saturating_sub(1);

    while walk > 0 {
        walk -= 1;
        let line = match lines.get(walk) { Some(l) => l, None => break };
        let trimmed = line.trim_start();

        if is_not_preamble(trimmed) { break; }
        
        if trimmed.starts_with("///") && is_governed_tag(trimmed) {
            let indent_len = line.len() - trimmed.len();
            lines[walk] = " ".repeat(indent_len);
            modified = true;
        }
    }
    modified
}

/// [WHAT] Identifica se uma linha de documentação contém uma das tags governadas pela Aesthetic Core.
/// [WHY] Auxiliar para o processo de Memory Decay para evitar nesting excessivo no loop principal.
/// [WHERE] Utilitário interno de lib.rs.
fn is_governed_tag(trimmed: &str) -> bool {
    trimmed.contains("[WHAT]") || trimmed.contains("[WHY]") || 
    trimmed.contains("[WHERE]") || trimmed.contains("[USAGE]")
}

/// [WHAT] Verifica se uma linha não faz parte do preâmbulo (doc ou atributo) de uma função.
/// [WHY] Critério de parada para o algoritmo de 'walk up' do Memory Decay.
/// [WHERE] Utilitário interno de lib.rs.
fn is_not_preamble(trimmed: &str) -> bool {
    !trimmed.starts_with("///") && !trimmed.starts_with("#[") && !trimmed.is_empty()
}

/// [WHAT] Verifies if a path is eligible for auditing.

// Memory management for raw WASM ABI
/// [WHAT] Allocates a block of memory of the specified size and prevents the Rust runtime from automatically reclaiming it.
/// [WHY] To provide the WASM host with a stable buffer where it can write input data (source code, paths, config) before calling the audit logic.
/// [WHERE] Exported as a global FFI function for the WASM orchestrator.
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    unsafe {
        std::alloc::alloc(std::alloc::Layout::from_size_align_unchecked(size, 1))
    }
}

/// [WHAT] Reclaims a previously allocated memory block, allowing the Rust allocator to free the resources.
/// [WHY] To prevent memory leaks in long-running audit sessions where the host repeatedly passes large files to the geode.
/// [WHERE] Critical FFI utility for memory lifecycle management in the WASM host.
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    if ptr.is_null() { return; }
    unsafe {
        std::alloc::dealloc(ptr, std::alloc::Layout::from_size_align_unchecked(size, 1));
    }
}

/// [WHAT] The industrial FFI entry point for the geode's governance engine. It accepts raw pointers to input strings and returns a pointer to the generated SARIF report.
/// [WHY] To bridge the gap between the complex Rust types used internally and the simplified ABI required for WASM-Host interoperability. 
/// [WHERE] Primary execution target for the Aesthetic Core WASM orchestrator.
#[no_mangle]
pub extern "C" fn audit_code_raw(
    src_ptr: *mut u8, src_len: usize,
    path_ptr: *mut u8, path_len: usize,
    cfg_ptr: *mut u8, cfg_len: usize
) -> *mut u8 {
    let source = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(src_ptr, src_len)) };
    let path = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(path_ptr, path_len)) };
    let config = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(cfg_ptr, cfg_len)) };

    let mut result_json = audit_code(source, path, config);
    result_json.push('\0'); // Null-terminate for the C-style ABI expected by the host.
    
    // Convert to boxed slice to ensure pointer stability
    let mut boxed_slice = result_json.into_bytes().into_boxed_slice();
    let result_ptr = boxed_slice.as_mut_ptr();
    
    std::mem::forget(boxed_slice);
    result_ptr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elite_reporter_visual() {
        reporter::print_elite_report("governance_results.sarif", 3, 2, 1, 0);
    }
}

