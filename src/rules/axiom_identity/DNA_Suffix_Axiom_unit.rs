use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "DNA_Suffix_Axiom_unitcore.rs"]
mod DNA_Suffix_Axiom_unitcore;

pub struct DnaSuffixAxiom;

impl Default for DnaSuffixAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for DnaSuffixAxiom {
    fn id(&self) -> &str {
        "dna_suffix"
    }
    fn name(&self) -> &str {
        "DnaSuffixAxiom Identity Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        DNA_Suffix_Axiom_unitcore::patrol(ctx);
    }
}
