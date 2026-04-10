/// [WHAT] Coleção centralizada de todas as mensagens de violação de governança industrial.
/// [WHY] Padroniza as comunicações do sistema para facilitar a manutenção e o consumo por agentes de IA e dashboards de conformidade.
/// [WHERE] Camada de diagnóstico no módulo rules_logs.rs.
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

/// [WHAT] Macro de log compatível com ambientes WASM e nativos que armazena mensagens em um buffer de string.
/// [WHY] Permite a captura de logs de auditoria em ambientes onde a saída padrão de console é restrita ou invisível durante a execução do geode.
/// [WHERE] Camada de log assíncrono em rules_logs.rs.
/// [USAGE] delirium_log!(logs_vec, "Mensagem: {}", dados); - Esta macro deve se utilizada para preencher o campo de logs do AuditResult durante o processo de escaneamento.
#[macro_export]
macro_rules! delirium_log {
    ($buffer:expr, $($arg:tt)*) => {
        $buffer.push(format!($($arg)*));
    };
}

/// [WHAT] Macro de rastreamento de depuração segura e de custo zero para ambientes industriais Aesthetic Core.
/// [WHY] Fornece capacidades de inspeção detalhada que são removidas em produção, garantindo que logs de depuração não poluam a saída oficial de governança.
/// [WHERE] Sistema de rastreamento Elite em rules_logs.rs.
/// [USAGE] ELITE_TRACE!("Valor: {}", x); - Use esta macro para diagnósticos profundos que só devem ser ativados durante o desenvolvimento local (debug_assertions).
#[macro_export]
macro_rules! ELITE_TRACE {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            $crate::restricted_tokens::out::elite_err(&format!("[ELITE-TRACE] {}", format!($($arg)*)));
        }
    };
}
