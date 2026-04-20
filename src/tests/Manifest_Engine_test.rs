#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::engine::Manifest_Engine_unit::ManifestEngine;
    use crate::structs::{AuditConfig, DynamicNode, HierarchyConfig};

    fn temp_dir(name: &str) -> String {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        let path = format!("/tmp/{}_{}", name, nonce);
        fs::create_dir_all(&path).expect("temp folder should be created");
        path
    }

    fn set_target_core(hierarchy: &mut HierarchyConfig, target: &str) {
        let node = hierarchy
            .nodes
            .entry("hierarchy.target".to_string())
            .or_insert_with(DynamicNode::default);
        node.properties
            .insert("core".to_string(), format!("[\"{}\"]", target));
    }

    #[test]
    fn unitcore_manifest_false_aggregates_into_unit_manifest() {
        let workspace = temp_dir("manifest_engine");
        let unit_file = format!("{}/Sample_unit.rs", workspace);
        let unitcore_file = format!("{}/Sample_unitcore.rs", workspace);

        fs::write(
            &unit_file,
            "/// [WHAT] Unit entry\npub fn sample_unit() {}\n",
        )
        .expect("unit fixture should be written");
        fs::write(
            &unitcore_file,
            "/// [WHY] Private core\nfn sample_unitcore() {}\n",
        )
        .expect("unitcore fixture should be written");

        let mut hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config");
        set_target_core(&mut hierarchy, &workspace);

        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };

        let written = ManifestEngine::generate(&config).expect("manifest generation should succeed");

        let unit_manifest = format!("{}/Sample_unit_manifest.json", workspace);
        let unitcore_manifest = format!("{}/Sample_unitcore_manifest.json", workspace);

        assert!(
            written.iter().any(|path| path == &unit_manifest),
            "unit manifest should be generated"
        );
        assert!(
            !Path::new(&unitcore_manifest).exists(),
            "unitcore manifest must not exist when child manifest=false"
        );

        let content = fs::read_to_string(&unit_manifest).expect("unit manifest should be readable");
        assert!(
            content.contains("\"source\": \"Sample_unit.rs\""),
            "unit source should be included in parent manifest"
        );
        assert!(
            content.contains("\"source\": \"Sample_unitcore.rs\""),
            "unitcore source should be merged into parent manifest"
        );
    }

    #[test]
    fn plain_rs_generates_its_own_manifest() {
        let workspace = temp_dir("manifest_engine_plain");
        let file_path = format!("{}/alpha.rs", workspace);
        fs::write(&file_path, "/// [WHAT] Plain\npub fn alpha() {}\n").expect("fixture should be written");

        let mut hierarchy = HierarchyConfig::load_from_folder("ant_delirium_config");
        set_target_core(&mut hierarchy, &workspace);
        let config = AuditConfig {
            hierarchy,
            ..AuditConfig::default()
        };

        let written = ManifestEngine::generate(&config).expect("manifest generation should succeed");
        let manifest = format!("{}/alpha_manifest.json", workspace);

        assert!(written.iter().any(|path| path == &manifest));
        assert!(Path::new(&manifest).exists());
    }
}
