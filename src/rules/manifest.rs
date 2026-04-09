use crate::config::AuditConfig;

#[derive(Default, Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub range: (usize, usize),
    pub narrative: String,
    pub complexity: usize,
}

/// [WHAT] Generates a semantic manifest of code symbols and their narratives.
/// [WHY] To provide AI agents with architectural context and pedological insights.
/// [WHERE] Called during the AI semantic audit phase or as a standalone analysis tool.
pub fn generate(source: &str, _config: &AuditConfig) -> Vec<SymbolInfo> {
    let mut manifest = Vec::new();
    let mut current_symbol: Option<SymbolInfo> = None;
    let mut brace_depth = 0;
    let mut start_line_idx = 0;

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        
        // Detection logic
        if current_symbol.is_none() {
            current_symbol = start_symbol(idx, trimmed, source);
            if current_symbol.is_some() {
                start_line_idx = idx;
                brace_depth = 0;
            }
            continue;
        }

        // Tracking and Finalization logic
        if let Some(ref mut sym) = current_symbol {
            brace_depth += trimmed.matches('{').count() as i32;
            brace_depth -= trimmed.matches('}').count() as i32;

            if brace_depth == 0 && (trimmed.contains('}') || (sym.kind == "struct" && trimmed.contains(';'))) {
                finalize_symbol(sym, source, start_line_idx, idx, &mut manifest);
                current_symbol = None;
            }
        }
    }
    manifest
}

/// [WHAT] Helper to start a new symbol detection and initialization.
/// [WHY] To keep the main 'generate' loop nesting depth at 3.
/// [WHERE] Internal utility for semantic manifest generation.
fn start_symbol(idx: usize, trimmed: &str, source: &str) -> Option<SymbolInfo> {
    let (kind, name) = detect_symbol(trimmed);
    if name.is_empty() { return None; }

    Some(SymbolInfo {
        name,
        kind: kind.to_string(),
        range: (idx + 1, 0),
        narrative: extract_narrative(source, idx),
        complexity: 0,
    })
}

/// [WHAT] Finalizes a symbol entry and populates its metrics.
/// [WHY] To maintain cognitive clarity and satisfy structural governance.
/// [WHERE] Internal utility for semantic manifest generation.
fn finalize_symbol(sym: &mut SymbolInfo, source: &str, start: usize, end: usize, manifest: &mut Vec<SymbolInfo>) {
    sym.range.1 = end + 1;
    let block = source.lines().skip(start).take(end - start + 1).collect::<Vec<_>>().join("\n");
    sym.complexity = crate::rules::metrics::count_clean_lines(&block);
    manifest.push(sym.clone());
}

/// [WHAT] Helper to detect symbol kind and name from a code line.
/// [WHY] To keep the logic flat and follow the else-free mandate.
/// [WHERE] Core parser utility for the semantic engine.
fn detect_symbol(trimmed: &str) -> (&str, String) {
    if trimmed.starts_with("fn ") || trimmed.contains("pub fn ") {
        return ("function", extract_name(trimmed, "fn "));
    }
    if trimmed.starts_with("struct ") || trimmed.contains("pub struct ") {
        return ("struct", extract_name(trimmed, "struct "));
    }
    if trimmed.starts_with("macro_rules!") {
        return ("macro", extract_name(trimmed, "macro_rules!"));
    }
    ("", String::new())
}

/// [WHAT] Safely extracts a symbol name from a code line using pattern matching.
/// [WHY] To keep the logic robust and free of prohibited tokens like unwrap.
/// [WHERE] Core parser utility for the semantic engine.
fn extract_name(line: &str, trigger: &str) -> String {
    let part = match line.split(trigger).nth(1) {
        Some(p) => p,
        None => return String::new(),
    };
    
    let name = match part.split(|c: char| !c.is_alphanumeric() && c != '_').next() {
        Some(n) => n,
        None => return String::new(),
    };
    
    name.to_string()
}

/// [WHAT] Extracts a pedagogical narrative (doc comments) preceding a symbol.
/// [WHY] To associate symbols with their architectural explanations for AI agents.
/// [WHERE] Semantic metadata extraction layer.
fn extract_narrative(source: &str, start_idx: usize) -> String {
    let mut narrative = Vec::new();
    let search_start = compute_search_start(start_idx);

    for line in source.lines().take(start_idx).skip(search_start) {
        let trimmed = line.trim();
        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            narrative.push(trimmed.to_string());
        }
    }
    narrative.join("\n")
}

/// [WHAT] Helper to compute the search window start for narrative extraction.
/// [WHY] To avoid else blocks and maintain linear logic flow.
/// [WHERE] Internal utility for semantic manifest generation.
fn compute_search_start(idx: usize) -> usize {
    if idx > 10 { return idx - 10; }
    0
}
