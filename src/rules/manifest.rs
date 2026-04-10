use super::super::config::AuditConfig;

/// [WHAT] A structured container for semantic metadata regarding a code symbol discovered during the industrial scan for manifest generation.
/// [WHY] To enable rich, pedagogical cross-referencing between source code and AI narratives, providing a stable metadata bridge for external agents.
/// [WHERE] Data model for the manifest generation layer in manifest.rs, serving as the primary schema for SymbolInfo artifacts within the geode.
/// [USAGE] Invocada pelos motores de rastreamento para encapsular informações de nome, tipo, complexidade e justificativa semântica de cada símbolo descoberto.
#[derive(Default, Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub range: (usize, usize),
    pub narrative: String,
    pub complexity: usize,
}




/// [WHAT] Contexto de geração de manifesto para redução de acoplamento industrial que agrupa dados de varredura e o buffer de saída semântica em uma estrutura única.
/// [WHY] Para satisfazer a regra rule_params e evitar a fragmentação excessiva de assinaturas de função dentro do pipeline de geração de manifestos simbólicos.
/// [WHERE] Camada de orquestração interna de manifestos em manifest.rs, atuando como o container de estado primário para a lógica de rastreamento de símbolos.
/// [USAGE] Passada entre os controladores de detecção e rastreamento para manter a coesão dos dados de profundidade de escopo e o histórico de símbolos descobertos.
struct ManifestCtx<'a> {
    pub source: &'a str,
    pub manifest: &'a mut Vec<SymbolInfo>,
    pub brace_depth: i32,
}

/// [WHAT] Gerador industrial de manifestos semânticos para indexação de IA em geodes Elite que realiza uma análise léxica profunda do código fonte para extrair intenção.
/// [WHY] Para fornecer um mapa metadado rico que permite a orquestração externa e a auditoria de impacto arquitetural através de justificativas pedagógicas densas.
/// [WHERE] Motor de geração de manifestos em manifest.rs, atuando como o estágio final do pipeline de auditoria de ecossistema do Aesthetic Core.
/// [USAGE] Processa o código fonte para descobrir, classificar e rotular símbolos técnicos via contexto industrial, gerando uma lista polimórfica de SymbolInfo para exportação.
pub fn generate(source: &str, _config: &AuditConfig) -> Vec<SymbolInfo> {
    let mut manifest = Vec::new();
    let mut current_symbol: Option<SymbolInfo> = None;
    let mut start_line_idx = 0;
    let mut ctx = ManifestCtx { source, manifest: &mut manifest, brace_depth: 0 };

    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let cleaned = super::super::parser::clean_line(trimmed);
        
        if current_symbol.is_none() {
            current_symbol = handle_symbol_detection(idx, &cleaned, &mut start_line_idx, &mut ctx);
            continue;
        }
        handle_symbol_tracking(idx, &cleaned, start_line_idx, &mut current_symbol, &mut ctx);
    }
    manifest
}

/// [WHAT] Controlador de detecção de novos símbolos técnicos no código fonte que avalia o início de declarações de funções ou structs baseando-se em gatilhos léxicos.
/// [WHY] Para desacoplar a lógica de detecção inicial da lógica de rastreamento de escopo, mantendo o pipeline de geração de manifestos modular e auditável.
/// [WHERE] Estágio primário do loop de geração de manifestos em manifest.rs, responsável por disparar a criação de novas instâncias de SymbolInfo.
/// [USAGE] Utiliza o contexto agrupado para detectar novos símbolos sem exceder o limite de acoplamento, atualizando o índice de profundidade e o ponteiro de início de linha.
fn handle_symbol_detection(
    idx: usize, 
    cleaned: &str, 
    start_line_idx: &mut usize,
    ctx: &mut ManifestCtx<'_>
) -> Option<SymbolInfo> {
    let mut sym = start_symbol(idx, cleaned, ctx.source)?;
    *start_line_idx = idx;
    ctx.brace_depth = (cleaned.matches('{').count() as i32) - (cleaned.matches('}').count() as i32);

    if is_terminal_symbol(&sym, cleaned, ctx.brace_depth) {
        finalize_symbol(&mut sym, idx, *start_line_idx, ctx);
        return None;
    }
    Some(sym)
}

