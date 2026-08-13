#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::engine::Search_Engine_unit::internal::AgnosticScanner;
    use crate::structs::{DynamicNode, HierarchyConfig, SearchFiles};

    #[test]
    fn test_rule_identity_and_requirement_resolution() {
        let config_path = "ant_delirium_config/type_1";
        let config = AgnosticScanner::load_from_folder(config_path);

        let identity_paths = config.find_paths_by_id("dna_suffix");
        assert_eq!(
            identity_paths,
            vec!["hierarchy.identity.dna_suffix".to_string()]
        );

        let requirement = config.resolve_parameter("tag_separator");
        assert_eq!(requirement.as_deref(), Some("_"));

        let functionalities = config.search_functionalities("dna_suffix");
        assert_eq!(
            functionalities,
            vec![
                ("operation_zone".to_string(), Vec::<String>::new()),
                ("tag_separator".to_string(), vec!["_".to_string()]),
            ]
        );
    }

    #[test]
    fn test_rule_resolution_with_custom_root_name() {
        let raw = fs::read_to_string("ant_delirium_config/type_1/default_hierarchy.toml")
            .expect("default_hierarchy.toml deveria existir");
        let raw_rules = fs::read_to_string("ant_delirium_config/type_1/default_rules.toml")
            .expect("default_rules.toml deveria existir");
        let custom_hierarchy = raw
            .replace("[hierarchy", "[rule")
            .replace("hierarchy.", "rule.");
        let custom_rules = raw_rules
            .replace("[hierarchy", "[rule")
            .replace("hierarchy.", "rule.");
        let mut config = HierarchyConfig::parse_brute(&custom_hierarchy);
        let rules = HierarchyConfig::parse_brute(&custom_rules);
        crate::engine::Search_Engine_unit::internal::HierarchyResolver::merge_config(
            &mut config,
            rules,
        );

        let identity_paths = config.find_paths_by_id("dna_suffix");
        assert_eq!(identity_paths, vec!["rule.identity.dna_suffix".to_string()]);

        let requirement = config.resolve_parameter("tag_separator");
        assert_eq!(requirement.as_deref(), Some("_"));

        let functionalities = config.search_functionalities("dna_suffix");
        assert_eq!(
            functionalities,
            vec![
                ("operation_zone".to_string(), Vec::<String>::new()),
                ("tag_separator".to_string(), vec!["_".to_string()]),
            ]
        );
    }

    #[test]
    fn test_taxonomy_parameter_resolution_with_fallback() {
        let config = AgnosticScanner::load_from_folder("ant_delirium_config/type_1");

        let (scope, value) = config
            .resolve_taxonomy_parameter("hierarchy.files.unit.unitcore", "logical_density")
            .expect("logical_density deveria ser resolvido no dna atual");
        assert_eq!(scope, "hierarchy.files.unit.unitcore");
        assert_eq!(value, "40");

        let (scope, value) = config
            .resolve_taxonomy_parameter("hierarchy.files.macro.macrocore", "max_params")
            .expect("max_params deveria cair para o primeiro ancestral com valor concreto");
        assert_eq!(scope, "hierarchy.files.macro");
        assert_eq!(value, "4");

        let error = config
            .resolve_taxonomy_parameter("hierarchy.files.unit.unitcore", "banana_param")
            .expect_err("parametro inexistente deveria gerar erro");
        assert!(error.contains(
            "user not define param \"banana_param\" in dna \"hierarchy.files.unit.unitcore\""
        ));
    }

    #[test]
    fn test_resolve_jurisdiction_skips_ignored_paths() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be monotonic enough for test setup")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ac_ant_delirium_ignore_test_{}", unique));
        let src_dir = root.join("src");
        let logs_dir = root.join("AC_Ant-Delirium_props/Logs/artifacts");
        let target_dir = root.join("target/debug");

        fs::create_dir_all(&src_dir).expect("src dir should be created");
        fs::create_dir_all(&logs_dir).expect("logs dir should be created");
        fs::create_dir_all(&target_dir).expect("target dir should be created");
        fs::write(src_dir.join("lib.rs"), "pub fn audited() {}\n")
            .expect("source file should be written");
        fs::write(logs_dir.join("noise.rs"), "pub fn noise() {}\n")
            .expect("noise file should be written");
        fs::write(target_dir.join("generated.rs"), "pub fn generated() {}\n")
            .expect("generated file should be written");

        let mut config = HierarchyConfig::default();
        let mut target_node = DynamicNode::default();
        target_node
            .properties
            .insert("core".to_string(), "src".to_string());
        config
            .nodes
            .insert("hierarchy.target".to_string(), target_node);

        let mut search_node = DynamicNode::default();
        search_node
            .properties
            .insert("requirement::target.core".to_string(), "true".to_string());
        config
            .nodes
            .insert("hierarchy.engine.search_file".to_string(), search_node);

        let mut runtime_node = DynamicNode::default();
        runtime_node.properties.insert(
            "project_root".to_string(),
            root.to_string_lossy().to_string(),
        );
        config.nodes.insert("runtime".to_string(), runtime_node);

        let mut exclude_logs = DynamicNode::default();
        exclude_logs
            .properties
            .insert("match".to_string(), "src/Logs".to_string());
        exclude_logs
            .properties
            .insert("ignore".to_string(), "true".to_string());
        config
            .nodes
            .insert("hierarchy.files.exclude_logs".to_string(), exclude_logs);

        let mut exclude_target = DynamicNode::default();
        exclude_target
            .properties
            .insert("match".to_string(), "target".to_string());
        exclude_target
            .properties
            .insert("ignore".to_string(), "true".to_string());
        config
            .nodes
            .insert("hierarchy.files.exclude_target".to_string(), exclude_target);

        let audited = SearchFiles::resolve_jurisdiction(&config, SearchFiles::ID);

        assert_eq!(audited, vec!["src/lib.rs".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }
}
