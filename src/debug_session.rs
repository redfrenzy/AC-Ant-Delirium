//@ [WHAT] Authorized debug session for ac_ant_delirium.
//@ [WHY] To demonstrate the 'Elite Trace' system where prints are liberated.
//@ [WHERE] ac_ant_delirium/src/debug_session.rs

use crate::ELITE_TRACE;

/// [WHAT] A function that uses the new governed tracing system.
pub fn debug_internal_state() {
    // This PRINTLN is allowed because this file is in the whitelist!
    println!("DEBUG: Starting internal state inspection...");
    
    // This is the preferred way: zero-cost and corporate formatted
    ELITE_TRACE!("Inspecting memory buffers...");
}
