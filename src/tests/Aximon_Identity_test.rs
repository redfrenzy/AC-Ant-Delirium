#[cfg(test)]
mod tests {
    use crate::engine::Swarm_Engine_unit::SwarmEngine;
    use crate::structs::{AuditConfig, AuditResult, HierarchyConfig};
    use crate::Logs::GovernanceLogs;

    fn config_for(profile: &str) -> AuditConfig {
        AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder(&format!(
                "ant_delirium_config/{}",
                profile
            )),
            ..AuditConfig::default()
        }
    }

    fn fixture_result(profile: &str, rule_suffix: &str) -> (AuditConfig, AuditResult) {
        let config = config_for(profile);

        let violations = SwarmEngine::audit(&config)
            .violations
            .into_iter()
            .filter(|violation| violation.rule_id.ends_with(rule_suffix))
            .filter(|violation| violation.file_path.contains("/identity_workspace/"))
            .collect();

        let result = AuditResult {
            violations,
            logs: vec![format!(
                "[TEST] Identity sweep filtered by '{}' in profile '{}'.",
                rule_suffix, profile
            )],
        };

        (config, result)
    }

    #[test]
    fn print_tag_separator_violations_from_swarm_manifest() {
        let cases = [
            ("type_1", "tag_separator", "Tag-Separator-Error-unit.rs"),
            ("type_2", "tag_separator", "Tag-Separator-Error-unite.rs"),
            ("type_3", "tag_separator", "Tag-Separator-Error-unidade.rs"),
        ];

        for (profile, rule_suffix, expected_file) in cases {
            let (config, result) = fixture_result(profile, rule_suffix);
            println!(
                "{}",
                GovernanceLogs::format_terminal_report(&result, &config, None)
            );

            assert!(
                result
                    .violations
                    .iter()
                    .any(|violation| { violation.file_path.contains(expected_file) }),
                "missing tag_separator violation for {}",
                profile
            );
        }
    }

    #[test]
    fn print_name_separator_violations_from_swarm_manifest() {
        let cases = [
            ("type_1", "name_separator", "Name_Separator_Error_unit.rs"),
            ("type_2", "name_separator", "Name_Separator_Error_unite.rs"),
            (
                "type_3",
                "name_separator",
                "Name_Separator_Error_unidade.rs",
            ),
        ];

        for (profile, rule_suffix, expected_file) in cases {
            let (config, result) = fixture_result(profile, rule_suffix);
            println!(
                "{}",
                GovernanceLogs::format_terminal_report(&result, &config, None)
            );

            assert!(
                result
                    .violations
                    .iter()
                    .any(|violation| { violation.file_path.contains(expected_file) }),
                "missing name_separator violation for {}",
                profile
            );
        }
    }

    #[test]
    fn print_pascal_case_violations_from_swarm_manifest() {
        let cases = [
            ("type_1", "pascal_case", "pascal_case_error_unit.rs"),
            ("type_2", "pascal_case", "pascal_case_error_unite.rs"),
            ("type_3", "pascal_case", "pascal_case_error_unidade.rs"),
        ];

        for (profile, rule_suffix, expected_file) in cases {
            let (config, result) = fixture_result(profile, rule_suffix);
            println!(
                "{}",
                GovernanceLogs::format_terminal_report(&result, &config, None)
            );

            assert!(
                result
                    .violations
                    .iter()
                    .any(|violation| { violation.file_path.contains(expected_file) }),
                "missing pascal_case violation for {}",
                profile
            );
        }
    }

    #[test]
    fn print_dna_suffix_violations_from_swarm_manifest() {
        let cases = [
            ("type_1", "dna_suffix", "DnaSuffixError.rs"),
            ("type_2", "dna_suffix", "DnaSuffixError.rs"),
            ("type_3", "dna_suffix", "DnaSuffixError.rs"),
        ];

        for (profile, rule_suffix, expected_file) in cases {
            let (config, result) = fixture_result(profile, rule_suffix);
            println!(
                "{}",
                GovernanceLogs::format_terminal_report(&result, &config, None)
            );

            assert!(
                result
                    .violations
                    .iter()
                    .any(|violation| { violation.file_path.contains(expected_file) }),
                "missing dna_suffix violation for {}",
                profile
            );
        }
    }

    #[test]
    fn print_invalid_dna_suffix_violations_from_swarm_manifest() {
        let cases = [
            ("type_1", "dna_suffix", "DnaTypoError_unti.rs"),
            ("type_2", "dna_suffix", "DnaTypoError_unitee.rs"),
            ("type_3", "dna_suffix", "DnaTypoError_unidadi.rs"),
        ];

        for (profile, rule_suffix, expected_file) in cases {
            let (config, result) = fixture_result(profile, rule_suffix);
            println!(
                "{}",
                GovernanceLogs::format_terminal_report(&result, &config, None)
            );

            assert!(
                result
                    .violations
                    .iter()
                    .any(|violation| { violation.file_path.contains(expected_file) }),
                "missing invalid dna_suffix violation for {}",
                profile
            );
        }
    }

    #[test]
    fn print_manifest_anchoring_violations_from_swarm_manifest() {
        let cases = [
            ("type_1", "manifest_anchoring", "Guide_unit.md"),
            ("type_2", "manifest_anchoring", "Guide_unite.md"),
            ("type_3", "manifest_anchoring", "Guide_unidade.md"),
        ];

        for (profile, rule_suffix, expected_file) in cases {
            let (config, result) = fixture_result(profile, rule_suffix);
            println!(
                "{}",
                GovernanceLogs::format_terminal_report(&result, &config, None)
            );

            assert!(
                result
                    .violations
                    .iter()
                    .any(|violation| { violation.file_path.contains(expected_file) }),
                "missing manifest_anchoring violation for {}",
                profile
            );
        }
    }

    #[test]
    fn print_recursion_violations_from_swarm_manifest() {
        let cases = [
            ("type_1", "recursion", "Recursion-Missing_unit.rs"),
            ("type_2", "recursion", "Recursion-Missing_unite.rs"),
            ("type_3", "recursion", "Recursion-Missing_unidade.rs"),
        ];

        for (profile, rule_suffix, expected_file) in cases {
            let (config, result) = fixture_result(profile, rule_suffix);
            println!(
                "{}",
                GovernanceLogs::format_terminal_report(&result, &config, None)
            );

            assert!(
                result
                    .violations
                    .iter()
                    .any(|violation| { violation.file_path.contains(expected_file) }),
                "missing recursion violation for {}",
                profile
            );
        }
    }
}
