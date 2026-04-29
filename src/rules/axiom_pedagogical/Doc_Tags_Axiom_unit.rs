use crate::structs::{GovernanceAxiom, HierarchyConfig};

#[path = "Doc_Tags_Axiom_unitcore.rs"]
mod Doc_Tags_Axiom_unitcore;

#[derive(Default)]
pub struct DocTagsAxiom;

impl GovernanceAxiom for DocTagsAxiom {
    fn id(&self) -> &str {
        "doc_tags"
    }

    fn name(&self) -> &str {
        "Doc Tags"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        crate::structs::SearchFiles::resolve_jurisdiction(
            hierarchy,
            crate::structs::SearchFiles::ID,
        )
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Doc_Tags_Axiom_unitcore::patrol(ctx);
    }
}
