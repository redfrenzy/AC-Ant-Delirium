#[cfg(test)]
mod tests {
    use crate::engine::Swarm_Engine_unit::SwarmEngine;
    use crate::structs::{AuditConfig, HierarchyConfig};

    #[test]
    fn test_swarm_resolves_unitcore_taxonomy_from_file_name() {
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config"),
            ..AuditConfig::default()
        };

        let (detected_dna, taxonomy_path) = SwarmEngine::resolve_file_taxonomy(
            &config,
            "src/rules/axiom_structural/Logical_Density_Axiom_unitcore.rs",
        );

        assert_eq!(detected_dna, "unitcore");
        assert_eq!(taxonomy_path, "hierarchy.files.unit.unitcore");
    }

    #[test]
    fn test_swarm_falls_back_to_scope_root_when_file_has_no_suffix() {
        let config = AuditConfig {
            hierarchy: HierarchyConfig::load_from_folder("ant_delirium_config"),
            ..AuditConfig::default()
        };

        let (detected_dna, taxonomy_path) = SwarmEngine::resolve_file_taxonomy(
            &config,
            "src/rules/metrics.rs",
        );

        assert_eq!(detected_dna, "");
        assert_eq!(taxonomy_path, "hierarchy.files");
    }
}
