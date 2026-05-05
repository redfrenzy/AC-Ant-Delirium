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

        assert!(
            result.violations.is_empty(),
            "dependency governance does not inspect Cargo.toml while the runtime is restricted to Rust source files"
        );
    }

    #[test]
    fn print_dependency_purification_violations_from_swarm_manifest() {
        let (config, result) =
            fixture_result("hierarchy.dependency_governance.dependency_purification");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(
            result.violations.is_empty(),
            "dependency governance does not inspect Cargo.toml while the runtime is restricted to Rust source files"
        );
    }
}
