use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Visibility_Axiom_unitcore.rs"]
mod Visibility_Axiom_unitcore;

pub struct VisibilityAxiom;

impl Default for VisibilityAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for VisibilityAxiom {
    fn id(&self) -> &str { "visibility" }
    fn name(&self) -> &str { "VisibilityAxiom Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Visibility_Axiom_unitcore::patrol(ctx);
    }
}
