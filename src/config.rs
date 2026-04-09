//@ [WHAT] Manual TOML-style parser for Aesthetic Core.
//@ [WHY] To avoid heavy dependencies like 'serde' and 'toml'.
//@ [WHERE] ac_ant_delirium/src/config.rs

use std::collections::HashMap;

#[derive(Default, Debug)]
pub struct TagConfig {
    pub min_chars: usize,
    pub hint: String,
}

#[derive(Default, Debug)]
pub struct MacroConfig {
    pub max_lines: usize,
    pub allow_complex_logic: bool,
    pub tip: String,
}

#[derive(Default, Debug)]
pub struct LimitsConfig {
    pub max_lines: usize,
    pub tip_lines: String,
    pub max_nesting: usize,
    pub max_params: usize,
    pub logical_density: usize,
}

#[derive(Default, Debug)]
pub struct RulesConfig {
    pub limits: LimitsConfig,
    pub doc_tags: HashMap<String, TagConfig>,
    pub restricted_tokens: HashMap<String, String>,
    pub macros: MacroConfig,
    pub allowed_crates: Vec<String>,
}

#[derive(Default, Debug)]
pub struct DebugConfig {
    pub enabled: bool,
    pub allow_list: Vec<String>,
}

#[derive(Default, Debug)]
pub struct ManifestConfig {
    pub enabled: bool,
    pub features: Vec<String>,
}

#[derive(Default, Debug)]
pub struct AuditConfig {
    pub active_to: HashMap<String, String>,
    pub ignore_to: HashMap<String, Vec<String>>,
    pub rules: RulesConfig,
    pub manifest: ManifestConfig,
    pub debug: DebugConfig,
}

impl AuditConfig {
    /// [WHAT] Parses a TOML-like string manually.
    /// [WHY] To keep the geode dependency-free and audit-ready.
    /// [WHERE] Called by the audit_code entry point during geode initialization.
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

    
    
    
    /// [WHAT] Dispatches a specific key-value configuration pair to its corresponding technical sub-section within the broader architectural configuration model. This function acts as the primary traffic controller for the brute-force TOML parser, identifying which geode component or governance rule the incoming data belongs to and ensuring it is routed to the appropriate specialized apply or handle method for final processing and state assignment.
    /// [WHY] To maintain a strictly shallow nesting depth and a linear, early-return-based execution flow that adheres to the Elite Industry standard of code readability and maintainability. By isolating the dispatch logic into this specialized entry point, the engine avoids the 'Cognitive Abyss' of deeply nested match blocks, ensuring that even the most complex configuration manifests remain transparent and easily auditable by both human developers and automated AI agents.
    /// [WHERE] Executed during the configuration initialization phase of the geode audit, specifically invoked by the `parse_brute` loop whenever a valid key-value pair is successfully extracted from a recognized configuration section.
    fn apply_kv(&mut self, section: &str, key: &str, value: &str) {
        match section {
            "active_to" => { self.active_to.insert(key.to_string(), value.to_string()); }
            "rules.limits" => self.rules.limits.apply_kv(key, value),
            "rules.macros" => self.rules.macros.apply_kv(key, value),
            "manifest" => self.manifest.apply_kv(key, value),
            "rules.debug" => self.debug.apply_kv(key, value),
            "rules.restricted_tokens" => { self.rules.restricted_tokens.insert(key.to_string(), value.to_string()); }
            "rules" => self.handle_rules_section(key, value),
            _ => self.handle_tag_section(section, key, value),
        }
    }

    /// [WHAT] Specialized handler for the rules section.
    /// [WHY] To keep nesting depth low and maintain 'Elite' standards.
    /// [WHERE] Internal dispatcher for the brute parser.
    /// [WHAT] Specialized configuration handler dedicated to parsing and applying rules related to the global crate whitelist and architectural dependency constraints. This method specifically targets the 'allowed_crates' property, extracting and normalizing the list of authorized external dependencies from the TOML manifest and updating the geode's internal governance state to reflect these established purity boundaries.
    /// [WHY] To provide a dedicated, shallow-nested entry point for dependency governance configuration that bypasses the need for complex conditional logic within the main dispatch loop. This isolation ensures that the dependency whitelist—the geode's primary defense against impure third-party code—is handled with the highest degree of reliability and pedagogical transparency, supporting the project's mandate for absolute architectural integrity and security.
    /// [WHERE] Dispatched internally by the primary `apply_kv` method whenever a configuration pair belonging to the 'rules' section is encountered, serving as a critical sub-step in the geode's pre-audit initialization sequence.
    fn handle_rules_section(&mut self, key: &str, value: &str) {
        if key != "allowed_crates" { return; }
        self.rules.allowed_crates = value.trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    /// [WHAT] Specialized handler for doc_tag sections.
    /// [WHY] Because doc_tags have dynamic sub-sections in TOML.
    /// [WHERE] Internal dispatcher for the brute parser during geode setup.
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
    /// [WHAT] Parses macro-related key-value pairs.
    /// [WHY] Encapsulation of macro governance for code simplicity.
    /// [WHERE] Used during config initialization to restrict macro complexity.
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
    /// [WHAT] Parses manifest-related key-value pairs.
    /// [WHY] Encapsulation of AI semantic manifest settings.
    /// [WHERE] Used during config initialization to toggle semantic tracing.
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
    /// [WHAT] Parses debug-related key-value pairs.
    /// [WHY] To support authorized tracing zones in Elite geodes.
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

/// [WHAT] Splits a line into key and value.
/// [WHY] Standard KV parsing without external dependencies.
/// [WHERE] Core utility for configuration loading.
fn split_kv(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, '=');
    let k = parts.next()?;
    let v = parts.next()?;
    Some((k, v))
}
