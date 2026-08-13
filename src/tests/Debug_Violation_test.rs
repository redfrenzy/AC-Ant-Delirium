#[cfg(test)]
mod tests {
    use crate::engine::Swarm_Engine_unit::SwarmEngine;
    use crate::structs::{AuditConfig, HierarchyConfig};
    use std::collections::BTreeMap;

    #[test]
    fn test_debug_md_pairing_violations_distribution() {
        for profile in &["type_1", "type_2", "type_3"] {
            println!("\n--- DEBUG PROFILE: {} ---", profile);
            let mut config = AuditConfig {
                hierarchy: HierarchyConfig::load_from_folder(&format!(
                    "ant_delirium_config/{}",
                    profile
                )),
                ..AuditConfig::default()
            };
            config.shadow.output = format!("shadow_context_{}.json", profile);

            let result = SwarmEngine::audit(&config);

            // Gera artefatos (incluindo shadow_context.json)
            crate::Logs::GovernanceLogs::write_artifacts(
                &result,
                &config,
                "Logs/debug_artifacts",
                None,
            )
            .ok();

            let mut counts = BTreeMap::new();

            let mut violations: Vec<_> = result
                .violations
                .iter()
                .filter(|v| v.rule_id.contains("md_pairing"))
                .collect();

            violations.sort_by_key(|v| &v.file_path);

            for violation in violations {
                println!(
                    "  FILE: {} | RULE: {}",
                    violation.file_path, violation.rule_id
                );
                *counts.entry(violation.rule_id.clone()).or_insert(0) += 1;
            }

            for (rule, count) in counts {
                println!("  SUMMARY: {} => {}", rule, count);
            }
        }
    }
}
