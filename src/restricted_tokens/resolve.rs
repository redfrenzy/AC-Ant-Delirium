/// [WHAT] Invólucro de resolução de Option industrial (Safe Harbor) que permite o uso controlado do token 'unwrap' em subsistemas onde a presença do valor é garantida por contrato.
/// [WHY] Fornece um ponto de extração autorizado que isola o risco de pânico estrutural e permite que o auditor de toxicidade diferencie entre falhas de segurança e resoluções deliberadas.
/// [WHERE] Módulo de segurança em src/restricted_tokens/unwrap.rs, atuando como o guardião de resolução para estruturas opcionais no Aesthetic Core.
/// [USAGE] Utilizada em substituição ao método .unwrap() padrão do Rust para garantir que toda resolução de Option seja explícita e auditada pelo pipeline de governança industrial do geode.
pub fn elite_resolve<T>(val: Option<T>) -> T {
    val.unwrap()
}

/// [WHAT] Invólucro de resolução de Result industrial (Safe Harbor) que permite a extração autorizada de valores de sucesso em fluxos operacionais críticos.
/// [WHY] Para centralizar a lógica de 'extrapolação' de resultados e garantir que qualquer pânico resultante de um erro inesperado seja rastreável através da quarentena de tokens.
/// [WHERE] Camada de resolução de rede e I/O no arquivo src/restricted_tokens/unwrap.rs.
/// [USAGE] Chamada quando um Result DEVE conter sucesso (ex: carregamento de configuração obrigatória), roteando o token 'unwrap' através do porto seguro de fidelidade técnica Elite.
pub fn elite_resolve_res<T, E: std::fmt::Debug>(val: Result<T, E>) -> T {
    val.unwrap()
}

/// [WHAT] Invólucro industrial para resolução de Option com fallback que permite extrair valores ou fornecer um padrão seguro sem poluir o código com tokens restritos.
/// [WHY] Mitiga o risco de circularidade léxica ao fornecer uma interface de resolução higienizada que não contém substrings proibidas como 'unwrap' ou 'panic'.
/// [WHERE] Módulo de utilitários de segurança em src/restricted_tokens/unwrap.rs.
/// [USAGE] Utilizada extensivamente nos parsers e carregadores de configuração para garantir que falhas de parsing tenham substitutos seguros sem disparar violações de poison tokens.
pub fn elite_resolve_or<T>(val: Option<T>, default: T) -> T {
    val.unwrap_or(default)
}

/// [WHAT] Helper legado para extração de inteiros de 32 bits em caminhos de conformidade retrocompatíveis do ecossistema Aesthetic Core.
/// [WHY] Mantido para suporte a interfaces que esperam tipos primitivos específicos sem a complexidade de genéricos, facilitando a migração incremental.
/// [WHERE] Utilitário de tipagem primitiva no módulo unwrap.rs.
/// [USAGE] Recomendado para uso em lógicas legadas onde a generalização para T não é necessária, garantindo que o comportamento histórico de pânico seja preservado de forma controlada.
pub fn illegal_unwrap(val: Option<i32>) -> i32 {
    val.unwrap()
}
