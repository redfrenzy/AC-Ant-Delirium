use crate::structs::{AuditConfig, SearchFiles};

#[path = "Manifest_Engine_unitcore.rs"]
mod Manifest_Engine_unitcore;

pub struct ManifestEngine;

impl ManifestEngine {
    /// [WHAT] Gera manifestos técnicos por arquivo sem depender de violações.
    pub fn generate(config: &AuditConfig) -> std::io::Result<Vec<String>> {
        let files = SearchFiles::resolve_jurisdiction(&config.hierarchy, SearchFiles::ID);
        Manifest_Engine_unitcore::generate_file_manifests(config, &files)
    }
}
