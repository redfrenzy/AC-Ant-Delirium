use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "Test_Case_Mapping_Axiom_unitcore.rs"]
mod Test_Case_Mapping_Axiom_unitcore;

pub struct TestCaseMappingAxiom;

impl Default for TestCaseMappingAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for TestCaseMappingAxiom {
    fn id(&self) -> &str { "test_case_mapping" }
    fn name(&self) -> &str { "TestCaseMappingAxiom Pedagogy Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Test_Case_Mapping_Axiom_unitcore::patrol(ctx);
    }
}
