pub mod config;
pub mod governance;
pub mod rules;
pub mod rules_logs;
pub mod parser;
pub mod reporter;
pub mod sarif;
pub mod scanner;

use self::config::AuditConfig;
use self::scanner::RecursiveScanner;

/// [WHAT] A metadata-rich structure representing a specific governance violation.
/// [WHY] To provide standardized diagnostic information across all reporting layers (CLI, SARIF, WASM).
/// [WHERE] Core data type used throughout the entire audit pipeline.
#[derive(Default, Debug, Clone)]
pub struct AuditViolation {
    pub file_path: String,
    pub line: usize,
    pub rule_id: String,
    pub message: String,
    pub snippet: Option<String>,
    pub quick_fix: Option<String>,
}

/// [WHAT] The aggregated result of a complete file audit, including violations and execution logs.
/// [WHY] To decouple violation detection from reporting and allow for multiple output formats.
/// [WHERE] Primary return type for all audit entry points.
/// [WHAT] A structured container for summarizing the compliance state of a geode after a complete governance audit has been performed by the engine.
/// [WHY] To avoid rule_params violations and provide a clean, grouped interface for industrial reporting by aggregating violation counts and SARIF paths into a single cohesive entity.
/// [WHERE] Data model for the reporting layer in lib.rs, serving as the primary source of truth for dashboarding and automated CI/CD compliance gatekeepers.
/// [USAGE] Passed to print_elite_report to provide a unified metadata set for the reporter, ensuring that all priority levels (P0-P3) are correctly counted and displayed to the human operator.
#[derive(Default, Debug, Clone)]
#[allow(dead_code)]
pub struct EliteReportMetadata {
    pub sarif_path: String,
    pub p0: usize,
    pub p1: usize,
    pub p2: usize,
    pub p3: usize,
}

#[derive(Default, Debug, Clone)]
pub struct AuditResult {
    pub violations: Vec<AuditViolation>,
    pub logs: Vec<String>,
}

/// [WHAT] Aggregates the core parameters required for a specific audit phase, including source code, file path, and the current configuration manifest.
/// [WHY] To satisfy the rule_params governance gate by grouping related data into a single, cohesive structure that can be passed through multiple validation layers without increasing signature complexity.
/// [WHERE] Internal data transfer object defined in lib.rs for pipeline orchestration, acting as the primary state container for the active auditing session.
/// [USAGE] Passed between the primary orchestration stages (structural, pedagogical, ecosystem) to maintain low coupling and ensure that every rule has consistent access to the same audit context.
pub struct AuditContext<'a> {
    pub source: &'a str,
    pub file_path: &'a str,
    pub config: &'a AuditConfig,
    pub functions: &'a [parser::ScannedFunction<'a>],
    pub result: &'a mut AuditResult,
}

impl AuditResult {
    /// [WHAT] Orchestrates the transformation of audit findings into a SARIF 2.1.0 report via the dedicated sarif module for machine consumption.
    /// [WHY] To provide a standardized, machine-readable format for external CI/CD integration and industrial dashboarding, enabling seamless integration with IDEs and compliance-tracking platforms.
    /// [WHERE] Primary serialization bridge within the AuditResult struct in lib.rs, serving as the egress point for standardized diagnostic data in the Aesthetic Core.
    /// [USAGE] Invoked by the primary geode entry point or CLI tools to generate the final SARIF artifact, providing a high-fidelity JSON representation of all discovered governance violations.
    pub fn to_sarif(&self, file_path: &str, config: &AuditConfig) -> String {
        sarif::to_sarif(self, file_path, config)
    }
}

/// [WHAT] Maps an internal governance rule identifier to its corresponding SARIF severity level for standardized diagnostic reporting across industrial platforms.
/// [WHY] To provide a standardized and deterministic mapping between the geode's proprietary priority system (P0-P3) and the industry-standard SARIF severity levels, ensuring clear communication of risk.
/// [WHERE] Core categorization utility in lib.rs for SARIF serialization and downstream reporting, acting as the translation layer between the engine and external IDE diagnostics.
/// [USAGE] Called internally by the SARIF serialization engine to categorize violations for CI/CD dashboards, allowing automated tools to fail builds based on severity thresholds.
// A função map_rule_to_level foi movida para o módulo governance para centralizar a lógica de grão fino.

