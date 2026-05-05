pub use crate::structs::CodeScanner;
use crate::structs::ScannedBlock;

#[path = "Parser_unitcore.rs"]
mod parser_unitcore;

impl CodeScanner {
    /// [WHAT] Scans source code for function boundaries and docstrings using a Zero-Copy approach to minimize memory fragmentation during massive geode audits.
    /// [WHY] To isolate functions for structural and semantic validation without duplicating the source in memory, maintaining elite performance standards for CI.
    /// [WHERE] Used by rules requiring per-function context in parser.rs, serving as the bridge between raw source and structured ScannedBlock artifacts.
    /// [USAGE] Invocado durante o processamento de resultados para anexar o conteúdo do trecho de código afetado. Este contexto visual é vital para que os desenvolvedores e agentes de IA compreendam instantaneamente a natureza da violação de governança, fornecendo a evidência textual necessária para a triagem e para a aplicação automática de sugestões de reparo (QuickFix) no ecossistema do geode. processadas pelas regras de governança e métricas industriais na geode. Este escaneador percorre todo o código fonte de forma linear, identificando assinaturas de funções e acumulando preâmbulos pedagógicos ([WHAT], [WHY], etc.) que precedem cada declaração, garantindo que a estrutura final capturada contenha todo o contexto necessário para a validação técnica profunda.
    pub fn scan_blocks(source: &str) -> Vec<ScannedBlock<'_>> {
        let mut blocks = Vec::new();
        let mut doc_accumulator = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        let module_doc = parser_unitcore::collect_module_doc(source);
        if !module_doc.is_empty() {
            blocks.push(parser_unitcore::create_module_block(&module_doc));
        }

        for (idx, line) in source.lines().enumerate() {
            parser_unitcore::process_line_in_scan(
                idx,
                line,
                &mut doc_accumulator,
                &mut blocks,
                &lines,
                source,
            );
        }
        blocks
    }
}

/// [WHAT] Strips string literals and comments from a source line for surgical analysis of functional tokens and structural terminators in the code.
/// [WHY] To prevent false positives where tokens like 'fn' or braces exist inside strings or comments, which would otherwise corrupt the audit results.
/// [WHERE] Core parser utility in parser.rs, used by both the CodeScanner and the ManifestGenerator for consistent and sanitized code processing across the engine.
/// [USAGE] Invocada para higienizar linhas de código antes da detecção de símbolos, garantindo que apenas a lógica operacional seja considerada pelo gatekeeper industrial. Este pré-processador é fundamental para evitar falsos positivos causados por palavras-chave ou braces contidos em strings de log ou comentários pedagógicos, assegurando que a auditoria estrutural e funcional seja aplicada apenas sobre o código operante real da geode.
pub fn clean_line(line: &str) -> String {
    parser_unitcore::clean_line(line)
}
