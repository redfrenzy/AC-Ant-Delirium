#[cfg(test)]
mod tests {
    use std::fs;

    use crate::engine::Search_Engine_unit::internal::AgnosticScanner;
    use crate::structs::HierarchyConfig;

    #[test]
    fn test_rule_identity_and_requirement_resolution() {
        let config_path = "ant_delirium_config";
        let config = AgnosticScanner::load_from_folder(config_path);

        let identity_paths = config.find_paths_by_id("dna_suffix");
        assert_eq!(identity_paths, vec!["hierarchy.identity.dna_suffix".to_string()]);

        let requirement = config.resolve_parameter("casing");
        assert_eq!(requirement.as_deref(), Some("PascalCase"));

        let functionalities = config.search_functionalities("dna_suffix");
        assert_eq!(
            functionalities,
            vec![("casing".to_string(), vec!["PascalCase".to_string()])]
        );
    }

    #[test]
    fn test_rule_resolution_with_custom_root_name() {
        let raw = fs::read_to_string("ant_delirium_config/default_hierarchy.toml")
            .expect("default_hierarchy.toml deveria existir");
        let raw_rules = fs::read_to_string("ant_delirium_config/default_rules.toml")
            .expect("default_rules.toml deveria existir");
        let custom_hierarchy = raw
            .replace("[hierarchy", "[rule")
            .replace("hierarchy.", "rule.");
        let custom_rules = raw_rules
            .replace("[hierarchy", "[rule")
            .replace("hierarchy.", "rule.");
        let mut config = HierarchyConfig::parse_brute(&custom_hierarchy);
        let rules = HierarchyConfig::parse_brute(&custom_rules);
        crate::engine::Search_Engine_unit::internal::HierarchyResolver::merge_config(&mut config, rules);

        let identity_paths = config.find_paths_by_id("dna_suffix");
        assert_eq!(identity_paths, vec!["rule.identity.dna_suffix".to_string()]);

        let requirement = config.resolve_parameter("casing");
        assert_eq!(requirement.as_deref(), Some("PascalCase"));

        let functionalities = config.search_functionalities("dna_suffix");
        assert_eq!(
            functionalities,
            vec![("casing".to_string(), vec!["PascalCase".to_string()])]
        );
    }

    #[test]
    fn test_taxonomy_parameter_resolution_with_fallback() {
        let config = AgnosticScanner::load_from_folder("ant_delirium_config");

        let (scope, value) = config
            .resolve_taxonomy_parameter("hierarchy.files.unit.unitcore", "logical_density")
            .expect("logical_density deveria ser resolvido no dna atual");
        assert_eq!(scope, "hierarchy.files.unit.unitcore");
        assert_eq!(value, "40");

        let (scope, value) = config
            .resolve_taxonomy_parameter("hierarchy.files.macro.macrocore", "max_params")
            .expect("max_params deveria cair para o primeiro ancestral com valor concreto");
        assert_eq!(scope, "hierarchy.files");
        assert_eq!(value, "5");

        let error = config
            .resolve_taxonomy_parameter("hierarchy.files.unit.unitcore", "banana_param")
            .expect_err("parametro inexistente deveria gerar erro");
        assert!(error.contains("user not define param \"banana_param\" in dna \"hierarchy.files.unit.unitcore\""));
    }
}
