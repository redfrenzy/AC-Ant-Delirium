use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Dependency_Purification_Axiom_unitcore.rs"]
mod Dependency_Purification_Axiom_unitcore;

pub struct DependencyPurificationAxiom;

impl Default for DependencyPurificationAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for DependencyPurificationAxiom {
    fn id(&self) -> &str { "dependency_purification" }
    fn name(&self) -> &str { "DependencyPurificationAxiom Sanitization Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Dependency_Purification_Axiom_unitcore::patrol(ctx);
    }
}
