use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Inheritance_Visibility_Axiom_unitcore.rs"]
mod Inheritance_Visibility_Axiom_unitcore;

pub struct InheritanceVisibilityAxiom;

impl Default for InheritanceVisibilityAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for InheritanceVisibilityAxiom {
    fn id(&self) -> &str { "inheritance_visibility" }
    fn name(&self) -> &str { "InheritanceVisibilityAxiom Pedagogy Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Inheritance_Visibility_Axiom_unitcore::patrol(ctx);
    }
}
