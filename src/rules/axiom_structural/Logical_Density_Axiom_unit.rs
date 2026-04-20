use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Logical_Density_Axiom_unitcore.rs"]
mod Logical_Density_Axiom_unitcore;

pub struct LogicalDensityAxiom;

impl Default for LogicalDensityAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for LogicalDensityAxiom {
    fn id(&self) -> &str { "logical_density" }
    fn name(&self) -> &str { "LogicalDensityAxiom Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Logical_Density_Axiom_unitcore::patrol(ctx);
    }
}
