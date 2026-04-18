use crate::structs::{HierarchyConfig, GovernanceAxiom, SearchFiles};

#[path = "What_Mission_Axiom_unitcore.rs"]
mod What_Mission_Axiom_unitcore;

pub struct WhatMissionAxiom;

impl Default for WhatMissionAxiom {
    fn default() -> Self { Self {} }
}

impl GovernanceAxiom for WhatMissionAxiom {
    fn id(&self) -> &str { "what_mission" }
    fn name(&self) -> &str { "WhatMissionAxiom Pedagogy Auditor" }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        SearchFiles::resolve_jurisdiction(hierarchy, self.id())
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        What_Mission_Axiom_unitcore::patrol(ctx);
    }
}
