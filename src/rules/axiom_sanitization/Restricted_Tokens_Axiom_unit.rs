use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Restricted_Tokens_Axiom_unitcore.rs"]
mod Restricted_Tokens_Axiom_unitcore;

pub struct RestrictedTokensAxiom;

impl Default for RestrictedTokensAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for RestrictedTokensAxiom {
    fn id(&self) -> &str { "restricted_tokens" }
    fn name(&self) -> &str { "RestrictedTokensAxiom Sanitization Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Restricted_Tokens_Axiom_unitcore::patrol(ctx);
    }
}
