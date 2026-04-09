//@ [WHAT] Safe Harbor for raw printing logic.
//@ [WHY] Banned by Elite Governance in core; must be isolated and authorized.
//@ [WHERE] ac_ant_delirium/src/restricted_tokens/println.rs

/// [WHAT] Encapsulated raw print utility.
/// [WHY] To provide a controlled escape hatch for debugging when ELITE_TRACE is insufficient.
pub fn quarantined_print(msg: &str) {
    // This PRINTLN is allowed ONLY because this file is in the Safe Harbor Whitelist.
    println!("[RESTRICTED-IO] {}", msg);
}
