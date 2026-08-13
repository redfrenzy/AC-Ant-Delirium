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
            .filter(|violation| {
                violation
                    .file_path
                    .contains("src/exemples_error/type1/sanitization_workspace/")
            })
            .collect();

        let result = AuditResult {
            violations,
            logs: vec![format!(
                "[TEST] Sanitization sweep filtered by '{}'.",
                rule_fragment
            )],
        };

        (config, result)
    }

    fn fixture_result_for_profile(
        profile: &str,
        rule_fragment: &str,
    ) -> (AuditConfig, AuditResult) {
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder(&format!(
                "ant_delirium_config/{}",
                profile
            )),
            ..AuditConfig::default()
        };

        let workspace_hint = match profile {
            "type_1" => "src/exemples_error/type1/sanitization_workspace/",
            "type_2" => "src/exemples_error/type2/sanitization_workspace/",
            "type_3" => "src/exemples_error/type3/sanitization_workspace/",
            _ => "src/exemples_error/type1/sanitization_workspace/",
        };

        let violations = SwarmEngine::audit(&config)
            .violations
            .into_iter()
            .filter(|violation| violation.rule_id.contains(rule_fragment))
            .filter(|violation| violation.file_path.contains(workspace_hint))
            .collect();

        let result = AuditResult {
            violations,
            logs: vec![format!(
                "[TEST] Sanitization sweep filtered by '{}' for profile '{}'.",
                rule_fragment, profile
            )],
        };

        (config, result)
    }

    #[test]
    fn print_restricted_tokens_violations_from_swarm_manifest() {
        let (config, result) = fixture_result("hierarchy.sanitization.restricted_tokens");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.iter().any(|violation| {
            violation
                .file_path
                .contains("Restricted-Tokens-Error_unit.rs")
                && violation.rule_id.contains("unwrap")
        }));
        assert!(result.violations.iter().any(|violation| {
            violation
                .file_path
                .contains("Token-Sanitization-Error_unit.rs")
                && violation.rule_id.contains("panic")
        }));
        assert_eq!(result.violations.len(), 2);
    }

    #[test]
    fn print_token_sanitization_violations_from_swarm_manifest() {
        let (config, result) = fixture_result("hierarchy.sanitization.token_sanitization");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.iter().any(|violation| {
            violation
                .file_path
                .contains("Token-Sanitization-Error_unit.rs")
                && violation.rule_id.contains("panic")
        }));
    }

    #[test]
    fn print_allowed_crates_violations_from_swarm_manifest() {
        let (config, result) = fixture_result("hierarchy.dependency_governance.allowed_crates");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.iter().any(|violation| {
            violation
                .file_path
                .contains("sanitization_workspace/Cargo.toml")
                && violation.rule_id.contains("serde_json")
        }));
    }

    #[test]
    fn print_dependency_purification_violations_from_swarm_manifest() {
        let (config, result) =
            fixture_result("hierarchy.dependency_governance.dependency_purification");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.iter().any(|violation| {
            violation
                .file_path
                .contains("sanitization_workspace/Cargo.toml")
                && violation.rule_id.contains("serde")
        }));
    }

    #[test]
    fn print_strict_wrapper_boundary_violations_from_swarm_manifest_across_profiles() {
        let matrix = vec![
            ("type_1", "sanitization_workspace/src/libres/Winit_unit.rs"),
            ("type_2", "sanitization_workspace/src/libres/Winit_unite.rs"),
            (
                "type_3",
                "sanitization_workspace/src/libres/Winit_unidade.rs",
            ),
        ];

        for (profile, file_hint) in matrix.into_iter() {
            let (config, result) = fixture_result_for_profile(
                profile,
                "dependency_governance.strict_wrapper_boundary",
            );
            println!(
                "{}",
                GovernanceLogs::format_terminal_report(&result, &config, None)
            );

            assert!(
                result.violations.iter().any(|violation| {
                    violation.file_path.contains(file_hint)
                        && violation.rule_id.contains("glob_reexport")
                }),
                "missing strict_wrapper_boundary glob_reexport violation for {}",
                profile
            );
        }
    }
}
