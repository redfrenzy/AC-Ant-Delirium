use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "Type_Logic_Axiom_unitcore.rs"]
mod Type_Logic_Axiom_unitcore;

pub struct TypeLogicAxiom;

impl Default for TypeLogicAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for TypeLogicAxiom {
    fn id(&self) -> &str {
        "type_logic"
    }
    fn name(&self) -> &str {
        "TypeLogicAxiom Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Type_Logic_Axiom_unitcore::patrol(ctx);
    }
}