/// [WHAT] Sanitizes a string for inclusion in a JSON structure by escaping control characters and ensuring that the output complies with international serialization standards.
/// [WHY] To prevent malformed or malicious source code snippets from corrupting the generated SARIF report by ensuring full compliance with RFC 8259, maintaining high report fidelity.
/// [WHERE] Primary string sanitization utility used throughout the to_sarif serialization pipeline in lib.rs, providing a secure bridge between raw source and machine-readable JSON.
/// [USAGE] Invoked by the result serialization stage to safely encode diagnostic messages and signatures, preventing injection attacks or parsing errors in downstream IDE consumers.
fn json_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        if !apply_escape(c, &mut escaped) {
            escaped.push(c);
        }
    }
    escaped
}

/// [WHAT] Applies a JSON-specific escape sequence for a single character if required by RFC 8259, ensuring that non-printable characters are correctly represented.
/// [WHY] To maintain a shallow execution flow and comply with architectural nesting limits while ensuring full industrial coverage of the JSON specification for reliable report generation.
/// [WHERE] Helper character filter in lib.rs for the high-fidelity json_escape engine, acting as a low-level dispatcher for specific escape character codes discovered during the scan.
/// [USAGE] Called internally by json_escape() for each character encountered during the source code scan, facilitating the correct transformation of line breaks, tabs, and backslashes into JSON format.
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

/// [WHAT] Encodes a low-level ASCII control character into a standard JSON Unicode escape sequence (e.g., \u00XX) to maintain report integrity in modern editors.
/// [WHY] To ensure that control characters do not corrupt the generated SARIF JSON structure during serialization, preventing breakage in external parsing tools and maintaining Elite standards.
/// [WHERE] Specialist Unicode encoder located within the lib.rs orchestration layer, providing a robust fallback for non-standard or invisible character sequences discovered in source.
/// [USAGE] Called by apply_escape when a control character is detected during the string scanning process, appending the hexadecimal representation of the character to the output buffer.
fn push_unicode_escape(c: char, out: &mut String) {
    let code = c as u32;
    out.push_str("\\u00");
    let hex = b"0123456789abcdef";
    out.push(hex[(code >> 4) as usize] as char);
    out.push(hex[(code & 0xf) as usize] as char);
}
// =====================================================================
// PUBLIC API: audit_code
// This is the high-level entry point that returns a SARIF JSON string.
// It is used when the caller wants a machine-readable report (e.g. for
// CI/CD integration or VS Code diagnostics). For programmatic access
// to the violation list, use audit_code_to_result() instead.
// =====================================================================
/// [WHAT] The main audit entry point for WASM-based governance that consumes source code and returns a standardized SARIF report string for compliance monitoring.
/// [WHY] Orchestrates all governance rules in a single dependency-free call for AI compliance, ensuring that every project geode can be audited autonomously by the host system.
/// [WHERE] Primary bridge between the Host Orchestrator and the Geode in lib.rs, serving as the top-level API for the Aesthetic Core governance ecosystem.
/// [USAGE] Called by the WASM host with source code, file path, and configuration TOML strings to receive a complete diagnostic report in machine-readable SARIF format for IDE integration.
pub fn audit_code(source: &str, file_path: &str, config_toml: &str) -> String {
    let config = config::AuditConfig::parse_brute(config_toml);
    let result = audit_code_to_result(source, file_path, config_toml);
    result.to_sarif(file_path, &config)
}

/// [WHAT] Internal audit engine that returns a structured result for programmatic consumption by native tools.
/// [WHY] To allow the build script and other native governance auditors to process violations using standardized Rust data types.
/// [WHERE] Primary internal orchestration entry point within the lib.rs audit pipeline.
/// [USAGE] Called by audit_code() or external build scripts to receive detailed AuditResult artifacts.
/// [WHAT] Orchestrates a comprehensive recursive audit of a directory tree, aggregating all discovered governance violations into a single unified result set for global compliance reporting.
/// [WHY] To provide a centralized mechanism for auditing entire project geodes at once, ensuring that the complete architectural state is consistent with the established governance manifest in delirium.toml.
/// [WHERE] Core orchestration layer in lib.rs for recursive geode scanning, acting as the primary entry point for full-project compliance verification and industrial reporting.
/// [USAGE] Invocado pelo ponto de entrada principal do binário WASI para realizar uma varredura completa da árvore de arquivos do projeto, consolidando diagnósticos estruturais, pedagógicos e de ecossistema em um único artefato AuditResult.
pub fn audit_geode_recursive(dir_path: &str, config_toml: &str) -> AuditResult {
    let config = config::AuditConfig::parse_brute(config_toml);
    let mut total_result = AuditResult::default();

    RecursiveScanner::scan(std::path::Path::new(dir_path), &config, |path, source| {
        let result = audit_code_to_result(source, path.to_str().unwrap_or("unknown"), config_toml);
        total_result.violations.extend(result.violations);
    });

    total_result
}

