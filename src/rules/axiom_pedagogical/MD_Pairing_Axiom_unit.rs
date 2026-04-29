use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "MD_Pairing_Axiom_unitcore.rs"]
mod MD_Pairing_Axiom_unitcore;

pub struct MdPairingAxiom;

impl Default for MdPairingAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for MdPairingAxiom {
    fn id(&self) -> &str {
        "md_pairing"
    }
    fn name(&self) -> &str {
        "MdPairingAxiom Pedagogy Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        MD_Pairing_Axiom_unitcore::patrol(ctx);
    }
}
