use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "Pascal_Case_Axiom_unitcore.rs"]
mod Pascal_Case_Axiom_unitcore;

pub struct PascalCaseAxiom;

impl Default for PascalCaseAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for PascalCaseAxiom {
    fn id(&self) -> &str {
        "pascal_case"
    }
    fn name(&self) -> &str {
        "PascalCaseAxiom Identity Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Pascal_Case_Axiom_unitcore::patrol(ctx);
    }
}
