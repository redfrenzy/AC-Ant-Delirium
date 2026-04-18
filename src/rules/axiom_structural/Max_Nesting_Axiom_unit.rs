use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Max_Nesting_Axiom_unitcore.rs"]
mod Max_Nesting_Axiom_unitcore;

pub struct MaxNestingAxiom;

impl Default for MaxNestingAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for MaxNestingAxiom {
    fn id(&self) -> &str { "max_nesting" }
    fn name(&self) -> &str { "MaxNestingAxiom Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Max_Nesting_Axiom_unitcore::patrol(ctx);
    }
}
