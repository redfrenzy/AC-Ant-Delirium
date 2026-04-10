# [Aesthetic Core] Módulo de Quarentena: Println

## [WHAT] 
Isolamento industrial para a macro de saída padrão `println!`, que é restrita em todo o ecossistema Aesthetic Core para garantir a pureza dos logs e a previsibilidade do stdout.

## [WHY]
No padrão Elite, a saída direta para o terminal é considerada uma "impureza" que pode interferir em diagnósticos automatizados e sistemas de relatório SARIF. O uso aqui é permitido apenas para bridge de depuração bruta e visualização imediata em casos de falha catastrófica do motor de tracing.

## [WHERE]
Localizado em `src/restricted_tokens/println.rs`, este arquivo serve como o único ponto de saída não-governada no geode, agindo como um "Safe Harbor" para operações de IO bruto.

## [USAGE]
A função `quarantined_print` deve ser invocada apenas quando o sistema de logs ELITE_TRACE for insuficiente ou quando for necessário emitir sinais de pânico visual para o operador humano fora da estrutura SARIF padrão.
