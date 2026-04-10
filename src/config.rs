use std::collections::HashMap;
use crate::restricted_tokens::resolve::elite_resolve_or;

/// [WHAT] Configuration for pedagogical documentation tag length and helpfulness standards within the Elite documentation engine.
/// [WHY] To ensure that architectural justifications ([WHY]) provide sufficient context for AI agents and maintain the industrial rigor of the project.
/// [WHERE] Sub-component of the RulesConfig in config.rs, serving as the data model for tag-specific governance constraints in the geode.
#[derive(Default, Debug)]
pub struct TagConfig {
    pub min_chars: usize,
    pub hint: String,
}

/// [WHAT] Configuration for macro complexity and structural constraints to prevent the abuse of metaprogramming in business logic units.
/// [WHY] To prevent the accumulation of un-auditable logic within macros by enforcing strict industrial size limits and maintaining geode audidability.
/// [WHERE] Sub-component of the RulesConfig in config.rs, providing the switches for the macro isolation audit engine.
#[derive(Default, Debug)]
pub struct MacroConfig {
    pub max_lines: usize,
    pub allow_complex_logic: bool,
    pub tip: String,
}

/// [WHAT] Global structural limits for code clean lines, nesting, and complexity used to enforce architectural purity across the entire source tree.
/// [WHY] To enforce a consistently shallow and readable architecture across all geodes, preventing the formation of 'Cognitive Abysses' in the logic.
/// [WHERE] Sub-component of the RulesConfig in config.rs, acting as the primary repository for structural governance thresholds.
#[derive(Default, Debug)]
pub struct LimitsConfig {
    pub max_lines: usize,
    pub tip_lines: String,
    pub max_nesting: usize,
    pub max_params: usize,
    pub logical_density: usize,
}

/// [WHAT] The primary aggregator for all individual governance rule configurations, coordinating limits, tags, and restricted tokens.
/// [WHY] To provide a centralized structure for rule-based audit logic, enabling a cohesive policy enforcement mechanism across the Aesthetic Core ecosystem.
/// [WHERE] Part of the top-level AuditConfig in config.rs, serving as the core repository for active compliance rules.
#[derive(Default, Debug)]
pub struct RulesConfig {
    pub limits: LimitsConfig,
    pub doc_tags: HashMap<String, TagConfig>,
    pub restricted_tokens: HashMap<String, String>,
    pub macros: MacroConfig,
    pub allowed_crates: Vec<String>,
}

/// [WHAT] Configuration for authorized tracing zones and log pollution control to manage debugging visibility in Elite geodes.
/// [WHY] To allow localized debugging in specific project modules while preventinghibited tokens like 'println!' from leaking into the industrial production binary.
/// [WHERE] Part of the top-level AuditConfig in config.rs, providing the security policy for tracing and diagnostic output.
#[derive(Default, Debug)]
pub struct DebugConfig {
    pub enabled: bool,
    pub allow_list: Vec<String>,
}

/// [WHAT] Configuration for the generation of AI-ready semantic metadata manifests used for architectural discovery and industrial reporting.
/// [WHY] To provide deep architectural context for external agents and compliance tools, facilitating the integration of the geode into larger automated ecosystems.
/// [WHERE] Part of the top-level AuditConfig in config.rs, serving as the control plane for the manifest generation engine.
#[derive(Default, Debug, Clone)]
pub struct ManifestConfig {
    pub enabled: bool,
    pub features: Vec<String>,
}

/// [WHAT] Configuration for the Shadow Context (SARIF) exporter, defining the destination and format of the audit artifacts.
/// [WHY] To allow users to customize where the AI-ready metadata is saved, ensuring integration with specific project structures.
/// [WHERE] Part of the top-level AuditConfig in config.rs.
#[derive(Debug, Clone)]
pub struct ShadowConfig {
    pub output: String,
    pub format: String,
}

impl Default for ShadowConfig {
    /// [WHAT] Fornece a inicialização padrão para a configuração do exportador Shadow Context, definindo valores industrialmente seguros.
    /// [WHY] Para garantir que o motor de reporte SARIF tenha sempre um destino de saída válido e um formato determinístico, mesmo na ausência de diretrizes explícitas no manifesto.
    /// [WHERE] Camada de inicialização de dados em config.rs.
    /// [USAGE] Chamado automaticamente pelo AuditConfig::default() para garantir a prontidão do subsistema de exportação de conformidade durante o bootstrap do Geode.
    fn default() -> Self {
        Self {
            output: "delirium.shadow.json".to_string(),
            format: "json".to_string(),
        }
    }
}

