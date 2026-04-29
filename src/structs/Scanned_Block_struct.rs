/// [WHAT] A metadata-focused structure representing a scanned code block (function, struct, enum, trait), utilizing lifetimes to avoid redundant heap allocations of the source code during audits.
/// [WHY] To implement a Zero-Copy scanning architecture that maintains high performance and low memory footprint during real-time industrial code analysis.
/// [WHERE] Internal data type for parser scanning results, acting as the primary carrier for block-level governance diagnostics.
/// [USAGE] Invocada pelo motor de rastreamento para encapsular o nome, limites de linha e metadados de documentação de cada unidade arquitetural descoberta.
#[derive(Default, Debug)]
pub struct ScannedBlock<'a> {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub docstring: String,
    pub body: &'a str,
    pub declaration: &'a str,
    pub kind: String,
}
