use crate::restricted_tokens::resolve::elite_resolve_or;

use super::ScannedBlock;

pub(super) fn collect_module_doc(source: &str) -> Vec<&str> {
    let mut module_doc = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            module_doc.push(trimmed);
            continue;
        }
        if !trimmed.is_empty() && !trimmed.starts_with("#[") {
            break;
        }
    }
    module_doc
}

pub(super) fn create_module_block<'a>(module_doc: &[&'a str]) -> ScannedBlock<'a> {
    ScannedBlock {
        name: "geode_root".to_string(),
        start_line: 1,
        end_line: 1,
        docstring: module_doc.join("\n"),
        body: "",
        declaration: "",
        kind: "_module".to_string(),
    }
}

/// [WHAT] Internal line processor for the CodeScanner loop that manages docstring accumulation and function detection through a single-pass state machine.
/// [WHY] To keep nesting depth at level 3 or below and ensure that the auditing logic remains flat, linear, and compliant with industrial complexity limits.
/// [WHERE] Helper function for CodeScanner::scan_blocks in parser.rs, providing localized state transitions for each discovered line of source code.
/// [USAGE] Chamada iterativamente pelo motor de rastreamento para cada linha, decidindo se deve arquivar docstrings ou disparar a criação de uma nova entrada funcional. Esta função mantém a carga cognitiva sob controle industrial ao delegar o processamento pesado para workers especializados, assegurando que o loop de descoberta de símbolos permaneça plano e linear conforme as normas de governança Elite para sistemas de missão crítica no Aesthetic Core.
pub(super) fn process_line_in_scan<'a, 'b>(
    idx: usize,
    line: &'a str,
    docs: &mut Vec<&'a str>,
    blocks: &mut Vec<ScannedBlock<'a>>,
    lines: &'b [&'a str],
    source_full: &'a str,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        docs.clear();
        return;
    }

    if trimmed.starts_with("///") {
        docs.push(trimmed);
        return;
    }

    let cleaned = clean_line(trimmed);
    if is_block_start(&cleaned) {
        blocks.push(create_block_entry(idx, line, docs, lines, source_full));
        return;
    }

    if !trimmed.starts_with("#[") {
        docs.clear();
    }
}

/// [WHAT] Strips string literals and comments from a source line for surgical analysis of functional tokens and structural terminators in the code.
/// [WHY] To prevent false positives where tokens like 'fn' or braces exist inside strings or comments, which would otherwise corrupt the audit results.
/// [WHERE] Core parser utility in parser.rs, used by both the CodeScanner and the ManifestGenerator for consistent and sanitized code processing across the engine.
/// [USAGE] Invocada para higienizar linhas de código antes da detecção de símbolos, garantindo que apenas a lógica operacional seja considerada pelo gatekeeper industrial. Este pré-processador é fundamental para evitar falsos positivos causados por palavras-chave ou braces contidos em strings de log ou comentários pedagógicos, assegurando que a auditoria estrutural e funcional seja aplicada apenas sobre o código operante real da geode.
pub(super) fn clean_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_string = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' && !in_string {
            in_string = true;
            continue;
        }
        if c == '"' && in_string {
            in_string = false;
            continue;
        }
        if in_string {
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            break;
        }
        out.push(c);
    }
    out
}

/// [WHAT] Detects whether a given line of code marks the beginning of a Rust code block (function, struct, enum, trait, impl) by analyzing structural keywords.
/// [WHY] To establish a vertical search boundary for block-level auditing, ensuring that only valid functional and layout units are processed by the structural scanners.
/// [WHERE] Internal predicate used by the code scanner logic in parser.rs, acting as the primary filter for the discovery phase of the audit.
/// [USAGE] Utilizada pelo motor de rastreamento para identificar o início de uma unidade estrutural industrial, permitindo a extração subsequente de metadados e corpo técnico.
fn is_block_start(line: &str) -> bool {
    let has_terminator = line.contains('{') || line.contains(';');
    let has_kw = line.contains("fn ")
        || line.starts_with("pub fn ")
        || line.contains("struct ")
        || line.contains("enum ")
        || line.contains("trait ")
        || line.contains("macro_rules!")
        || is_impl_start(line);
    has_terminator && has_kw
}

/// [WHAT] Finalizes a ScannedBlock entry with line ranges and docstrings by calculating the vertical boundaries and capturing the associated pedagogical tags.
/// [WHY] To package function metadata into a validator-ready structure that contains all the necessary context for structural, lexical, and semantic governance rules.
/// [WHERE] Internal helper for CodeScanner in parser.rs, providing the construction logic for function metadata artifacts during the lexical scan phase.
/// [USAGE] Invocada quando o início de uma declaração funcional é detectado pelo escaneador de linha, acionando o cálculo de offsets de ponteiros e a captura de tags pedagógicas associadas. Este componente é vital para a arquitetura Zero-Copy, pois traduz os índices de linhas físicas em fatias (slices) de memória seguras do buffer original, permitindo que as regras de governança analisem o corpo da função sem duplicar strings no heap industrial.
fn create_block_entry<'a, 'b>(
    idx: usize,
    line: &'a str,
    docs: &mut Vec<&'a str>,
    source_lines: &'b [&'a str],
    source_full: &'a str,
) -> ScannedBlock<'a> {
    let name = extract_block_name(line);
    let (kind, _) = detect_block_type(line);
    let docstring = docs.join("\n");
    docs.clear();

    let start_line = idx + 1;
    let end_line = find_closing_brace(idx, source_lines);

    ScannedBlock {
        name,
        start_line,
        end_line,
        docstring,
        body: extract_body_slice(source_full, source_lines, start_line, end_line),
        declaration: line,
        kind: kind.to_string(),
    }
}

