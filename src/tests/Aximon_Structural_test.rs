#[cfg(test)]
mod tests {
    use crate::engine::Swarm_Engine_unit::SwarmEngine;
    use crate::structs::{AuditConfig, AuditResult, HierarchyConfig};
    use crate::Logs::GovernanceLogs;

    fn fixture_result(rule_fragment: &str) -> (AuditConfig, AuditResult) {
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config/type_1"),
            ..AuditConfig::default()
        };

        let violations = SwarmEngine::audit(&config)
            .violations
            .into_iter()
            .filter(|violation| violation.rule_id.contains(rule_fragment))
            .filter(|violation| violation.file_path.contains("src/exemples_error/"))
            .collect();

        let result = AuditResult {
            violations,
            logs: vec![format!(
                "[TEST] Structural sweep filtered by '{}'.",
                rule_fragment
            )],
        };

        (config, result)
    }

    #[test]
    fn print_logical_density_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".structural.logical_density");
        let logical_density_violations = result.violations.clone();

        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(
            logical_density_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_unit.rs")),
            "unit fixture should be detected"
        );
        assert!(
            logical_density_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_unitcore.rs")),
            "unitcore fixture should be detected"
        );
        assert!(
            logical_density_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_macro.rs")),
            "macro fixture should be detected"
        );
        assert!(
            logical_density_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_macrocore.rs")),
            "macrocore fixture should be detected"
        );
        assert!(
            logical_density_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_struct.rs")),
            "struct fixture should be detected"
        );
        assert!(
            logical_density_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_structcore.rs")),
            "structcore fixture should be detected"
        );
    }

    #[test]
    fn print_type_logic_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".structural.type_logic");
        let type_logic_violations = result.violations.clone();

        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(
            type_logic_violations.iter().any(|violation| violation.file_path.contains("Type-Logic-Error_struct.rs")),
            "struct fixture should be detected by type_logic because it mixes struct and fn in the same DNA"
        );
        assert!(
            type_logic_violations.iter().any(|violation| violation.file_path.contains("Type-Logic-Error_unitcore.rs")),
            "unitcore fixture should be detected by type_logic because it leaks a public fn into a private DNA"
        );
        assert!(
            type_logic_violations.iter().any(|violation| violation.file_path.contains("Visibility-Error_struct.rs")),
            "the legacy visibility fixture should now be detected by type_logic as an unsupported struct form"
        );

        assert!(
            !type_logic_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_unit.rs")),
            "unit fixture should remain valid for type_logic"
        );
        assert!(
            !type_logic_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_unitcore.rs")),
            "unitcore fixture should remain valid for type_logic"
        );
        assert!(
            !type_logic_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_macro.rs")),
            "macro fixture should remain valid for type_logic"
        );
        assert!(
            !type_logic_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_macrocore.rs")),
            "macrocore fixture should remain valid for type_logic"
        );
        assert!(
            !type_logic_violations.iter().any(|violation| violation
                .file_path
                .contains("Logical-Dencity-Error_structcore.rs")),
            "structcore fixture should remain valid for type_logic"
        );
    }

    #[test]
    fn print_max_params_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".structural.max_params");
        let max_params_violations = result.violations.clone();

        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(
            max_params_violations
                .iter()
                .any(|violation| violation.file_path.contains("Max-Params-Error_unit.rs")),
            "unit fixture should violate max_params"
        );
        assert!(
            max_params_violations
                .iter()
                .any(|violation| violation.file_path.contains("Max-Params-Error_unitcore.rs")),
            "unitcore fixture should violate max_params"
        );
    }

    #[test]
    fn print_max_nesting_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".structural.max_nesting");
        let max_nesting_violations = result.violations.clone();

        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(
            max_nesting_violations
                .iter()
                .any(|violation| violation.file_path.contains("Max-Nesting-Error_unit.rs")),
            "unit fixture should violate max_nesting"
        );
        assert!(
            max_nesting_violations.iter().any(|violation| violation
                .file_path
                .contains("Max-Nesting-Error_structcore.rs")),
            "structcore fixture should violate max_nesting"
        );
    }

    #[test]
    fn print_else_free_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".structural.else_free");
        let else_free_violations = result.violations.clone();

        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(
            else_free_violations
                .iter()
                .any(|violation| violation.file_path.contains("Else-Free-Error_unitcore.rs")),
            "unitcore fixture with plain else should be detected"
        );
        assert!(
            !else_free_violations.iter().any(|violation| violation
                .file_path
                .contains("Else-Free-Allowed_unitcore.rs")),
            "fixture using a detailed [REASON] bypass should remain valid"
        );
    }
}
