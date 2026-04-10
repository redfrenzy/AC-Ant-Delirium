//@ [WHAT] Manual TOML-style parser for Aesthetic Core.
//@ [WHY] To avoid heavy dependencies like 'serde' and 'toml'.
//@ [WHERE] ac_ant_delirium/src/config.rs

use std::collections::HashMap;

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

/// [WHAT] Configuration for AI-facing contextual metadata injected into SARIF properties and terminal summaries.
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
    fn handle_exclusive_section(&mut self, key: &str, value: &str) {
        let rules: Vec<String> = value.trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.exclusive_to.insert(key.to_string(), rules);
    }

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
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }

            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len()-1].to_string();
                continue;
            }

            if let Some((k, v)) = split_kv(line) {
                config.apply_kv(&section, k.trim().trim_matches('"'), v.trim().trim_matches('"'));
            }
        }
        config
    }

    /// [WHAT] Dispatches a raw TOML key-value pair to its corresponding sub-config section based on the current context of the brute parser.
    /// [WHY] To maintain a shallow execution flow and prevent deeply nested match blocks from obscuring the configuration logic, satisfying Elite structural requirements for industrial geodes.
    /// [WHERE] Primary internal routing mechanism for the AuditConfig data model in config.rs, providing the logic for hierarchical property assignment and rule synchronization.
    /// [USAGE] Called internally by parse_brute() for each valid configuration entry to update the active governance state of the geode accurately during the initialization phase.
    fn apply_kv(&mut self, section: &str, key: &str, value: &str) {
        match section {
            "architecture.boundaries" => self.architecture.apply_kv(key, value),
            "jurisdiction.exclusive_to" => self.jurisdiction.handle_exclusive_section(key, value),
            "jurisdiction.ignore_to" => self.jurisdiction.handle_ignore_section(key, value),
            "rules.limits" => self.rules.limits.apply_kv(key, value),
            "rules.macros" => self.rules.macros.apply_kv(key, value),
            "manifest" => self.manifest.apply_kv(key, value),
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
            "min_chars" => entry.min_chars = value.parse().unwrap_or(0), // [Safety Required]
            "hint" => entry.hint = value.to_string(),
            _ => {}
        }
    }
}

impl LimitsConfig {
    
    
    
    /// [WHAT] Updates the internal structural limit model with a new configuration key-value pair to synchronize active governance thresholds across the geode.
    /// [WHY] To ensure that architectural limits on nesting, lines, and parameters are correctly reflected in the active audit state, preventing logical drift in the geode during CI.
    /// [WHERE] Component parser for structural constraints within config.rs, acting as the final implementation layer for limit updates and architectural policy enforcement.
    /// [USAGE] Called by the AuditConfig dispatcher to update specific limit fields such as max_lines or max_nesting based on the provided configuration tokens from the manifest.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "max_lines" => self.max_lines = value.parse().unwrap_or(0), // [Safety Required]
            "tip_lines" => self.tip_lines = value.to_string(),
            "max_nesting" => self.max_nesting = value.parse().unwrap_or(0),
            "max_params" => self.max_params = value.parse().unwrap_or(0),
            "logical_density" => self.logical_density = value.parse().unwrap_or(0),
            _ => {}
        }
    }
}

impl MacroConfig {
    /// [WHAT] Updates the macro governance model with structural constraints on line length and complexity markers discovered during the manifest parse.
    /// [WHY] To prevent the accumulation of un-auditable logic within macros by enforcing strict industrial size limits and discouraging the use of flow control in expansions.
    /// [WHERE] Component parser for macro-specific governance rules in config.rs, serving as the tactical layer for macro isolation policy enforcement.
    /// [USAGE] Called by the AuditConfig dispatcher to update macro configuration state, ensuring that the macro auditor has the latest thresholds for complexity detection in the geode.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "max_lines" => self.max_lines = value.parse().unwrap_or(0), // [Safety Required]
            "allow_complex_logic" => self.allow_complex_logic = value == "true",
            "tip" => self.tip = value.to_string(),
            _ => {}
        }
    }
}

impl ManifestConfig {
    /// [WHAT] Updates the semantic manifest configuration to enable or disable AI-ready metadata generation and select active discovery features for the geode.
    /// [WHY] To provide external tools with the necessary switches to control the depth of architectural discovery during audits, optimizing the metadata footprint and relevance.
    /// [WHERE] Component parser for manifest-related settings in config.rs, providing the implementation logic for manifest generation toggles and feature synchronization.
    /// [USAGE] Called by the AuditConfig dispatcher to synchronize manifest generation state and active feature flags with the provided configuration manifest during geode initialization.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "enabled" => self.enabled = value == "true",
            "features" => {
                self.features = value.trim_matches(|c| c == '[' || c == ']')
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .collect();
            }
            _ => {}
        }
    }
}

impl DebugConfig {
    /// [WHAT] Updates the debug tracing configuration with the list of authorized debug-zone identifiers to permit controlled log usage in localized Elite geodes.
    /// [WHY] To provide a formal whitelist that permits authorized logging while maintaining production trace sanctity and preventing log pollution in the final industrial binary of the project.
    /// [WHERE] Component parser for debug and tracing governance in config.rs, acting as the primary security layer for diagnostic output and trace-level policy enforcement.
    /// [USAGE] Called by the AuditConfig dispatcher to update the authorized debug allow-list and enabled state, ensuring consistent log levels across the entire architectural tree.
    fn apply_kv(&mut self, key: &str, value: &str) {
        match key {
            "enabled" => self.enabled = value == "true",
            "allow_list" => {
                self.allow_list = value.trim_matches(|c| c == '[' || c == ']')
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
}

impl InfoConfig {
    /// [WHAT] Updates the AI context configuration with priority ordering metadata.
    /// [WHY] To allow the [info] section to configure the priority display order for AI agents.
    /// [WHERE] Component parser for the info section in config.rs.
    /// [USAGE] Called by the AuditConfig dispatcher when processing keys directly under [info].
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
