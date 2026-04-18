use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Manifest_Anchoring_Axiom_unitcore.rs"]
mod Manifest_Anchoring_Axiom_unitcore;

pub struct ManifestAnchoringAxiom;

impl Default for ManifestAnchoringAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for ManifestAnchoringAxiom {
    fn id(&self) -> &str { "manifest_anchoring" }
    fn name(&self) -> &str { "ManifestAnchoringAxiom Identity Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Manifest_Anchoring_Axiom_unitcore::patrol(ctx);
    }
}
