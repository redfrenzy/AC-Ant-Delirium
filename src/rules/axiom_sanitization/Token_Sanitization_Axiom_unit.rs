use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Token_Sanitization_Axiom_unitcore.rs"]
mod Token_Sanitization_Axiom_unitcore;

pub struct TokenSanitizationAxiom;

impl Default for TokenSanitizationAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for TokenSanitizationAxiom {
    fn id(&self) -> &str { "token_sanitization" }
    fn name(&self) -> &str { "TokenSanitizationAxiom Sanitization Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Token_Sanitization_Axiom_unitcore::patrol(ctx);
    }
}
