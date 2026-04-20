#[cfg(test)]
mod tests {
    use crate::Logs::GovernanceLogs;
    use crate::engine::Swarm_Engine_unit::SwarmEngine;
    use crate::structs::{AuditConfig, AuditResult, HierarchyConfig};

    fn fixture_result(rule_fragment: &str) -> (AuditConfig, AuditResult) {
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config"),
            ..AuditConfig::default()
        };

        let violations = SwarmEngine::audit(&config)
            .violations
            .into_iter()
            .filter(|violation| violation.rule_id.contains(rule_fragment))
            .filter(|violation| violation.file_path.contains("src/exemples_error/identity_workspace/"))
            .collect();

        let result = AuditResult {
            violations,
            logs: vec![format!(
                "[TEST] Identity sweep filtered by '{}'.",
                rule_fragment
            )],
        };

        (config, result)
    }

    #[test]
    fn print_tag_separator_violations_from_swarm_manifest() {
        let (config, result) = fixture_result("hierarchy.identity.tag_separator");
        println!("{}", GovernanceLogs::format_terminal_report(&result, &config, None));

        assert!(result.violations.iter().any(|violation| {
            violation.file_path.contains("Tag-Separator-Error-unit.rs")
        }));
    }

    #[test]
    fn print_name_separator_violations_from_swarm_manifest() {
        let (config, result) = fixture_result("hierarchy.identity.name_separator");
        println!("{}", GovernanceLogs::format_terminal_report(&result, &config, None));

        assert!(result.violations.iter().any(|violation| {
            violation.file_path.contains("Name_Separator_Error_unit.rs")
        }));
    }

    #[test]
    fn print_pascal_case_violations_from_swarm_manifest() {
        let (config, result) = fixture_result("hierarchy.identity.pascal_case");
        println!("{}", GovernanceLogs::format_terminal_report(&result, &config, None));

        assert!(result.violations.iter().any(|violation| {
            violation.file_path.contains("pascal_case_error_unit.rs")
        }));
    }

    #[test]
    fn print_dna_suffix_violations_from_swarm_manifest() {
        let (config, result) = fixture_result("hierarchy.identity.dna_suffix");
        println!("{}", GovernanceLogs::format_terminal_report(&result, &config, None));

        assert!(result.violations.iter().any(|violation| {
            violation.file_path.contains("DnaSuffixError.rs")
        }));
    }

    #[test]
    fn print_manifest_anchoring_violations_from_swarm_manifest() {
        let (config, result) = fixture_result("hierarchy.identity.manifest_anchoring");
        println!("{}", GovernanceLogs::format_terminal_report(&result, &config, None));

        assert!(result.violations.iter().any(|violation| {
            violation.file_path.contains("Guide_unit.md")
        }));
    }

    #[test]
    fn print_recursion_violations_from_swarm_manifest() {
        let (config, result) = fixture_result("hierarchy.identity.recursion");
        println!("{}", GovernanceLogs::format_terminal_report(&result, &config, None));

        assert!(result.violations.iter().any(|violation| {
            violation.file_path.contains("Recursion-Missing_unit.rs")
        }));
    }
}
