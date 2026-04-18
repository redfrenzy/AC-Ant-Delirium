use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Recursion_Axiom_unitcore.rs"]
mod Recursion_Axiom_unitcore;

pub struct RecursionAxiom;

impl Default for RecursionAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for RecursionAxiom {
    fn id(&self) -> &str { "recursion" }
    fn name(&self) -> &str { "RecursionAxiom Identity Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Recursion_Axiom_unitcore::patrol(ctx);
    }
}
