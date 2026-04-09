/// [WHAT] A metadata-focused structure representing a scanned function, utilizing lifetimes to avoid redundant heap allocations of the source code.
/// [WHY] To implement a Zero-Copy scanning architecture that maintains high performance and low memory footprint during real-time audits.
/// [WHERE] Internal data type for CodeScanner results.
#[derive(Default, Debug)]
pub struct ScannedFunction<'a> {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub docstring: String,
    pub body: &'a str,
    pub declaration: &'a str,
}

/// [WHAT] Legacy-inspired brute function scanner for Elite auditing.
/// [WHY] To provide granular code analysis without external grammar/parsing crates.
/// [WHERE] Core utility for function-level rules (docs, structure, etc.).
pub struct CodeScanner;

impl CodeScanner {
    /// [WHAT] Scans source code for function boundaries and docstrings using a Zero-Copy approach.
    /// [WHY] To isolate functions for structural and semantic validation without duplicating the source in memory.
    /// [WHERE] Used by rules requiring per-function context.
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

/// [WHAT] Internal line processor for the CodeScanner loop.
/// [WHY] To keep nesting depth at level 3 or below.
/// [WHERE] Helper function for CodeScanner::scan_functions.
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

    if is_function_start(trimmed) {
        functions.push(create_function_entry(idx, trimmed, docs, lines, source_full));
        return;
    }

    if !trimmed.starts_with("#[") {
        docs.clear();
    }
}

/// [WHAT] Checks if a line represents a function start.
/// [WHY] Shallow detection for manual audit scaling.
/// [WHERE] Internal helper for CodeScanner.
fn is_function_start(line: &str) -> bool {
    (line.contains("fn ") || line.starts_with("pub fn ")) && line.contains('{')
}

/// [WHAT] Finalizes a ScannedFunction entry with line ranges and docstrings.
/// [WHY] To package function metadata into a validator-ready structure.
/// [WHERE] Internal helper for CodeScanner.
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

/// [WHAT] Extracts the function name from a declaration line.
/// [WHY] To identify functions in audit logs and SARIF results.
/// [WHERE] Internal string manipulation for CodeScanner.
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

/// [WHAT] Finds the matching closing brace for a function block.
/// [WHY] To determine the exact line range of a function for structural audits.
/// [WHERE] Brute-force vertical scanner for CodeScanner.
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
