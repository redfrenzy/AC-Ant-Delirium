use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};

#[path = "Tag_Separator_Axiom_unitcore.rs"]
mod Tag_Separator_Axiom_unitcore;

pub struct TagSeparatorAxiom;

impl Default for TagSeparatorAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for TagSeparatorAxiom {
    fn id(&self) -> &str {
        "tag_separator"
    }
    fn name(&self) -> &str {
        "TagSeparatorAxiom Identity Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Tag_Separator_Axiom_unitcore::patrol(ctx);
    }
}
