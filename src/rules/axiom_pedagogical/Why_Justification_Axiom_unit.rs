use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "Why_Justification_Axiom_unitcore.rs"]
mod Why_Justification_Axiom_unitcore;

pub struct WhyJustificationAxiom;

impl Default for WhyJustificationAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for WhyJustificationAxiom {
    fn id(&self) -> &str {
        "why_justification"
    }
    fn name(&self) -> &str {
        "WhyJustificationAxiom Pedagogy Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Why_Justification_Axiom_unitcore::patrol(ctx);
    }
}
