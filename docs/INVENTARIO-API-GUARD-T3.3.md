# Inventário da API legada do `guard`

Tarefa: T3.3  
Estado: baseline de migração e allow-list de paridade

## Resultado

O `guard` antigo não era uma API limitada. Ele resolvia um plugin e acrescentava `--project-root` e `--config`, mas em seguida encaminhava todos os argumentos recebidos com `.args(args)`. Como os valores do agente vinham depois dos valores calculados pelo guard, o parser interno ainda aceitava seletores repetidos, consultas administrativas e mutações diretas.

Esse comportamento permanece compilado somente para rollback. Ele não é uma fronteira de confiança e não recebe garantia de paridade no novo cliente.

## Allow-list de capacidades que sobrevivem

Paridade significa preservar a intenção funcional, não necessariamente a grafia antiga, o texto de saída ou o acesso aos mesmos arquivos.

| Capacidade semântica | Forma legada representativa | Destino protegido | Limite obrigatório |
|---|---|---|---|
| ler tarefa atual | `--api hierarchy.task.[PLAN_ID]` | `task.current.read` | somente a tarefa atribuída; sem plano completo ou IDs físicos |
| ler regra relacionada | `--api hierarchy.rule.<rule>` | parte de `task.current.read` | somente regra relacionada à tarefa/violação projetada |
| obter próxima ação | `--next-action` | resposta limitada do agente | uma ação permitida, sem fila ou plano futuro |
| criar submissão | `--submit <evidência>` | `submission.create` | evidência imutável; nunca marca conclusão |
| ler submissão atribuída | `--submit-show <id>` | `submission.read.assigned` | somente própria ou explicitamente atribuída |
| decidir revisão atribuída | `--submit-accept` / `--submit-reject` | `review.decide.assigned` | decisão limitada; sem grade ou edição direta do plano |
| executar auditoria explícita | `audit` | futuro `antdelirium check` | usa projeto descoberto e configuração autorizada, não paths escolhidos pelo agente |

Os testes de paridade cobrem exclusivamente essas sete capacidades. A futura API pode renomear comandos para o contrato canônico, especialmente `hierarchy.task.current.submit`, desde que mantenha a semântica limitada.

## Inventário das demais famílias legadas

| Família observada | Exemplos | Decisão |
|---|---|---|
| apresentação geral | `--help`, `--plugin.help`, `--installation`, `--version`, `--json` | ajuda será reconstruída por perfil; versão segue T3.8; instalação não aparece ao agente |
| seleção física | `--project-root`, `--config`, `--search` | não preservar como input público irrestrito; descoberta e serviço resolvem escopo |
| bootstrap/configuração local | `config dump`, `config init`, `config new_task`, `config hooks ...`, `--init` | substituir pelos fluxos tipados de operador/instalação; sem paridade pública |
| enumeração de topologia | `swarm status`, `--getagents`, `--getprojects`, `--show-agent*`, `--whoami` | não pertence à API pública v1; eventual diagnóstico é escopado ou administrativo |
| manutenção de agentes | `--release-stale-agent-claims`, `--prune-stale-agents` | operador/serviço, sem pass-through |
| histórico e checks internos | `--root-checks`, `--rebase-check`, `--governance-history` | operador, serviço ou CI conforme capability; não expor histórico completo ao agente |
| mensagens e feedback | `--messages`, `--thread`, `--inbox`, `--message`, `--broadcast`, `--feedback` e metadados relacionados | fora da allow-list `protocol-v1`; retirar da superfície pública até existir contrato próprio |
| listagem global de submissões | `--submits`, `--submits-active`, `--submits-history` | remover; somente `submission.read.assigned` sobre item escopado sobrevive |
| administração de submissão | `--submit-note`, `--submit-grade`, `--submit-close` | não preservar no agente; nota pode integrar decisão autenticada, grade direta é retirada |
| tarefas de gerente | `--manager-tasks`, `--manager-task-add`, `--manager-task-done` e modificadores | estado administrativo; somente operador/serviço após contrato tipado |
| contexto/Git | `hierarchy.context.*`, `hierarchy.git.*` | não expor pelo parser genérico; eventual função futura exige operação e schema próprios |
| override do orquestrador | `hierarchy.override.mode`, `hierarchy.override.active_plan` | negar publicamente; somente `state.admin`/`state.transition` autorizados |
| mutação direta de tarefa | `.implemented`, `.task_reviewed`, `.review_status`, `.grade` | remover; submissão e decisão nunca escrevem checkbox diretamente |
| criação/avanço de plano | `.add_task`, `.needs_more_tasks` | operador/serviço; sem paridade pública |
| problemas em arquivo externo | `.problems.*` | substituir por submissão tipada; não revelar nem gravar em pasta de plugin |
| argumento desconhecido | flags ignoradas ou valor tratado como raiz | falhar fechado com `AD-USAGE-0001`; não preservar tolerância silenciosa |

## O que os testes congelam

[`Legacy_Guard_Api_Parity_test.rs`](../src/tests/Legacy_Guard_Api_Parity_test.rs) verifica:

1. a evidência estrutural de que o guard antigo fazia pass-through irrestrito;
2. a allow-list exata das sete capacidades semânticas;
3. a intenção já reconhecida pelo parser para leituras, submissão, revisão e auditoria;
4. ausência de garantia para checkbox, override, grade, configuração, enumeração e mensagens;
5. rejeição conceitual de combinações ambíguas em vez de encaminhamento múltiplo.

Os testes não legitimam o parser antigo como API pública. T3.4 cria a superfície do agente; T3.5 implementa submissão canônica; T3.6 autentica revisão e administração; T3.7 elimina enumeração por ajuda e erro.
