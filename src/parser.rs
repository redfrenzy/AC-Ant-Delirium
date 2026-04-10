/// [WHAT] A metadata-focused structure representing a scanned function, utilizing lifetimes to avoid redundant heap allocations of the source code during audits.
/// [WHY] To implement a Zero-Copy scanning architecture that maintains high performance and low memory footprint during real-time industrial code analysis.
/// [WHERE] Internal data type for CodeScanner results in parser.rs, acting as the primary carrier for function-level governance diagnostics.
/// [USAGE] Invocada pelo motor de rastreamento para encapsular o nome, limites de linha e metadados de documentação de cada unidade funcional descoberta.
#[derive(Default, Debug)]
pub struct ScannedFunction<'a> {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub docstring: String,
    pub body: &'a str,
    pub declaration: &'a str,
}

/// [WHAT] Legacy-inspired brute function scanner for Elite auditing that performs surgical extraction of code blocks without the overhead of an AST.
/// [WHY] To provide granular code analysis and structural tracking without external grammar crates, ensuring the zero-dependency goal of the Aesthetic Core.
/// [WHERE] Core utility for function-level rules (docs, structure, etc.) located in parser.rs for centralizing code discovery logic.
/// [USAGE] Utilizada como a espinha dorsal do loop de auditoria para identificar limites funcionais e capturar preâmbulos pedagógicos em todo o projeto.
pub struct CodeScanner;

impl CodeScanner {
    /// [WHAT] Scans source code for function boundaries and docstrings using a Zero-Copy approach to minimize memory fragmentation during massive geode audits.
    /// [WHY] To isolate functions for structural and semantic validation without duplicating the source in memory, maintaining elite performance standards for CI.
    /// [WHERE] Used by rules requiring per-function context in parser.rs, serving as the bridge between raw source and structured ScannedFunction artifacts.
    /// [USAGE] Invocada pelo orquestrador de auditoria global para preparar o conjunto de funções que serão processadas pelas regras de governança e métricas industriais na geode.
    pub fn scan_functions(source: &str) -> Vec<ScannedFunction<'_>> {
        let mut functions = Vec::new();
        let mut doc_accumulator = Vec::new();
        
        // We still collect lines for random-access lookup during brace matching
        let lines: Vec<&str> = source.lines().collect();

        for (idx, line) in source.lines().enumerate() {
            process_line_in_scan(idx, line, &mut doc_accumulator, &mut functions, &lines, source);
        }
        functions
    }
}

/// [WHAT] Internal line processor for the CodeScanner loop that manages docstring accumulation and function detection through a single-pass state machine.
/// [WHY] To keep nesting depth at level 3 or below and ensure that the auditing logic remains flat, linear, and compliant with industrial complexity limits.
/// [WHERE] Helper function for CodeScanner::scan_functions in parser.rs, providing localized state transitions for each discovered line of source code.
/// [USAGE] Chamada iterativamente pelo motor de rastreamento para cada linha, decidindo se deve arquivar docstrings ou disparar a criação de uma nova entrada funcional.
fn process_line_in_scan<'a, 'b>(
    idx: usize, 
    line: &'a str, 
    docs: &mut Vec<&'a str>, 
    functions: &mut Vec<ScannedFunction<'a>>, 
    lines: &'b [&'a str],
    source_full: &'a str
) {
    let trimmed = line.trim();
    if trimmed.is_empty() { return; }
    
    if trimmed.starts_with("///") {
        docs.push(trimmed);
        return;
    }

    let cleaned = clean_line(trimmed);
    if is_function_start(&cleaned) {
        functions.push(create_function_entry(idx, trimmed, docs, lines, source_full));
        return;
    }

    if !trimmed.starts_with("#[") {
        docs.clear();
    }
}

/// [WHAT] Strips string literals and comments from a source line for surgical analysis of functional tokens and structural terminators in the code.
/// [WHY] To prevent false positives where tokens like 'fn' or braces exist inside strings or comments, which would otherwise corrupt the audit results.
/// [WHERE] Core parser utility in parser.rs, used by both the CodeScanner and the ManifestGenerator for consistent and sanitized code processing across the engine.
/// [USAGE] Invocada para higienizar linhas de código antes da detecção de símbolos, garantindo que apenas a lógica operacional seja considerada pelo gatekeeper industrial.
pub fn clean_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_string = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' && !in_string { in_string = true; continue; }
        if c == '"' && in_string { in_string = false; continue; }
        if in_string { continue; }
        if c == '/' && chars.peek() == Some(&'/') { break; }
        out.push(c);
    }
    out
}

