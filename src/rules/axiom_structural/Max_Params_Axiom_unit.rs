use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Max_Params_Axiom_unitcore.rs"]
mod Max_Params_Axiom_unitcore;

pub struct MaxParamsAxiom;

impl Default for MaxParamsAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for MaxParamsAxiom {
    fn id(&self) -> &str { "max_params" }
    fn name(&self) -> &str { "MaxParamsAxiom Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Max_Params_Axiom_unitcore::patrol(ctx);
    }
}
