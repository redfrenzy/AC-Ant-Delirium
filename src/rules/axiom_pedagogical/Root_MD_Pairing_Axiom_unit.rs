use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "Root_MD_Pairing_Axiom_unitcore.rs"]
mod Root_MD_Pairing_Axiom_unitcore;

pub struct RootMdPairingAxiom;

impl Default for RootMdPairingAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for RootMdPairingAxiom {
    fn id(&self) -> &str {
        "root_md_pairing"
    }
    fn name(&self) -> &str {
        "RootMdPairingAxiom Pedagogy Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Root_MD_Pairing_Axiom_unitcore::patrol(ctx);
    }
}
