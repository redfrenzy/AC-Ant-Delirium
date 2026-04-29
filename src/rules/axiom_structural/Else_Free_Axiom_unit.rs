use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "Else_Free_Axiom_unitcore.rs"]
mod Else_Free_Axiom_unitcore;

pub struct ElseFreeAxiom;

impl Default for ElseFreeAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for ElseFreeAxiom {
    fn id(&self) -> &str {
        "else_free"
    }

    fn name(&self) -> &str {
        "ElseFreeAxiom Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Else_Free_Axiom_unitcore::patrol(ctx);
    }
}
