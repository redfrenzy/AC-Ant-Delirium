/// [WHAT] Hub de orquestração central para todas as regras de governança e auditoria industrial do motor Ant Delirium.
/// [WHY] Centraliza e modulariza a lógica do motor de auditoria para facilitar a manutenção e a expansão de novas políticas de conformidade no geode.
/// [WHERE] Ponto de entrada do módulo de regras em ac_ant_delirium/src/rules/mod.rs.
/// [USAGE] Provê acesso aos sub-módulos de métricas, documentação, estrutura e segurança para os orquestradores de auditoria globais do sistema.

pub mod metrics;
pub mod docs;
pub mod structure;
pub mod macros;
pub mod tokens;
pub mod manifest;
pub mod deps;
pub mod markdown;
pub mod quality;