/// [WHAT] Atomic code analyzer that generates compliance diagnostics for individual source files without the overhead of a full recursive tree scan.
/// [WHY] To enable granular verification of specific components and provide fast feedback during the development cycle, maintaining a high-velocity compliance pipeline.
/// [WHERE] Primary execution layer in lib.rs for file-level auditing, acting as the bridge between raw source and structured governance violations.
/// [USAGE] Invocado pelo orquestrador recursivo para cada arquivo descoberto ou diretamente por hosts de auditoria focal que necessitam de diagnósticos granulares de conformidade atômica.
pub fn audit_code_to_result(source: &str, file_path: &str, config_toml: &str) -> AuditResult {
    let config = config::AuditConfig::parse_brute(config_toml);
    let mut result = AuditResult::default();
    
    // Centralized Scanning (Audit 4 Optimization)
    let functions = parser::CodeScanner::scan_functions(source);
    let mut ctx = AuditContext {
        source, 
        file_path, 
        config: &config, 
        functions: &functions, 
        result: &mut result 
    };

    // Stage 1: Structure & Safety
    audit_structural(&mut ctx);

    // Stage 2: Documentation & Quality
    audit_pedagogical(&mut ctx);

    // Stage 3: Ecosystem & Health (MD pairing, deps, manifest)
    audit_ecosystem(&mut ctx);

    // [ELITE POLICY] Global Final Filter. We filter out any ignored violations 
    // BEFORE Memory Decay so that we don't accidentally mutate code for an ignored violation.
    result.violations.retain(|v| !governance::is_rule_ignored(&v.file_path, &v.rule_id, &config));

    // Stage 4: Memory Decay (Final Mutation)
    if let Some(mutated_source) = trigger_memory_decay(source, &functions, &result.violations) {
        // Isolation: Only mutate if the file is part of this project's source
        if !file_path.contains("ac_ant_delirium/src") && file_path.contains("/") {
            result.logs.push(format!("[LOG] Memory Decay BLOCKED for external file: {}. (Project Isolation Active)", file_path));
            return result;
        }

        let _ = std::fs::write(file_path, mutated_source);
        result.logs.push(format!("[LOG] Memory Decay triggered for local file {}.", file_path));
    }

    result
}

/// [WHAT] Orchestrates the structural and safety audit phase for a scanned source file.
/// [WHY] To isolate P0/P1 checks and maintain a linear, auditable execution pipeline within the provided context.
/// [WHERE] Phase 1 orchestration layer in lib.rs.
/// [USAGE] Called by audit_code_to_result() to perform high-priority structural and toxicity audits using the provided context wrapper for signature and line-level validation logic.
fn audit_structural(ctx: &mut AuditContext) {
    let mut s_ctx = rules::structure::StructureCtx {
        file_path: ctx.file_path,
        source: ctx.source,
        config: ctx.config,
        violations: &mut ctx.result.violations,
        current_nesting: 0,
    };
    rules::structure::verify(&mut s_ctx);

    if !governance::is_rule_ignored(ctx.file_path, "rule_poison_tokens", ctx.config) {
        rules::tokens::verify(ctx.file_path, ctx.source, ctx.config, &mut ctx.result.violations);
    }
    if !governance::is_rule_ignored(ctx.file_path, "rule_macro_isolation", ctx.config) {
        let mut m_ctx = rules::macros::MacroAuditCtx {
            file_path: ctx.file_path,
            source: ctx.source,
            config: ctx.config,
            violations: &mut ctx.result.violations,
        };
        rules::macros::verify(&mut m_ctx);
    }
}

/// [WHAT] Orchestrates the pedagogical and semantic audit phase for a scanned source file.
/// [WHY] To ensure that documentation quality and architectural density meet established Elite P2 standards.
/// [WHERE] Phase 2 orchestration layer in lib.rs.
/// [USAGE] Called by audit_code_to_result() to verify pedagogical documentation and function quality by scanning for mandatory tags and lexical density in function docstrings.
fn audit_pedagogical(ctx: &mut AuditContext) {
    if !governance::is_rule_ignored(ctx.file_path, "rule_doc_tags", ctx.config) {
        let mut d_ctx = rules::docs::DocsAuditCtx {
            file_path: ctx.file_path,
            functions: ctx.functions,
            config: ctx.config,
            violations: &mut ctx.result.violations,
        };
        rules::docs::verify(&mut d_ctx);
    }
    
    // [Elite Granularity] Quality rules have their own audit gate
    if !governance::is_rule_ignored(ctx.file_path, "rule_word_salad", ctx.config) {
        rules::quality::verify(ctx.file_path, ctx.functions, ctx.config, &mut ctx.result.violations);
    }
}

