use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Repository_Anchor_Axiom_unitcore.rs"]
mod Repository_Anchor_Axiom_unitcore;

pub struct RepositoryAnchorAxiom;

impl Default for RepositoryAnchorAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for RepositoryAnchorAxiom {
    fn id(&self) -> &str { "repository_anchor" }
    fn name(&self) -> &str { "RepositoryAnchorAxiom Identity Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Repository_Anchor_Axiom_unitcore::patrol(ctx);
    }
}