/// [WHAT] Configuration for AI-facing contextual metadata injected into Shadow Context (SARIF) properties and terminal summaries.
/// [WHY] To provide strategic operational context that helps AI agents maintain awareness of project tools, priorities, and authorized commands across prompt boundaries.
/// [WHERE] Part of the top-level AuditConfig in config.rs, populated from the [info] section of delirium.toml.
#[derive(Default, Debug, Clone)]
pub struct InfoConfig {
    pub priority_order: String,
    pub context: Vec<String>,
    pub tools: Vec<String>,
    pub terminal_commands: Vec<String>,
}

#[derive(Default, Debug)]
pub struct ArchitectureConfig {
    pub global_excludes: Vec<String>,
}

impl ArchitectureConfig {
    /// [WHAT] Sincroniza as configurações de exclusão global do sistema de arquivos com o modelo de arquitetura do Geode.
    /// [WHY] Para garantir que o auditor de conformidade ignore zonas de código não-gerenciadas ou artefatos de build, otimizando o desempenho da varredura industrial.
    /// [WHERE] Componente tômico de configuração de arquitetura em config.rs.
    /// [USAGE] Invocado pelo despachante de configuração para popular a lista de caminhos ignorados globalmente, assegurando que o RecursiveScanner respeite as fronteiras de exclusão definidas no manifesto de governança.
    fn apply_kv(&mut self, key: &str, value: &str) {
        if key == "global_excludes" {
            self.global_excludes = value.trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
}

#[derive(Default, Debug)]
pub struct JurisdictionConfig {
    pub exclusive_to: HashMap<String, Vec<String>>,
    pub ignore_to: HashMap<String, Vec<String>>,
}

impl JurisdictionConfig {
    /// [WHAT] Processa a jurisdição de exclusividade para regras de governança, vinculando caminhos específicos a conjuntos de regras permitidas.
    /// [WHY] Para suportar o isolamento de componentes (como Safe Harbors ou testes) que requerem um subconjunto restrito e controlado de auditorias, garantindo jurisdição clara no Geode.
    /// [WHERE] Lógica de jurisdição no módulo de configuração em config.rs.
    /// [USAGE] Chamada pelo despachante principal para preencher o mapeamento exclusive_to, definindo áreas onde apenas regras selecionadas têm autoridade operacional.
    fn handle_exclusive_section(&mut self, key: &str, value: &str) {
        let rules: Vec<String> = value.trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.exclusive_to.insert(key.to_string(), rules);
    }

    /// [WHAT] Sincroniza a lista de regras ignoradas para caminhos específicos, permitindo silenciamentos justificados de governança.
    /// [WHY] Para oferecer flexibilidade operacional em casos onde conformidades estruturais ou semânticas são intencionalmente violadas por razões técnicas documentadas.
    /// [WHERE] Lógica de jurisdição no módulo de configuração em config.rs.
    /// [USAGE] Invocada pelo motor de parsing para atualizar o mapa ignore_to, garantindo que o auditor respeite as exceções de conformidade autorizadas no projeto.
    fn handle_ignore_section(&mut self, key: &str, value: &str) {
        let rules: Vec<String> = value.trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.ignore_to.insert(key.to_string(), rules);
    }
}

/// [WHAT] Top-level container for the complete governance policy of a geode, aggregating active zones, exceptions, and industrial limits.
/// [WHY] To encapsulate the entire project's architectural sanctity in a single, auditable structure that can be easily parsed and enforced by the engine.
/// [WHERE] Primary configuration data model for the ac_ant_delirium system in config.rs.
#[derive(Default, Debug)]
pub struct AuditConfig {
    pub architecture: ArchitectureConfig,
    pub jurisdiction: JurisdictionConfig,
    pub rules: RulesConfig,
    pub manifest: ManifestConfig,
    pub shadow: ShadowConfig,
    pub debug: DebugConfig,
    pub info: InfoConfig,
}

impl AuditConfig {
    /// [WHAT] Parses a TOML-like configuration string into a structured AuditConfig instance using a zero-dependency industrial-grade brute scanner.
    /// [WHY] To provide a lightweight and deterministic configuration engine that avoids heavy parser crates like 'serde' or 'toml', maintaining geode purity.
    /// [WHERE] Top-level initialization entry point within the configuration module in config.rs, acting as the primary gateway for policy loading.
    /// [USAGE] Invoked by the primary geode entry point to load governance policy from a manifest file, synchronously populating the internal configuration model for the audit.
    pub fn parse_brute(input: &str) -> Self {
        let mut config = AuditConfig::default();
        let mut section = String::new();

        for line in input.lines() {
            Self::parse_brute_line(&mut config, &mut section, line.trim());
        }
        config
    }

    /// [WHAT] Parses a single line from the configuration manifest and mutates the current section or config state.
    /// [WHY] Extracts loop body logic from parse_brute to satisfy Elite cognitive nesting limits (<= 3).
    /// [WHERE] Camada de execução atômica do motor de parsing bruto em config.rs, responsável pela transição de estado de seção e processamento de pares chave-valor individuais.
    /// [USAGE] Invocado pelo motor de inicialização do geode para processar cada linha do manifesto de governança de forma individual. Esta função de worker permite que o loop principal permaneça plano e linear, garantindo que o parser brute-force consiga extrair chaves e valores sem exceder os limites rigorosos de aninhamento cognitivo, mantendo a auditabilidade total sobre como cada propriedade técnica é despachada para o modelo de dados de conformidade.
    fn parse_brute_line(config: &mut AuditConfig, section: &mut String, line: &str) {
        if line.is_empty() || line.starts_with('#') { return; }

        if line.starts_with('[') && line.ends_with(']') {
            *section = line[1..line.len()-1].to_string();
            return;
        }

        if let Some((k, v)) = split_kv(line) {
            config.apply_kv(section, k.trim().trim_matches('"'), v.trim().trim_matches('"'));
        }
    }

    /// [WHAT] Dispatches a raw TOML key-value pair to its corresponding sub-config section based on the current context of the brute parser.
    /// [WHY] To maintain a shallow execution flow and prevent deeply nested match blocks from obscuring the configuration logic, satisfying Elite structural requirements for industrial geodes.
    /// [WHERE] Ponto central de despacho do motor de inicialização em config.rs, atuando como o roteador principal para a sincronização do modelo de dados.
    /// [USAGE] Atua como o ponto central de despacho para a sincronização de propriedades configuradas no delirium.toml, direcionando cada par de chave-valor para sua seção correspondente no modelo de governança. Este mecanismo é fundamental para garantir que as regras industriais de limites, tokens restritos e camadas arquiteturais sejam aplicadas de forma consistente em todo o motor de auditoria, assegurando que o portão de conformidade Elite opere sobre dados devidamente normalizados.
    /// [USAGE] Called internally by parse_brute() for each valid configuration entry to update the active governance state of the geode accurately during the initialization phase.
    fn apply_kv(&mut self, section: &str, key: &str, value: &str) {
        match section {
            "architecture.boundaries" => self.architecture.apply_kv(key, value),
            "jurisdiction.exclusive_to" => self.jurisdiction.handle_exclusive_section(key, value),
            "jurisdiction.ignore_to" => self.jurisdiction.handle_ignore_section(key, value),
            "rules.limits" => self.rules.limits.apply_kv(key, value),
            "rules.macros" => self.rules.macros.apply_kv(key, value),
            "manifest" => self.manifest.apply_kv(key, value),
            "shadow_context" => self.shadow.apply_kv(key, value),
            "rules.debug" => self.debug.apply_kv(key, value),
            "rules.restricted_tokens" => { self.rules.restricted_tokens.insert(key.to_string(), value.to_string()); }
            "rules" => self.handle_rules_section(key, value),
            "info" => self.info.apply_kv(key, value),
            "info.context" => self.info.context.push(value.to_string()),
            "info.tools" => self.info.tools.push(value.to_string()),
            "info.terminal_commands" => self.info.terminal_commands.push(value.to_string()),
            _ => self.handle_tag_section(section, key, value),
        }
    }

    /// [WHAT] Processes the global whitelist for authorized third-party dependencies to ensure that the project remains within secure architectural boundaries in the geode.
    /// [WHY] To enforce a strict 'zero-impurity' policy by ensuring that only approved external crates are permitted in the final industrial build of the geode for Elite security.
    /// [WHERE] Security-critical sub-handler for dependency governance within config.rs, providing the primary gate for external library integration and safety verification.
    /// [USAGE] Triggered during configuration parsing when the 'allowed_crates' property is matched in the rules section, updating the global security whitelist for all downstream auditors.
    fn handle_rules_section(&mut self, key: &str, value: &str) {
        if key != "allowed_crates" { return; }
        self.rules.allowed_crates = value.trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    /// [WHAT] Processes dynamic pedagogical tag configurations for the documentation audit engine, allowing for per-tag customization of compliance thresholds.
    /// [WHY] To enable precise customization of documentation requirements such as minimum character counts and AI-ready hints for each specific pedagogical marker (W-HAT, W-HY, etc.).
    /// [WHERE] Sub-dispatcher for the pedagogical rule engine configuration in config.rs, providing granular control over documentation quality standards in the geode.
    /// [USAGE] Triggered during configuration parsing when a sub-section of 'rules.doc_tags' is encountered, updating the specific tag config entry in the rules map accurately.
    fn handle_tag_section(&mut self, section: &str, key: &str, value: &str) {
        if !section.starts_with("rules.doc_tags.") { return; }
        let tag = section.replace("rules.doc_tags.", "");
        let entry = self.rules.doc_tags.entry(tag).or_default();
        match key {
            "min_chars" => entry.min_chars = elite_resolve_or(value.parse().ok(), 0), // [Safety Required]
            "hint" => entry.hint = value.to_string(),
            _ => {}
        }
    }
}

impl LimitsConfig {
    
    
    
    /// [WHAT] Sincroniza os limites estruturais (aninhamento, linhas, parâmetros) com o motor de auditoria de métricas.
    /// [WHY] Para garantir que a massa de código e a complexidade cognitiva do Geode permaneçam sob controle industrial Elite.
    /// [WHERE] Configuração de limites estruturais em config.rs.
    /// [USAGE] Invocada pelo despachante principal para atualizar os limiares de conformidade estrutural, como max_nesting e max_lines, assegurando que o auditor de métricas tenha os dados mais recentes para validação.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "max_lines" => self.max_lines = elite_resolve_or(value.parse().ok(), 0), // [Safety Required]
            "tip_lines" => self.tip_lines = value.to_string(),
            "max_nesting" => self.max_nesting = elite_resolve_or(value.parse().ok(), 0),
            "max_params" => self.max_params = elite_resolve_or(value.parse().ok(), 0),
            "logical_density" => self.logical_density = elite_resolve_or(value.parse().ok(), 0),
            _ => {}
        }
    }
}

impl MacroConfig {
    /// [WHAT] Updates the macro governance model with structural constraints on line length and complexity markers discovered during the manifest parse.
    /// [WHY] To prevent the accumulation of un-auditable logic within macros by enforcing strict industrial size limits and discouraging the use of flow control in expansions.
    /// [WHAT] Define os limites de complexidade e avisos de conformidade para o uso de macros no Geode.
    /// [WHY] Evita o abuso de metaprogramação, forçando a transparência e a auditabilidade estrutural no projeto.
    /// [WHERE] Configuração de macros em config.rs.
    /// [USAGE] Invocada pelo motor de auditoria para configurar as restrições estruturais e léxicas aplicadas à expansão de macros no Geode. Esta função assegura que o auditor de macros tenha visibilidade total sobre os limites de massa de código e as orientações pedagógicas (tips) que devem ser exibidas aos desenvolvedores em caso de violação, promovendo a criação de macros rasas e seguras conforme os padrões industriais Aesthetic Core.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "max_lines" => self.max_lines = elite_resolve_or(value.parse().ok(), 0), // [Safety Required]
            "allow_complex_logic" => self.allow_complex_logic = value == "true",
            "tip" => self.tip = value.to_string(),
            _ => {}
        }
    }
}

