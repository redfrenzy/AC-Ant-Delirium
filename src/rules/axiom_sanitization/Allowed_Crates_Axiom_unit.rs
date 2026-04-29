use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "Allowed_Crates_Axiom_unitcore.rs"]
mod Allowed_Crates_Axiom_unitcore;

pub struct AllowedCratesAxiom;

impl Default for AllowedCratesAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for AllowedCratesAxiom {
    fn id(&self) -> &str {
        "allowed_crates"
    }
    fn name(&self) -> &str {
        "AllowedCratesAxiom Sanitization Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Allowed_Crates_Axiom_unitcore::patrol(ctx);
    }
}