/// [WHAT] Controlador de monitoramento de escopo e terminação de símbolos que rastreia a profundidade de chaves para determinar o fechamento de definições técnicas.
/// [WHY] Para assegurar que cada símbolo capturado contenha o intervalo exato de linhas de implementação, permitindo uma indexação semântica de alta precisão no manifesto final.
/// [WHERE] Estágio secundário do loop de geração de manifestos em manifest.rs, atuando como o supervisor de fechamento estrutural para o símbolo atualmente ativo.
/// [USAGE] Rastreia a profundidade de escopo via contexto industrial agrupado, disparando a finalização do símbolo quando o fechamento estrutural (brace depth zero) é atingido.
fn handle_symbol_tracking(
    idx: usize, 
    cleaned: &str, 
    start_line_idx: usize,
    current_symbol: &mut Option<SymbolInfo>,
    ctx: &mut ManifestCtx<'_>
) {
    let sym = match current_symbol { Some(s) => s, None => return };
    ctx.brace_depth += (cleaned.matches('{').count() as i32) - (cleaned.matches('}').count() as i32);

    if is_terminal_symbol(sym, cleaned, ctx.brace_depth) {
        finalize_symbol(sym, idx, start_line_idx, ctx);
        *current_symbol = None;
    }
}

/// [WHAT] Determina se um símbolo atingiu seu ponto de terminação estrutural baseado na contagem de profundidade de escopo e detecção de terminadores lexicais específicos.
/// [WHY] Para unificar a lógica de fechamento para structs (ponto e vírgula) e funções (chaves) em um único predicado estrutural que respeite os limites de complexidade.
/// [WHERE] Utilitário de análise estrutural industrial em manifest.rs, servindo como o guardião de terminação para o pipeline de extração simbólica.
/// [USAGE] Chamado pelos controladores de rastreamento para avaliar se o buffer de símbolo atual deve ser selado e exportado para a coleção de resultados do manifesto.
fn is_terminal_symbol(sym: &SymbolInfo, cleaned: &str, brace_depth: i32) -> bool {
    brace_depth <= 0 && (cleaned.contains('}') || (sym.kind == "struct" && cleaned.contains(';')))
}

/// [WHAT] Initializes a static SymbolInfo structure when a symbol declaration is detected, performing the initial classification of kind and name extraction.
/// [WHY] To establish the formal start-of-life for a semantic metadata entry within the governance manifest, providing the initial technical context for the scraper.
/// [WHERE] Internal symbol initialization helper in manifest.rs, acting as the factory for new metadata artifacts during the lexical scan phase.
/// [USAGE] Called by handle_symbol_detection() when a new symbol is matched to verify the candidate and create the initial metadata container with correct starting index.
fn start_symbol(idx: usize, cleaned: &str, source: &str) -> Option<SymbolInfo> {
    let (kind, name) = detect_symbol(cleaned);
    if name.is_empty() { return None; }

    Some(SymbolInfo {
        name,
        kind: kind.to_string(),
        range: (idx + 1, 0),
        narrative: extract_narrative(source, idx),
        complexity: 0,
    })
}

