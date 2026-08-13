#[cfg(test)]
mod tests {
    use crate::engine::Swarm_Engine_unit::SwarmEngine;
    use crate::structs::{AuditConfig, HierarchyConfig};
    use crate::Logs::GovernanceLogs;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

    fn config_for(profile: &str) -> AuditConfig {
        AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder(&format!(
                "ant_delirium_config/{}",
                profile
            )),
            ..AuditConfig::default()
        }
    }

    fn violation_counts_by_rule(profile: &str) -> (usize, BTreeMap<String, usize>) {
        let config = config_for(profile);
        let result = SwarmEngine::audit(&config);
        let mut counts = BTreeMap::new();

        for violation in result.violations {
            let normalized = normalize_rule_id(&violation.rule_id);
            *counts.entry(normalized).or_insert(0) += 1;
        }

        let total = counts.values().copied().sum();
        (total, counts)
    }

    fn normalize_rule_id(rule_id: &str) -> String {
        let mut parts = rule_id.split('.');
        let _root = parts.next().unwrap_or_default();
        let category = parts.next().unwrap_or_default();
        let axiom = parts.next().unwrap_or_default();

        if category.is_empty() || axiom.is_empty() {
            return rule_id.to_string();
        }

        format!("{}.{}", category, axiom)
    }

    #[test]
    fn test_swarm_resolves_unitcore_taxonomy_from_file_name() {
        let config = config_for("type_1");

        let (detected_dna, taxonomy_path) = SwarmEngine::resolve_file_taxonomy(
            &config,
            "src/rules/axiom_structural/Logical_Density_Axiom_unitcore.rs",
        );

        assert_eq!(detected_dna, "unitcore");
        assert_eq!(taxonomy_path, "hierarchy.files.unit.unitcore");
    }

    #[test]
    fn test_swarm_falls_back_to_scope_root_when_file_has_no_suffix() {
        let config = config_for("type_1");

        let (detected_dna, taxonomy_path) =
            SwarmEngine::resolve_file_taxonomy(&config, "src/rules/metrics.rs");

        assert_eq!(detected_dna, "");
        assert_eq!(taxonomy_path, "hierarchy.files");
    }

    #[test]
    fn test_swarm_full_catalog_generates_shadow_context_artifact() {
        let mut config = config_for("type_1");
        config.shadow.output = "shadow_context.json".to_string();

        let audit_result = SwarmEngine::audit(&config);
        assert!(
            !audit_result.logs.is_empty(),
            "swarm should emit orchestration logs"
        );

        let output_root = GovernanceLogs::DEFAULT_OUTPUT_ROOT;
        let paths = GovernanceLogs::write_artifacts(&audit_result, &config, output_root, None)
            .expect("swarm should write log artifacts");

        GovernanceLogs::emit_terminal_report(&audit_result, &config, Some(&paths));
        eprintln!("[TEST][SWARM] terminal: {}", paths.terminal_report_path);
        eprintln!("[TEST][SWARM] manifest: {}", paths.violations_manifest_path);
        eprintln!("[TEST][SWARM] shadow:   {}", paths.shadow_context_path);

        assert!(
            paths
                .terminal_report_path
                .contains("AC_Ant-Delirium_props/Logs/artifacts"),
            "terminal report should live outside the audited src tree"
        );
        assert!(
            paths
                .violations_manifest_path
                .contains("AC_Ant-Delirium_props/Logs/artifacts"),
            "violations manifest should live outside the audited src tree"
        );
        assert!(
            paths.shadow_context_path.ends_with("shadow_context.json"),
            "shadow context should honor the configured output path"
        );
        assert!(
            !paths.shadow_context_path.contains("/src/"),
            "shadow context should live outside the audited src tree"
        );

        assert!(
            Path::new(&paths.shadow_context_path).exists(),
            "shadow context artifact must exist"
        );
        assert!(
            Path::new(&paths.violations_manifest_path).exists(),
            "manifest artifact must exist"
        );

        let shadow = fs::read_to_string(&paths.shadow_context_path)
            .expect("shadow context must be readable");
        assert!(
            shadow.contains("\"results\""),
            "shadow context must include SARIF results array"
        );
        assert!(
            shadow.contains("\"ruleId\": \""),
            "shadow context should include rule identifiers from the active catalog"
        );
    }

    #[test]
    fn test_swarm_isolates_execution_by_profile_target_core() {
        let profiles = [
            ("type_1", "src/exemples_error/type1"),
            ("type_2", "src/exemples_error/type2"),
            ("type_3", "src/exemples_error/type3"),
        ];

        for (profile, expected_root) in profiles {
            let config = config_for(profile);
            let result = SwarmEngine::audit(&config);

            assert!(
                !result.logs.is_empty(),
                "profile {} should produce swarm logs",
                profile
            );

            let inspected: Vec<_> = result
                .violations
                .iter()
                .filter(|v| v.file_path.ends_with(".rs") || v.file_path.ends_with(".md"))
                .collect();

            assert!(
                !inspected.is_empty(),
                "profile {} should inspect files from configured target",
                profile
            );

            for v in inspected {
                assert!(
                    v.file_path.contains(expected_root),
                    "profile {} leaked file outside target root: {}",
                    profile,
                    v.file_path
                );
            }
        }
    }

    #[test]
    fn test_swarm_blocks_when_rule_priorities_conflict() {
        let hierarchy = r#"
[hierarchy.rules.alpha]
priority = 0.1000
requirements = ["operation_zone"]

[hierarchy.rules.beta]
priority = 0.1000
requirements = ["operation_zone"]
"#;

        let config = AuditConfig {
            hierarchy: HierarchyConfig::parse_brute(hierarchy),
            ..AuditConfig::default()
        };

        let result = SwarmEngine::audit(&config);
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.rule_id.contains("rule_priority_conflict")),
            "duplicate rule priority should produce explicit preflight violation"
        );
        let conflict = result
            .violations
            .iter()
            .find(|v| v.rule_id.contains("rule_priority_conflict"))
            .expect("rule priority conflict violation should exist");
        assert!(
            conflict.message.contains("default_rules.toml"),
            "conflict message should direct user to fix TOML"
        );
        assert!(
            conflict.message.contains("assign a different priority"),
            "conflict message should explain why and how to fix"
        );
    }

    #[test]
    fn test_swarm_keeps_same_violation_totals_across_profiles() {
        let type_1 = violation_counts_by_rule("type_1");
        let type_2 = violation_counts_by_rule("type_2");
        let type_3 = violation_counts_by_rule("type_3");

        if type_1.1 != type_2.1 {
            eprintln!("[DIFF][type_2]");
            for key in type_1.1.keys().chain(type_2.1.keys()) {
                let left = type_1.1.get(key).copied().unwrap_or(0);
                let right = type_2.1.get(key).copied().unwrap_or(0);
                if left != right {
                    eprintln!("  {} => type_1={}, type_2={}", key, left, right);
                }
            }
        }

        if type_1.1 != type_3.1 {
            eprintln!("[DIFF][type_3]");
            for key in type_1.1.keys().chain(type_3.1.keys()) {
                let left = type_1.1.get(key).copied().unwrap_or(0);
                let right = type_3.1.get(key).copied().unwrap_or(0);
                if left != right {
                    eprintln!("  {} => type_1={}, type_3={}", key, left, right);
                }
            }
        }

        assert_eq!(
            type_1.0, type_2.0,
            "type_2 total diverged from type_1: {:?} vs {:?}",
            type_1.0, type_2.0
        );
        assert_eq!(
            type_1.0, type_3.0,
            "type_3 total diverged from type_1: {:?} vs {:?}",
            type_1.0, type_3.0
        );
        assert_eq!(
            type_1.1, type_2.1,
            "type_2 normalized rule distribution diverged from type_1"
        );
        assert_eq!(
            type_1.1, type_3.1,
            "type_3 normalized rule distribution diverged from type_1"
        );
    }
}
