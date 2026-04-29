use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "Name_Separator_Axiom_unitcore.rs"]
mod Name_Separator_Axiom_unitcore;

pub struct NameSeparatorAxiom;

impl Default for NameSeparatorAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for NameSeparatorAxiom {
    fn id(&self) -> &str {
        "name_separator"
    }
    fn name(&self) -> &str {
        "NameSeparatorAxiom Identity Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Name_Separator_Axiom_unitcore::patrol(ctx);
    }
}
