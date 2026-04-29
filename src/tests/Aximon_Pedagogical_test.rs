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
            .filter(|violation| violation.file_path.contains("pedagogical_workspace/"))
            .collect();

        let result = AuditResult {
            violations,
            logs: vec![format!(
                "[TEST] Pedagogical sweep filtered by '{}'.",
                rule_fragment
            )],
        };

        (config, result)
    }

    #[test]
    fn print_md_pairing_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".pedagogical.md_pairing");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.is_empty());
    }

    #[test]
    fn print_md_min_chars_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".pedagogical.md_min_chars");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.is_empty());
    }

    #[test]
    fn print_what_mission_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".pedagogical.what_mission");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.is_empty());
    }

    #[test]
    fn print_why_justification_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".pedagogical.why_justification");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.is_empty());
    }

    #[test]
    fn print_usage_example_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".pedagogical.usage_example");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.is_empty());
    }

    #[test]
    fn print_test_case_mapping_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".pedagogical.test_case_mapping");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.is_empty());
    }

    #[test]
    fn print_doc_tags_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".pedagogical.doc_tags");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.iter().any(|violation| {
            violation
                .file_path
                .contains("Test-Case-Mapping-Error_unit.rs")
        }));
    }

    #[test]
    fn print_root_md_pairing_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".pedagogical.root_md_pairing");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.is_empty());
    }

    #[test]
    fn print_inheritance_visibility_violations_from_swarm_manifest() {
        let (config, result) = fixture_result(".pedagogical.inheritance_visibility");
        println!(
            "{}",
            GovernanceLogs::format_terminal_report(&result, &config, None)
        );

        assert!(result.violations.is_empty());
    }
}
