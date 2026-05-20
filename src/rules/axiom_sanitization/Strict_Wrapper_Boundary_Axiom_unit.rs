use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "Strict_Wrapper_Boundary_Axiom_unitcore.rs"]
mod Strict_Wrapper_Boundary_Axiom_unitcore;

pub struct StrictWrapperBoundaryAxiom;

impl Default for StrictWrapperBoundaryAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for StrictWrapperBoundaryAxiom {
    fn id(&self) -> &str {
        "strict_wrapper_boundary"
    }

    fn name(&self) -> &str {
        "StrictWrapperBoundaryAxiom Sanitization Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Strict_Wrapper_Boundary_Axiom_unitcore::patrol(ctx);
    }
}
