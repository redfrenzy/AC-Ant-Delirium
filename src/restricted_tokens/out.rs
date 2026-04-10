/// [WHAT] Orchestrates the emission of raw character streams to the host's standard output using the restricted println macro for diagnostic purposes.
/// [WHY] To provide a controlled escape hatch for debugging when ELITE_TRACE is insufficient and raw terminal feedback is critical for industrial audits.
/// [WHERE] Isolated safety harbor for forbidden IO tokens in println.rs, serving as the authorized conduit for console-based reporting.
/// [USAGE] Invocado por componentes autorizados para despejar informações de estado bruto no stdout, permitindo uma visibilidade técnica imediata durante ciclos de depuração crítica no geode. Esta função deve ser usada com extrema cautela para não poluir a saída principal em ambientes de produção.
/// [WHAT] Invólucro de saída padrão industrial (Safe Harbor) que permite o uso autorizado do token 'print' em contextos de quarentena controlados.
/// [WHY] Para isolar a toxicidade do token de impressão bruto e fornecer uma interface higienizada que não dispara o motor de governança global em módulos ruidosos.
/// [WHERE] Camada de saída higienizada em src/restricted_tokens/println.rs, servindo como o ponto de egress autorizado do geode.
/// [USAGE] Invocado por componentes autorizados no delirium.toml para realizar log operacional sem violar as regras de pureza léxica do ecossistema Elite. Esta função substitui o uso direto de println! em todo o projeto industrial.
pub fn elite_out(msg: &str) {
    // This PRINTLN is allowed ONLY because this file is in the Safe Harbor Whitelist.
    println!("{}", msg);
}

/// [WHAT] Invólucro de saída de erro industrial (Safe Harbor) que permite o uso autorizado do token 'eprint' em contextos de diagnóstico.
/// [WHY] Garante que logs de erro e rastros de pânico sejam roteados de forma controlada através de uma interface de quarentena autoridada pelo gatekeeper.
/// [WHERE] Camada de diagnóstico em src/restricted_tokens/println.rs.
/// [USAGE] Utilizada para reportar falhas críticas e violações de governança diretamente no stream de erro padrão, contornando as restrições de token 'print' do motor de auditoria através da jurisdição de Safe Harbor.
pub fn elite_err(msg: &str) {
    eprintln!("{}", msg);
}
