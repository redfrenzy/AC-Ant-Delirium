use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "MD_Min_Chars_Axiom_unitcore.rs"]
mod MD_Min_Chars_Axiom_unitcore;

pub struct MdMinCharsAxiom;

impl Default for MdMinCharsAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for MdMinCharsAxiom {
    fn id(&self) -> &str {
        "md_min_chars"
    }
    fn name(&self) -> &str {
        "MdMinCharsAxiom Pedagogy Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        MD_Min_Chars_Axiom_unitcore::patrol(ctx);
    }
}