/// [WHAT] Detects whether a given line of code marks the beginning of a Rust function declaration by analyzing the presence of the 'fn' keyword and starting brace.
/// [WHY] To establish a vertical search boundary for function-level auditing, ensuring that only valid functional units are processed by the structural scanners.
/// [WHERE] Internal predicate used by the code scanner logic in parser.rs, acting as the primary filter for the functional discovery phase of the audit.
/// [USAGE] Utilizada pelo motor de rastreamento para identificar o início de uma unidade funcional industrial, permitindo a extração subsequente de metadados e corpo técnico.
fn is_function_start(line: &str) -> bool {
    (line.contains("fn ") || line.starts_with("pub fn ")) && line.contains('{')
}

/// [WHAT] Finalizes a ScannedFunction entry with line ranges and docstrings by calculating the vertical boundaries and capturing the associated pedagogical tags.
/// [WHY] To package function metadata into a validator-ready structure that contains all the necessary context for structural, lexical, and semantic governance rules.
/// [WHERE] Internal helper for CodeScanner in parser.rs, providing the construction logic for function metadata artifacts during the lexical scan phase.
/// [USAGE] Invoked when a function start is detected to calculate offsets and capture tags, ensuring that each functional unit is correctly represented in the internal model.
fn create_function_entry<'a, 'b>(
    idx: usize, 
    line: &'a str, 
    docs: &mut Vec<&'a str>, 
    source_lines: &'b [&'a str], 
    source_full: &'a str
) -> ScannedFunction<'a> {
    let name = extract_func_name(line);
    let docstring = docs.join("\n");
    docs.clear();

    let start_line = idx + 1;
    let end_line = find_closing_brace(idx, source_lines);
    
    // Extract body via Zero-Copy offsetting into the original source string
    let start_ptr = source_lines[idx].as_ptr();
    let end_ptr = source_lines[end_line - 1].as_ptr();
    let end_len = source_lines[end_line - 1].len();
    
    let offset_start = (start_ptr as usize).saturating_sub(source_full.as_ptr() as usize);
    let offset_end = (end_ptr as usize).saturating_sub(source_full.as_ptr() as usize) + end_len;
    
    if offset_end > source_full.len() || offset_start >= offset_end {
        return ScannedFunction {
            name,
            start_line,
            end_line,
            docstring,
            body: "",
            declaration: line,
        };
    }

    ScannedFunction {
        name,
        start_line,
        end_line,
        docstring,
        body: &source_full[offset_start..offset_end],
        declaration: line,
    }
}

/// [WHAT] Isolates the identifier from a function signature line by stripping the 'fn' prefix and surgical removal of generic or parameter markers.
/// [WHY] To provide a clean name for SARIF reporting and pedagogical manifest generation, ensuring that symbol identifiers are readable for both humans and AI.
/// [WHERE] Internal metadata extractor in parser.rs, serving as the final stage of functional name purification before the symbol is archived in the report.
/// [USAGE] Invocada durante a criação de entradas funcionais para extrair o nome técnico atômico a partir de assinaturas complexas contendo tipos genéricos ou parâmetros.
fn extract_func_name(line: &str) -> String {
    let mut parts = line.split("fn ");
    let _prefix = parts.next();
    let name_part = match parts.next() {
        Some(p) => match p.split('(').next() {
            Some(n) => n.trim(),
            None => "unknown",
        },
        None => "unknown",
    };
    name_part.to_string()
}

/// [WHAT] Scans vertically to find the physical line index of the terminating brace for a function block, respecting nested scopes and brace layering.
/// [WHY] To accurately calculate the line range and cognitive density of a function body, providing the structural boundaries for downstream auditing rules.
/// [WHERE] Core structural analyzer in parser.rs, providing vertical navigation capabilities to the function-level governance and metrics engine.
/// [USAGE] Invocada pelo motor de rastreamento para determinar o ponto final da massa funcional de um símbolo, permitindo a extração precisa do corpo e docstrings.
fn find_closing_brace(start_idx: usize, source: &[&str]) -> usize {
    let mut depth = 0;
    for (idx, line) in source.iter().enumerate().skip(start_idx) {
        depth += line.chars().filter(|&c| c == '{').count() as i32;
        depth -= line.chars().filter(|&c| c == '}').count() as i32;
        
        if depth == 0 && idx > start_idx {
            return idx + 1;
        }
    }
    start_idx + 1 // [Safety] Fallback
}