/// [WHAT] Orchestrates the ecosystem and health audit phase for a source file or manifest.
/// [WHY] To validate archival pairing, dependency purity, and generate persistent AI-ready manifests for governance.
/// [WHERE] Phase 3 orchestration layer in lib.rs.
/// [USAGE] Invocado pelo analisador de código atômico (audit_code_to_result) como a fase final do pipeline de governança para validar o emparelhamento de documentação industrial e a pureza das dependências externas do geode.
fn audit_ecosystem(ctx: &mut AuditContext) {
    // Metrics can trigger rule_params, rule_line_limit, rule_logical_density. 
    // We let it run unconditionally, and the Global Final Filter catches ignored specifics.
    let mut metrics_ctx = rules::metrics::MetricsCtx { 
        file_path: ctx.file_path,
        source: ctx.source, 
        functions: ctx.functions, 
        config: ctx.config, 
        violations: &mut ctx.result.violations 
    };
    rules::metrics::verify(&mut metrics_ctx);

    if !governance::is_rule_ignored(ctx.file_path, "rule_md_pairing", ctx.config) {
        let mut m_ctx = rules::markdown::MarkdownAuditCtx {
            file_path: ctx.file_path,
            config: ctx.config,
            violations: &mut ctx.result.violations,
        };
        rules::markdown::verify(&mut m_ctx);
    }

    // Deps rule can trigger "rule_deps" and "rule_layering".
    if ctx.file_path.ends_with("Cargo.toml") {
        let mut d_ctx = rules::deps::DepsAuditCtx {
            path: ctx.file_path,
            source: ctx.source,
            config: ctx.config,
            violations: &mut ctx.result.violations,
        };
        rules::deps::verify(&mut d_ctx);
    }
    if ctx.config.manifest.enabled {
        ctx.result.logs.push(format!("[LOG] AI Manifest generated for {}", ctx.file_path));
        let manifest = rules::manifest::generate(ctx.source, ctx.config);
        export_manifest(ctx.file_path, &manifest);
    }
}

