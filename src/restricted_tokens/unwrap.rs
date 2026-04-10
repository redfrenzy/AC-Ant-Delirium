//@ [WHAT] Quarentena industrial para o uso de unwrap() no ecossistema Aesthetic Core.
//@ [WHY] O uso de pânicos não tratados é proibido por padrão; este módulo isola exceções autorizadas para manter a integridade da governança.
//@ [WHERE] ac_ant_delirium/src/restricted_tokens/unwrap.rs

/// [WHAT] Executa uma extração forçada de valor (force-unwrap) que resulta em pânico imediato se o Option/Result estiver vazio.
/// [WHY] Adotado sob a filosofia 'Fail-Fast' industrial, onde estados teoricamente impossíveis devem resultar em interrupção imediata para evitar a propagação de corrupção lógica, preferindo o colapso limpo à operação inconsistente.
/// [WHERE] Quarentena de tokens proibidos em unwrap.rs, servindo como a única zona de exceção autorizada para operações de risco estrutural no geode.
/// [USAGE] Utilizado apenas quando invariantes externas garantem a presença do valor ou quando o custo de tratamento de erro obscurece a clareza da lógica pura em caminhos críticos de execução.
pub fn illegal_unwrap(val: Option<i32>) -> i32 {
    // This UNWRAP is in the correct folder, and NOW authorized in delirium.toml!
    val.unwrap() 
}
