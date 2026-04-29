use crate::structs::AuditConfig;

pub fn rule_id(id: &str, suffix: &str) -> String {
    format!("{}.{}", id, suffix)
}

pub fn check_gatekeeper(_config: &AuditConfig) {
    // [TODO] Implementar lógica de gatekeeper industrial.
}
