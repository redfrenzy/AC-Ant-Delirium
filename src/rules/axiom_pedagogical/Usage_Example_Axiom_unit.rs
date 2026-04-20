use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Usage_Example_Axiom_unitcore.rs"]
mod Usage_Example_Axiom_unitcore;

pub struct UsageExampleAxiom;

impl Default for UsageExampleAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for UsageExampleAxiom {
    fn id(&self) -> &str { "usage_example" }
    fn name(&self) -> &str { "UsageExampleAxiom Pedagogy Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Usage_Example_Axiom_unitcore::patrol(ctx);
    }
}
