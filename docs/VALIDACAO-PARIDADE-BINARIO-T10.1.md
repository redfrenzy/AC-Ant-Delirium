# Paridade funcional do binário único — T10.1

Data da prova: 1 de setembro de 2026.

## Resultado

T10.1 está concluída. Existe um único target executável, `antdelirium`, e as
sete capacidades legadas aprovadas na T3.3 possuem uma rota atual comprovada.
Paridade significa preservar a função autorizada, não manter nomes físicos,
grafia insegura, seleção de paths ou o pass-through irrestrito do `guard`.

O gate reproduzível é:

```bash
make final-binary-parity
```

Ele usa somente o release atual, testes focados e fixtures Rust mantidas neste
repositório. Não requer `sudo`, não acessa projeto externo e grava evidência
path-free sob `target/t101-final-binary-parity-logs/`.

## Os quatro nomes

| Nome histórico | Papel anterior | Resultado final |
|---|---|---|
| `delirium_cli` | target da CLI madura | funções aprovadas distribuídas entre os perfis do `antdelirium`; parser completo não é público |
| `ac_ant_delirium` | nome do pacote/library e das cópias antigas em plugins | continua somente como identidade interna da crate; não é target executável |
| `guard` | encaminhador que acrescentava paths e repassava qualquer argumento | nenhuma paridade para o pass-through; somente a pasta pública `guard/` permanece |
| `antdelirium` | binário global autoritativo | único `[[bin]]`, wrapper Cargo, agente, revisor, operador e serviço |

`cargo metadata` confirmou um único target de kind `bin`, chamado
`antdelirium`. O nome `ac_ant_delirium` ainda aparece como package/library Rust,
o que não cria segundo ELF nem comando público. Os modos legados continuam
compilados internamente apenas como rollback até a remoção controlada da T10.2;
nenhuma forma pública do dispatcher os seleciona.

## Mapeamento das sete capacidades

| Capacidade preservada | Rota atual | Prova |
|---|---|---|
| tarefa atual | `antdelirium --api hierarchy.task.current` | projeção local validada e resposta `AD-TASK` |
| regra relacionada | `antdelirium --api hierarchy.rule.related` | projeção local validada e resposta `AD-GOV` |
| próxima ação | `antdelirium next-action` | violações têm prioridade; sem violação retorna tarefa atual |
| criar submissão | `hierarchy.task.current.submit` | request tipado, imutável e sem mudança de estado |
| ler submissão atribuída | perfil revisor com capability escopada | protocolo ADP1 limita projeto, workspace, tarefa e submissão |
| decidir revisão atribuída | perfil revisor com capability escopada | decisão aceita/rejeitada sem avanço implícito |
| auditoria explícita | `cargo check` comum | integração instalada aciona o wrapper sem comando alternativo |

A previsão antiga de um `antdelirium check [--project-root ...]` foi retirada.
Ela conflitava com duas decisões já comprovadas: o agente deve usar comandos
Cargo existentes, e paths não podem escolher autoridade. `cargo check` é a
superfície correta porque o agente já o executa, a configuração global seleciona
o wrapper e o CI protegido revalida em target limpo.

## O que deliberadamente não recebe paridade

Não sobrevivem como API pública:

- `implemented=true`, grade, aprovação ou mudança direta de checkbox;
- override de modo ou plano ativo;
- `--project-root`, `--config` e outros seletores físicos;
- enumeração de planos, agentes, projetos, submissões ou histórico global;
- administração, migração e manutenção pelo perfil agente;
- tolerância a argumentos desconhecidos;
- cópia de `guard`, `delirium_cli` ou `ac_ant_delirium` para projetos.

Essas funções não são lacunas: foram removidas da garantia de paridade porque
quebravam a separação entre agente, revisor, operador e serviço.

## Evidência observada

As execuções `20260901T223244Z-121` e `20260901T223320Z-2` passaram. O recibo
final registrou:

```text
physical_binary_count=1
legacy_executable_targets=0
surviving_capability_count=7
explicit_audit=ordinary-cargo-check-passed
legacy_mutation_parity=forbidden
external_project_access=forbidden
validation=pass
```

A matriz de comandos comuns executou `cargo check`, `build`, `test`, `run` e
`clippy` pela integração instalada em um Cargo home descartável. Os testes de
protocolo comprovaram submissão, leitura atribuída e revisão escopadas; os
probes públicos de ajuda e versão produziram stderr vazio e não anunciaram
executáveis ou seletores legados.

## Limite

T10.1 confirma cobertura antes da retirada. Ela não remove ainda os módulos de
rollback internos: isso pertence à T10.2 e deve ocorrer em commit próprio, com
o release anterior e o histórico Git preservados.
