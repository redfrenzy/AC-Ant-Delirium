//@ [WHAT] Centralized governance messages and logging infrastructure.
//@ [WHY] To isolate UI/Text from logic and provide WASM-compatible output.
//@ [WHERE] ac_ant_delirium/src/rules_logs.rs

/// [WHAT] Collection of all governance violation messages.
/// [WHY] To centralize and standardize messaging for easier maintenance and AI comprehension.
/// [WHERE] Diagnostic layer in rules_logs.rs.
pub mod messages {
    pub const DOC_TAG_MISSING: &str = "Missing mandatory documentation tag ([WHAT], [WHY], or [WHERE]).";
    pub const DOC_TAG_TOO_SHORT: &str = "Documentation tag content is too short to satisfy pedagogical density (min 64 chars).";
    pub const LINE_LIMIT_EXCEEDED: &str = "Module exceeds clean line limit (300 lines). Extract logic into sub-modules.";
    pub const NESTING_TOO_DEEP: &str = "Abismo Cognitivo: Code nesting is too deep. Break logic into smaller functions.";
    pub const PARAMS_TOO_MANY: &str = "Acoplamento Excessivo: Function has too many parameters. Use a configuration struct.";
    pub const MACRO_COMPLEXITY: &str = "Macro contains complex business logic (if/match/loop). Transform to private function.";
    pub const MACRO_LENGTH: &str = "Macro definition is too long. Extract logic to satisfy clean line constraints.";
    pub const RESTRICTED_TOKEN: &str = "Usage of banned token detected. Check delirium.toml for restricted patterns.";
    pub const ELSE_BLOCK: &str = "Else block detected. Use early returns or match for linear flow logic.";
    pub const MD_PAIRING_MISSING: &str = "Missing mandatory architectural companion (.md file) for this module.";
    pub const DEPS_UNAUTHORIZED: &str = "Unauthorized dependency detected in Cargo.toml. Pure Rust mandate active.";
    pub const LOGICAL_DENSITY: &str = "Excessive Logical Density. Fragment this logic into smaller atomic units.";
    pub const LAYERING_VIOLATION: &str = "Architectural Layering Violation: Illegal dependency direction in DAG.";
    pub const IMPURE_ACTIVATION: &str = "Impure Activation: Optional crate force-activated in default features.";
    pub const STALE_DOCUMENTATION: &str = "Stale Documentation: .rs file is significantly newer than its .md companion.";
    pub const EMPTY_DOCUMENTATION: &str = "Empty Documentation: .md file lacks substantial pedagogical content.";
    pub const WORD_SALAD: &str = "Word Salad Detected: The description is circular or trivial. Provide unique pedagogical value.";
}

/// [WHAT] A WASM-friendly logging macro.
/// [WHY] Since standard 'println!' is often blocked or invisible in WASM geodes.
/// [USAGE] delirium_log!(logs_vec, "Message: {}", data);
#[macro_export]
macro_rules! delirium_log {
    ($buffer:expr, $($arg:tt)*) => {
        $buffer.push(format!($($arg)*));
    };
}
/// [WHAT] A safe, zero-cost debug tracer for Elite geodes.
/// [WHY] To provide debugging capabilities that disappear in production (WASM/Release)
/// and are governed by a whitelist to prevent log pollution.
/// [USAGE] ELITE_TRACE!("Value: {}", x);
#[macro_export]
macro_rules! ELITE_TRACE {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            // Standard corporate debug prefix
            eprintln!("[ELITE-TRACE] {}", format!($($arg)*));
        }
    };
}