/// [WHAT] Persists a discovered semantic manifest to a .manifest.json file on disk to provide persistent architectural context for external agents and auditing dashboards.
/// [WHY] To provide a stable metadata artifact that can be consumed by governance dashboards and external AI agents for deep architectural analysis and monitoring.
/// [WHERE] Final output and metadata export stage in lib.rs, serving as the bridge between in-memory analysis and persistent geode documentation.
/// [USAGE] Invocado pelo orquestrador de ecossistema após a geração bem-sucedida de um manifesto SymbolInfo para salvar os resultados em um formato de intercâmbio industrial estável.
fn export_manifest(file_path: &str, manifest: &[rules::manifest::SymbolInfo]) {
    let manifest_path = format!("{}_manifest.json", file_path.replace(".", "_"));
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

/// [WHAT] Audit scope validator for selective activation of components that determines if a file path should be processed based on the current inclusion manifest.
/// [WHY] To enable focused audits on specific sub-systems within massive geodes, significantly reducing CI execution time and focusing resources on active development areas.
/// [WHERE] Audit routing and scope management layer in lib.rs, acting as a high-level filter for the file scanner before rule execution begins.
/// [USAGE] Verifica se o caminho do arquivo está contido na lista de inclusão configurada no manifesto de governança para decidir se a auditoria completa deve ser disparada.
#[allow(dead_code)]
pub fn is_path_active(_path: &str, _config: &AuditConfig) -> bool {
    // [ELITE TOPOLOGY] A autoridade de exclusão global agora reside inteiramente no `scanner.rs`
    // via `config.architecture.global_excludes`. Todo arquivo que chega a este ponte já está autorizado.
    true
}

// A função is_rule_ignored foi movida para o módulo governance.



/// [WHAT] Implements the architectural 'Memory Decay' mechanism that purges pedagogical justifications from functions that have failed severe safety or structural checks.
/// [WHY] To prevent the accumulation of documentation staleness in Elite geodes where implementation integrity is paramount, ensuring that failed logic loses its justification.
/// [WHERE] Final corrective mutation stage within the primary audit_code_to_result process in lib.rs, providing a self-healing layer for the documentation tree.
/// [USAGE] Invoked as a post-audit cleanup step to ensure that only compliant code remains justified, acting as the primary enforcer of documentation-implementation synchronization.
fn trigger_memory_decay(source: &str, functions: &[parser::ScannedFunction<'_>], violations: &[AuditViolation]) -> Option<String> {
    let has_severe = violations.iter().any(|v| governance::is_severe(&v.rule_id));
    if !has_severe { return None; }

    let mut lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();
    let mut modified = false;

    for func in functions {
        // Check if THIS function has severe violations
        let func_has_severe = violations.iter().any(|v| {
            v.line >= func.start_line && v.line <= func.end_line && governance::is_severe(&v.rule_id)
        });

        if func_has_severe {
            modified |= wipe_function_tags(&mut lines, func.start_line);
        }
    }

    if !modified { return None; }
    Some(lines.join("\n") + "\n")
}





/// [WHAT] Performs a targeted documentation wipe for specific functions that failed severe safety or structural checks during the industrial governance scan.
/// [WHY] To enforce a 'semantic vacuum' that prevents stale justifications from persisting on non-compliant logic, forcing the developer to reconsider the architecture before redocumenting.
/// [WHERE] Corrective mutation layer in lib.rs, providing the low-level logic for precise documentation purging within the file's preamble blocks.
/// [USAGE] Called by trigger_memory_decay() to purge specific pedagogical tags (WHAT, WHY, etc.) from failed functional units by replacing them with sanitized whitespace.
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

/// [WHAT] Identifies if a documentation line contains one of the four mandatory pedagogical tags ([WHAT], [WHY], [WHERE], [USAGE]) that constitute the Elite documentation standard.
/// [WHY] To isolate the architectural justifications that require purging during Memory Decay mutation, ensuring that non-governed comments are preserved for context.
/// [WHERE] Semantic filtering utility in lib.rs for corrective mutations, acting as the primary discriminator for pedagogical content within documentation blocks.
/// [USAGE] Used by wipe_function_tags to identify specific documentation lines that must be targeted for deletion in cases of severe code-governance failure.
fn is_governed_tag(trimmed: &str) -> bool {
    trimmed.contains("[WHAT]") || trimmed.contains("[WHY]") || 
    trimmed.contains("[WHERE]") || trimmed.contains("[USAGE]")
}

/// [WHAT] Identifies documentation lines that sit outside of a function's pedagogical preamble by detecting non-comment markers and attribute decorators in the source.
/// [WHY] To provide a robust termination criterion for the Memory Decay walk-up algorithm, preventing the purger from accidentally deleting code or unrelated documentation.
/// [WHERE] Structural filtering utility in lib.rs for corrective mutations, serving as the guardrail for the documentation extraction and purging engine.
/// [USAGE] Used by wipe_function_tags to stop the reverse line scan at the boundaries of the documentation block, ensuring that only the relevant preamble is processed.
fn is_not_preamble(trimmed: &str) -> bool {
    !trimmed.starts_with("///") && !trimmed.starts_with("#[") && !trimmed.is_empty()
}


// =====================================================================
// FFI FUNCTIONS REMOVED
// The previous version had alloc(), dealloc(), and audit_code_raw()
// exported as #[no_mangle] extern "C" functions. These were designed
// for a cdylib model where an external host would:
//   1. Call alloc() to reserve memory in the WASM linear memory
//   2. Write source code bytes into that memory
//   3. Call audit_code_raw() with raw pointers
//   4. Call dealloc() to free the buffers
//
// This approach was WRONG because:
//   - It requires a complex host runtime (wasmer/wasmtime API) to
//     manage memory, call functions, and interpret results.
//   - The governance engine should be AUTONOMOUS — it reads files,
//     parses config, and reports violations by itself.
//
// The correct approach is to compile main.rs as a WASI binary:
//   cargo build --target wasm32-wasip1 --bin delirium_cli
//
// This produces a standalone program with _start that uses WASI
// syscalls (fd_read, path_open, args_get) to interact with the
// filesystem directly, just like a native binary would.
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// [WHAT] Verifies the visual formatting of the Elite reporting layer in a test environment.
    /// [WHY] To ensure that the industrial audit report remains readable and logically grouped for human operators.
    /// [WHERE] Localized unit test block within the lib.rs test module.
    /// [USAGE] Executado via cargo test para confirmar a sanidade da estrutura de metadados do relatório Elite e garantir que a formatação visual do resumo corporativo permaneça consistente com os padrões industriais do projeto.
    #[test]
    fn test_elite_reporter_visual() {
        let meta = EliteReportMetadata {
            sarif_path: "governance_results.sarif".to_string(),
            p0: 3, p1: 2, p2: 1, p3: 0,
        };
        reporter::print_elite_report(&meta);
    }
}
