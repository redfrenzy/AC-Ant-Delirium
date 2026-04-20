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

/// [WHAT] DADOS de conformidade e símbolos de um arquivo individual dentro de um manifesto âncora.
#[derive(Default, Debug, Clone)]
pub struct FileMetadata {
    pub dna: String,
    pub status: String,
    pub ai_info: String,
    pub symbols: Vec<SymbolInfo>,
}

/// [WHAT] O Manifesto Industrial Geográfico (Shield Sidecar) que consolida a alma técnica e a conformidade de um geode.
#[derive(Default, Debug, Clone)]
pub struct SidecarManifest {
    pub anchor_dna: String,
    pub version: String,
    pub files: std::collections::HashMap<String, FileMetadata>,
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

/// [WHAT] Extrator cirúrgico de identificadores de símbolos a partir de linhas de declaração brutas que processa nomes técnicos sem depender de motores de parsing externos pesados.
/// [WHY] Para fornecer chaves de identificação limpas e precisas para as entradas do manifesto semântico, garantindo que cada nome técnico seja auditável e único dentro do contexto do geode.
/// [WHERE] Camada de normalização lexical e processamento de identificadores em manifest.rs, servindo como o estágio final para nomes de funções e structs extraídos da fonte industrial.
/// [USAGE] Invocada pelo classificador de símbolos (detect_symbol) para isolar o nome técnico do identificador logo após a detecção do gatilho léxico, utilizando delimitadores alfanuméricos padrão.
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

impl SidecarManifest {
    /// [WHAT] Transforma o manifesto em uma string JSON industrial pura sem depender de crates externas de serialização pesada.
    /// [WHY] Mantém o motor zero-dependency e garante performance máxima durante a escrita de milhares de sidecars em geodes massasivos.
    /// [WHERE] Motor de serialização em manifest.rs, atuando como o exportador final de metadados agregados para o geode.
    /// [USAGE] Invocado pelo persist_shield_manifest no final do pipeline de auditoria para materializar a alma técnica do componente no disco host.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push('{');
        out.push_str("\n  \"anchor_dna\": \""); out.push_str(&self.anchor_dna); out.push_str("\",");
        out.push_str("\n  \"version\": \""); out.push_str(&self.version); out.push_str("\",");
        out.push_str("\n  \"files\": {");
        
        let mut first_file = true;
        for (path, meta) in &self.files {
            if !first_file { out.push(','); }
            first_file = false;
            out.push_str("\n    \""); out.push_str(path); out.push_str("\": {");
            out.push_str("\n      \"dna\": \""); out.push_str(&meta.dna); out.push_str("\",");
            out.push_str("\n      \"status\": \""); out.push_str(&meta.status); out.push_str("\",");
            out.push_str("\n      \"ai_info\": \""); out.push_str(&meta.ai_info); out.push_str("\",");
            out.push_str("\n      \"symbols\": [");
            
            for (i, sym) in meta.symbols.iter().enumerate() {
                if i > 0 { out.push(','); }
                out.push_str("\n        {");
                out.push_str("\n          \"name\": \""); out.push_str(&sym.name); out.push_str("\",");
                out.push_str("\n          \"kind\": \""); out.push_str(&sym.kind); out.push_str("\",");
                out.push_str("\n          \"complexity\": "); out.push_str(&sym.complexity.to_string());
                out.push_str("\n        }");
            }
            out.push_str("\n      ]");
            out.push_str("\n    }");
        }
        
        out.push_str("\n  }");
        out.push_str("\n}");
        out
    }