impl ManifestConfig {
    /// [WHAT] Updates the semantic manifest configuration to enable or disable AI-ready metadata generation and select active discovery features for the geode.
    /// [WHY] To provide external tools with the necessary switches to control the depth of architectural discovery during audits, optimizing the metadata footprint and relevance.
    /// [WHAT] Configura o motor de geração de manifestos semânticos e as features arquiteturais a serem exportadas.
    /// [WHY] Provê visibilidade sobre as capacidades industriais do geode em formatos consumíveis por agentes de IA externos.
    /// [WHERE] Configuração de manifesto em config.rs.
    /// [USAGE] Invocado pelo despachante do motor de configuração (AuditConfig) para sincronizar o estado das flags de descoberta semântica e do manifesto de governança. Este processo garante que o gerador de metadados industriais saiba exatamente quais camadas arquiteturais processar, permitindo uma descoberta seletiva de símbolos e tags pedagógicas que serão posteriormente exportadas para o artefato de Shadow Context da geode.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "enabled" => self.enabled = value == "true",
            "features" => self.features = parse_csv_list(value),
            _ => {}
        }
    }
}

impl ShadowConfig {
    /// [WHAT] Updates the Shadow Context configuration with the output path and target format markers.
    /// [WHY] To synchronize the exporter state with the project manifest, allowing for dynamic artifact destination control.
    /// [WHAT] Configura o destino e o formato de saída para o artefato de reporte Shadow Context (SARIF).
    /// [WHY] Para permitir a integração flexível com dashboards de conformidade e ferramentas de análise de terceiros.
    /// [WHERE] Configuração de reporte em config.rs.
    /// [USAGE] Invocado pelo orquestrador de configuração para definir o destino e o formato do artefato de reporte de governança exportado pelo motor. Esta configuração permite que o geode se integre perfeitamente a pipelines de CI/CD externos ao direcionar o Shadow Context (SARIF) para os arquivos e caminhos esperados por ferramentas de dashboard e agentes de IA, garantindo que o ciclo de feedback de conformidade seja automatizado e eficiente.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "output" => self.output = value.to_string(),
            "format" => self.format = value.to_string(),
            _ => {}
        }
    }
}