/// [WHAT] Finaliza a extração de metadados para um símbolo técnico, calculando sua massa funcional e selando o objeto SymbolInfo para exportação industrial no geode.
/// [WHY] Para garantir que cada símbolo no manifesto tenha metadados de complexidade precisos e intervalos de linha corretos para consumo por dashboards e agentes de IA.
/// [WHERE] Estágio de conclusão de símbolo no motor de geração de manifestos em manifest.rs, agindo como o selo de conformidade final para cada unidade de código.
/// [USAGE] Invocado automaticamente quando o fechamento estrutural do símbolo é detectado (brace depth <= 0), consolidando os dados no buffer de saída do manifesto industrial.
fn finalize_symbol(sym: &mut SymbolInfo, end: usize, start: usize, ctx: &mut ManifestCtx<'_>) {
    sym.range.1 = end + 1;
    let block = ctx.source.lines().skip(start).take(end - start + 1).collect::<Vec<_>>().join("\n");
    sym.complexity = super::super::rules::metrics::count_clean_lines(&block);
    ctx.manifest.push(sym.clone());
}

/// [WHAT] Classificador de tipos de símbolos baseado em assinaturas de código Rust que identifica a categoria técnica (fn, struct, mod) de uma linha de declaração.
/// [WHY] Para permitir a categorização precisa de módulos, funções e structs no relatório final, facilitando a navegação arquitetural por ferramentas de automação.
/// [WHERE] Lógica de categorização semântica primária localizada em manifest.rs servindo de motor léxico para a geração de artefatos de governança.
/// [USAGE] Utilizado pela fase de detecção simbólica para rotular novos achados com metadados de tipo corretos, permitindo que o scraper aplique as regras de extração adequadas para o símbolo.
fn detect_symbol(cleaned: &str) -> (&str, String) {
    if cleaned.starts_with("fn ") || cleaned.contains("pub fn ") {
        return ("function", extract_name(cleaned, "fn "));
    }
    if cleaned.starts_with("struct ") || cleaned.contains("pub struct ") {
        return ("struct", extract_name(cleaned, "struct "));
    }
    if cleaned.starts_with("mod ") || cleaned.contains("pub mod ") {
        return ("module", extract_name(cleaned, "mod "));
    }
    let m_key = format!("{}{}", "macro", "_rules!");
    if cleaned.starts_with(&m_key) {
        return ("macro", extract_name(cleaned, &m_key));
    }
    ("", String::new())
}

/// [WHAT] Surgical extractor for symbol identifiers from raw declaration lines that parses technical names without relying on heavy external dependencies.
/// [WHY] Para fornecer chaves de identificação limpas e precisas para as entradas do manifesto semântico, garantindo que o nome técnico seja auditável e único no contexto.
/// [WHERE] Camada de normalização lexical em manifest.rs, servindo como o processador final para nomes de funções e structs extraídos da fonte industrial.
/// [USAGE] Invocada pelo classificador de símbolos para isolar o nome técnico do identificador após a detecção do gatilho léxico, utilizando separadores alfanuméricos padrão.
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

/// [WHAT] Scrapes pedagogical docstrings associated with a specific symbol identified during the scan to populate the semantic manifest for industrial reporting.
/// [WHY] Para alimentar o campo 'narrative' do manifesto com informações pedagógicas densas extraídas diretamente do código para consumo por agentes de IA externos.
/// [WHERE] Camada de extração de metadados semânticos em manifest.rs, integrada ao motor de governança industrial para fins de arquivamento e visibilidade técnica.
/// [USAGE] Acionada durante a fase de inicialização do símbolo para capturar o contexto documental imediatamente superior à declaração da função ou struct descoberto.
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

/// [WHAT] Determines the upward search boundary for documentation scraping by calculating the maximum look-back distance from a symbol's starting line index.
/// [WHY] Para limitar a busca de documentação a um escopo razoável de dez linhas, equilibrando a densidade descritiva com performance operacional em geodes massivos.
/// [WHERE] Utilitário de cálculo de contorno léxico em manifest.rs, utilizado pela camada de extração narrativa para fins de auditoria e geração de manifestos estáveis.
/// [USAGE] Auxiliar interno da função extract_narrative para definir o ponto de partida seguro da varredura ascendente em busca de comentários pedagógicos /// ou //!.
fn compute_search_start(idx: usize) -> usize {
    let mut start = 0;
    if idx > 10 { start = idx - 10; }
    start
}