    /// [WHAT] Tenta ler um manifesto existente e reconstruir a estrutura mínima para merge industrial sem usar Serde.
    /// [WHY] Permite que o motor "aprenda" e agregue símbolos de múltiplos arquivos sequencialmente sem perder o estado de conformidade anterior.
    /// [WHERE] Motor de ingestão de manifestos em manifest.rs servindo de bridge de leitura para o merge de âncoras.
    /// [USAGE] Acionado no início do persist_shield_manifest para carregar o estado atual da âncora antes de injetar os novos símbolos detectados.
    pub fn from_json_brute(json: &str) -> Self {
        let mut manifest = SidecarManifest::default();
        manifest.version = "1.0.0".to_string();
        
        // 1. Extração de Anchor DNA
        if let Some(pos) = json.find("\"anchor_dna\": \"") {
            let start = pos + 15;
            if let Some(len) = json[start..].find("\"") {
                manifest.anchor_dna = json[start..start+len].to_string();
            }
        }

        // 2. Extração de Arquivos (Algoritmo de Fatiamento Industrial)
        let mut search_pos = match json.find("\"files\": {") {
            Some(p) => p + 10,
            None => return manifest,
        };

        while let Some(file_start) = json[search_pos..].find("\n    \"") {
            let name_start = search_pos + file_start + 6;
            let name_end = match json[name_start..].find("\": {") {
                Some(e) => e,
                None => break,
            };
            let filename = json[name_start..name_start + name_end].replace("\"", "");
            
            let mut meta = FileMetadata::default();
            let block_start = name_start + name_end + 4;
            
            // Extração de Metadados do Arquivo (DNA/Status)
            if let Some(dna_pos) = json[block_start..].find("\"dna\": \"") {
                let d_start = block_start + dna_pos + 8;
                if let Some(d_len) = json[d_start..].find("\"") {
                    meta.dna = json[d_start..d_start+d_len].to_string();
                }
            }
            if let Some(status_pos) = json[block_start..].find("\"status\": \"") {
                let s_start = block_start + status_pos + 11;
                if let Some(s_len) = json[s_start..].find("\"") {
                    meta.status = json[s_start..s_start+s_len].to_string();
                }
            }
            if let Some(ai_pos) = json[block_start..].find("\"ai_info\": \"") {
                let a_start = block_start + ai_pos + 12;
                if let Some(a_len) = json[a_start..].find("\"") {
                    meta.ai_info = json[a_start..a_start+a_len].to_string();
                }
            }

            // Extração de Símbolos do Arquivo
            if let Some(sym_pos) = json[block_start..].find("\"symbols\": [") {
                let mut sym_search = block_start + sym_pos + 12;
                while let Some(s_open) = json[sym_search..].find("{") {
                    let s_start = sym_search + s_open;
                    let mut sym = SymbolInfo::default();
                    if let Some(n_pos) = json[s_start..].find("\"name\": \"") {
                        let n_start = s_start + n_pos + 9;
                        if let Some(n_len) = json[n_start..].find("\"") {
                            sym.name = json[n_start..n_start+n_len].to_string();
                        }
                    }
                    if let Some(k_pos) = json[s_start..].find("\"kind\": \"") {
                        let k_start = s_start + k_pos + 9;
                        if let Some(k_len) = json[k_start..].find("\"") {
                            sym.kind = json[k_start..k_start+k_len].to_string();
                        }
                    }
                    if let Some(c_pos) = json[s_start..].find("\"complexity\": ") {
                        let c_start = s_start + c_pos + 14;
                        let mut c_val = 0;
                        for c in json[c_start..].chars() {
                            if let Some(digit) = c.to_digit(10) {
                                c_val = c_val * 10 + digit as usize;
                            } else { break; } // [REASON] Conclui a leitura do dígito ao encontrar delimitador não numérico.
                        }
                        sym.complexity = c_val;
                    }
                    meta.symbols.push(sym);
                    
                    if let Some(s_close) = json[s_start..].find("}") {
                        sym_search = s_start + s_close + 1;
                    } else { break; } // [REASON] Garante integridade: interrompe parsing se o bloco de símbolo estiver corrompido.
                    
                    if !json[sym_search..].trim_start().starts_with(',') { break; }
                }
            }

            manifest.files.insert(filename.to_string(), meta);
            
            // Avança para o fechamento do bloco do arquivo selecionado
            if let Some(next_block_end) = json[block_start..].find("\n    }") {
                search_pos = block_start + next_block_end + 6;
            } else { break; } // [REASON] Pivot de segurança léxica para evitar transbordamento de buffer em JSON malformado.
        }

        manifest
    }
}

/// [WHAT] Grava um snapshot intocável congelado vivo contrito profundo (SidecarManifest) único posicionado ao lado do alvo selecionado para a poderosa purificadora Migration.
/// [WHY] Fornece suporte estático estável ancorado e inquebrável atômico para agentes de Inteligência Artificial reestruturarem iterativamente as fendas estruturais cruas profundas e caóticas deste arquivo sem perder de vista as entidades e os métodos originais industriais vivos nativos, assegurando que o castelo não desabe (o Geode compile). 
/// [WHERE] Ferramenta independente de persistência secundária engastada no manifest.rs L0 puro profundo no Geode.
/// [USAGE] Roda acionada exclusiva pelo engine mestre purista no final do ciclo massivo do audit_geode_recursive SOMENTE SE o MIGRATION MODE atômico supremo for invocado no [hierarchy.target] pela IA governamental blindada viva no test.
pub fn persist_legacy_snapshot(file_path: &str, config: &AuditConfig) {
    let legacy_path = file_path.replace(".rs", "_legacy.json");
    if std::path::Path::new(&legacy_path).exists() { return; } // [SHIELD] Nunca sobrescreva um Legado. Guarde-o até que a IA termine!
    
    let source = match std::fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(_) => return,
    };
    
    let symbols = generate(&source, config);
    let mut manifest = SidecarManifest::default();
    manifest.anchor_dna = "migration".to_string();
    manifest.version = "1.0.0".to_string();
    
    let file_meta = FileMetadata {
        dna: "legacy".to_string(),
        status: "LEGACY".to_string(),
        ai_info: "AI: Preserve a assinatura estrita de todos os simbolos originais (structs/impl/funcs) listados neste manifesto para não corromper o compilador nativo do geode durante a refatoração fracionada desta imensa e massiva camada de caos residual industrial em Aesthetic Core.".to_string(),
        symbols,
    };
    
    let file_name_opt = std::path::Path::new(file_path).file_name();
    let relative_file = match file_name_opt {
        Some(os_str) => match os_str.to_str() { Some(s) => s, None => "unknown" },
        None => "unknown"
    };
    
    manifest.files.insert(relative_file.to_string(), file_meta);
    let _ = std::fs::write(&legacy_path, manifest.to_json());
}