impl DebugConfig {
    /// [WHAT] Updates the debug tracing configuration with the list of authorized debug-zone identifiers to permit controlled log usage in localized Elite geodes.
    /// [WHY] To provide a formal whitelist that permits authorized logging while maintaining production trace sanctity and preventing log pollution in the final industrial binary of the project.
    /// [WHAT] Configura as zonas de depuração autorizadas e as permissões de log para o Geode.
    /// [WHY] Permite que desenvolvedores depurem componentes sem violar permanentemente as restrições de tokens proibidos.
    /// [WHERE] Configuração de debug em config.rs.
    /// [USAGE] Invocado pelo despachante principal para atualizar a lista negra ou branca de rastreamento industrial durante a fase de inicialização do motor. Esta configuração é vital para permitir que zonas de depuração específicas (Safe Harbors) utilizem tokens restritos de forma controlada e auditável, garantindo que o ELITE_TRACE funcione apenas onde explicitamente autorizado pelo manifesto de governança do projeto.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "enabled" => self.enabled = value == "true",
            "allow_list" => self.allow_list = parse_csv_list(value),
            _ => {}
        }
    }
}

impl InfoConfig {
    /// [WHAT] Updates the AI context configuration with priority ordering metadata.
    /// [WHY] To allow the [info] section to configure the priority display order for AI agents.
    /// [WHAT] Atualiza a configuração de contexto de IA com metadados de ordem de prioridade.
    /// [WHY] Para permitir que a seção [info] configure a ordem de exibição das prioridades para agentes de IA.
    /// [WHERE] Componente de configuração de info em config.rs.
    /// [USAGE] Invocado pelo motor de inicialização para processar metadados informativos de alto nível que serão exibidos no dashboard industrial. Esta função captura a ordem de prioridade P0-P3 e outras diretrizes pedagógicas cruciais, assegurando que o reporte visual final no terminal esteja perfeitamente alinhado com as expectativas de governança e triagem arquitetural definidas pelo operador do projeto.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "priority_order" => self.priority_order = value.to_string(),
            _ => {}
        }
    }
}

/// [WHAT] Performs localized string splitting to isolate a configuration key from its assigned value using an industrial-grade deterministic algorithm.
/// [WHY] To provide a lightweight, high-performance string split that avoids the overhead of regex or complex state machines in the zero-dependency configuration engine.
/// [WHERE] Core string utility used globally by the configuration parser in config.rs to process each raw entry in the governance manifest.
/// [USAGE] Called by parse_brute() to pre-process each raw configuration line into a key-value pair for subsequent section dispatch and model synchronization.
fn split_kv(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, '=');
    let k = parts.next()?;
    let v = parts.next()?;
    Some((k, v))
}

/// [WHAT] Parses a TOML-style array of strings (e.g., ["a", "b"]) into a standard vector of clean strings.
/// [WHY] To centralize list parsing logic and satisfy Elite cognitive nesting limits by extracting bulk logic from dispatchers.
/// [WHERE] Internal utility function in config.rs.
/// [USAGE] Chamada por diversos despachantes de configuração como ManifestConfig e DebugConfig para extrair listas de features ou caminhos autorizados, garantindo que a higienização de aspas e colchetes seja feita de forma uniforme e determinística em todo o motor de governança.
fn parse_csv_list(value: &str) -> Vec<String> {
    value.trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