/// [WHAT] Safely extracts a zero-copy string slice from the full source based on line indices.
/// [WHY] To satisfy nesting limits by isolating pointer arithmetic and boundary checks for the geode.
/// [WHERE] Utilitário de baixo nível do motor de parsing em parser.rs, responsável pela aritmética de ponteiros e delimitação de escopos funcionais.
/// [USAGE] Chamado por create_function_entry para resolver as fronteiras físicas do corpo de uma função no buffer original do geode, garantindo que o slice resultante seja válido e não ultrapasse os limites de memória durante a auditoria. Esta implementação isola a aritmética de ponteiros e as validações de bound para manter o fluxo principal livre de ruído técnico, cumprindo os requisitos de clareza cognitiva e integridade industrial orquestrados pela governança.
fn extract_body_slice<'a>(
    source_full: &'a str,
    source_lines: &[&'a str],
    start: usize,
    end: usize,
) -> &'a str {
    let start_ptr = source_lines[start - 1].as_ptr();
    let end_ptr = source_lines[end - 1].as_ptr();
    let end_len = source_lines[end - 1].len();

    let offset_start = (start_ptr as usize).saturating_sub(source_full.as_ptr() as usize);
    let offset_end = (end_ptr as usize).saturating_sub(source_full.as_ptr() as usize) + end_len;

    if offset_end > source_full.len() || offset_start >= offset_end {
        return "";
    }
    &source_full[offset_start..offset_end]
}

/// [WHAT] Isola o identificador de uma assinatura estrutural através da remoção cirúrgica do prefixo de bloco (fn, struct, etc.) e marcações colaterais.
/// [WHY] Para fornecer um nome limpo e normalizado para o reporte SARIF e para a geração de manifestos pedagógicos.
/// [WHERE] Extrator de metadados interno em parser.rs.
/// [USAGE] Invocada durante a criação de entradas funcionais industriais para extrair o nome técnico atômico.
fn extract_block_name(line: &str) -> String {
    let (_, name) = detect_block_type(line);
    name
}

/// [WHAT] Detecta cirurgicamente o tipo e o nome de um bloco Rust.
fn detect_block_type(line: &str) -> (&'static str, String) {
    if is_impl_start(line) {
        return ("impl", extract_impl_name(line));
    }

    let keywords = [
        ("fn ", "function"),
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("macro_rules!", "macro"),
    ];

    for (kw, kind) in &keywords {
        if line.contains(kw) {
            let mut parts = line.split(kw);
            let _ = parts.next();
            if let Some(part) = parts.next() {
                let name = elite_resolve_or(
                    part.split(['(', '{', '<']).next(),
                    "unknown",
                );
                return (kind, name.trim().to_string());
            }
        }
    }
    ("unknown", "unknown".to_string())
}

fn is_impl_start(line: &str) -> bool {
    let normalized = line.trim_start();
    normalized.starts_with("impl ")
        || normalized.starts_with("impl<")
        || normalized.starts_with("unsafe impl ")
        || normalized.starts_with("unsafe impl<")
}

fn extract_impl_name(line: &str) -> String {
    let normalized = line.trim();

    if let Some((_, rhs)) = normalized.split_once(" for ") {
        return rhs
            .split(['{', '<', ' ', '\t'])
            .find(|segment| !segment.trim().is_empty())
            .unwrap_or("unknown")
            .trim()
            .to_string();
    }

    let after_impl = normalized
        .strip_prefix("unsafe ")
        .unwrap_or(normalized)
        .trim_start_matches("impl")
        .trim_start();

    after_impl
        .split(['{', '<', ' ', '\t'])
        .find(|segment| !segment.trim().is_empty())
        .unwrap_or("unknown")
        .trim()
        .to_string()
}

/// [WHAT] Scans vertically to find the physical line index of the terminating brace for a function block, respecting nested scopes and brace layering.
/// [WHY] To accurately calculate the line range and cognitive density of a function body, providing the structural boundaries for downstream auditing rules.
/// [WHERE] Core structural analyzer in parser.rs, providing vertical navigation capabilities to the function-level governance and metrics engine.
/// [USAGE] Invocada pelo motor de rastreamento para determinar o ponto final da massa funcional de um símbolo, permitindo a extração precisa do corpo e docstrings. Ao percorrer verticalmente o buffer e rastrear o equilíbrio de braces, esta função define as fronteiras físicas do geode que serão submetidas à análise de densidade lógica e métricas de massa de código, garantindo que o escopo da auditoria seja cirurgicamente exato.
fn find_closing_brace(start_idx: usize, source: &[&str]) -> usize {
    let mut depth = 0;
    for (idx, line) in source.iter().enumerate().skip(start_idx) {
        depth += count_braces_in_line(line, true);
        depth -= count_braces_in_line(line, false);

        if depth != 0 {
            continue;
        }
        if idx <= start_idx {
            continue;
        }
        return idx + 1;
    }
    start_idx + 1
}

/// [WHAT] Conta ocorrências de um brace específico em uma linha limpa.
/// [WHY] Evita o uso de closures complexas que poderiam confundir o auditor ou aumentar a profundidade cognitiva desnecessariamente.
/// [WHERE] Utilitário estrutural em parser.rs para análise de blocos.
/// [USAGE] Chamada por find_closing_brace para sincronizar a profundidade léxica e encontrar o fim de uma função, garantindo que o motor de auditoria identifique corretamente as fronteiras físicas de cada unidade funcional no geode.
fn count_braces_in_line(line: &str, opening: bool) -> i32 {
    let target = if opening { '\x7b' } else { '\x7d' };
    let mut count = 0;
    for c in line.chars() {
        if c == target {
            count += 1;
        }
    }
    count
}
