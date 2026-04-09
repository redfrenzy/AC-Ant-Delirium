//@ [WHAT] Centralized governance messages and logging infrastructure.
//@ [WHY] To isolate UI/Text from logic and provide WASM-compatible output.
//@ [WHERE] ac_ant_delirium/src/rules_logs.rs

/// [WHAT] Collection of all governance violation messages.
pub mod messages {
    pub const DOC_TAG_MISSING: &str = "Missing mandatory documentation tag.";
    pub const DOC_TAG_TOO_SHORT: &str = "Documentation tag content is too short for AI context.";
    pub const LINE_LIMIT_EXCEEDED: &str = "Module exceeds clean line limit. Refactor recommended.";
    pub const NESTING_TOO_DEEP: &str = "Code nesting is too deep. Break logic into smaller functions.";
    pub const PARAMS_TOO_MANY: &str = "Function has too many parameters. Use a configuration struct.";
    pub const MACRO_COMPLEXITY: &str = "Macro contains complex business logic (if/match/loop).";
    pub const MACRO_LENGTH: &str = "Macro definition is too long. Extract logic to a private function.";
    pub const RESTRICTED_TOKEN: &str = "Usage of banned token detected.";
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
