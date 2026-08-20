# Inventário read-only do AutoBrowser — T8.1

## Escopo e método

Fotografia estrutural realizada em `2026-08-20T14:43:33-03:00` sobre a origem
administrativa:

```text
/home/dracker/AntigravityProjects/playground/aesthetic_core_project/plugins/AutoBrowser
```

Este path é registrado somente nesta documentação administrativa de migração.
Ele não pode ser transportado para projeções `guard/`, mensagens, erros ou API
do agente.

A inspeção foi estritamente read-only: `stat`, `find` e comandos Git de leitura.
Nenhum JSON foi interpretado, validado ou reformatado; nenhum hash, contagem
autoritativa, permissão individual ou destino de link foi coletado. Essas provas
pertencem à T8.2. Nada foi copiado para staging e nenhum arquivo da origem foi
modificado.

## Estado Git da origem

A própria pasta do plugin é um repositório Git na branch `main`. O `HEAD`
observado foi:

```text
f740587 2026-08-10 Governance Checkpoint: Completed Task [[15.8]]
```

Os quatro commits imediatamente anteriores também são checkpoints das tarefas
15.7, 15.6, 15.5 e 15.4.

O worktree já estava sujo antes da migração:

- `connected.json`, `task_index.txt` e `task_orchestrator.json` estão modificados;
- os planos 0.0, 1.0, 2.0 e 3.0 estão modificados;
- doze nomes antigos de planos aparecem removidos no índice de trabalho;
- doze substitutos com os mesmos números aparecem como arquivos não rastreados;
- os documentos 16, 17 e 18 também aparecem como não rastreados;
- não havia alteração staged.

Essa combinação pode representar renomes sem staging, mas o inventário não faz
essa inferência. A T8.2 deve identificar bytes e metadados exatamente como estão.
Nenhuma operação futura pode usar `git checkout`, restauração ou limpeza para
resolver esse estado.

## Mapa do plugin legado

| Superfície observada | Papel aparente | Tratamento de migração |
|---|---|---|
| `.git/` | histórico próprio do plugin | preservar; não importar automaticamente como estado canônico |
| `ant_delirium_config/` | hierarquia e regras TOML | inventariar como configuração administrativa legado |
| `bin/` | cópias locais de `ac_ant_delirium` e `guard` | preservar até paridade; não executar nem copiar nesta tarefa |
| `plan/` | planos administrativos autoritativos atuais | proteger e manter na origem até cutover validado |
| `task_orchestrator.json` e `task_index.txt` | seleção e índice de tarefas | tratar como estado administrativo, nunca como projeção do agente |
| JSONs na raiz | governança, comunicação, revisão e filas legado | classificar antes de qualquer importação |
| `global/transition_task.json` | transição global legado | preservar separadamente dos planos |
| `problems/` | problema administrativo registrado | preservar |
| `project_state/` | mistura de código do AutoBrowser, fixtures, schemas e evidências | separar produto de estado antes de copiar |
| `scripts/` | scripts ativos e templates de manutenção | não executar; decidir legado versus documentação |
| `docs/` | documentação do AutoBrowser e da governança | preservar como material de referência |
| `trash/` | amostra descartada legado | não promover a plano autoritativo por presença |

## Planos observados

O diretório `plan/` possui as seguintes entradas visíveis no snapshot:

```text
automation/[12.0]_Server_Automation_Review_and_Promotion.json
automation/[14.0]_Server_Owned_Media_Interpretation.json
automation/[15.0]_Server_Governed_Offline_AI_Corpus.json
coderust/[10.0]_Server_Agent_Gateways.json
coderust/[11.0]_Server_Owned_Encrypted_Storage_and_Retention.json
coderust/[3.0]_Cryptographic_Core.json
coderust/[4.0]_Manager_Administrative_Agent.json
coderust/[6.0]_Server_DOM_Sanitizer_V2.json
coderust/[7.0]_Executor_Sandbox_and_Server_Executor_Terminal.json
coderust/[8.0]_Server_Automation_Authoring_and_Test_Pipeline.json
coderust/[9.0]_AutoBrowser_Server_Runtime_and_Browser_Control_Plane.json
extension/[13.0]_Server_Owned_Chrome_Sessions_and_Capability_Control.json
extension/[5.0]_Chrome_Extension_and_Visual_Server_Bridge.json
foundation/[0.0]_Governance_Baseline.json
foundation/[1.0]_Shared_Protocols_And_Schemas.json
foundation/[2.0]_Policy_Taxonomy_And_Guard.json
```

Os números 12, 14, 15, 4–11, 13 e 5 participam do estado de substituição não
rastreado descrito acima. O inventário não declara qual nome é canônico apenas
com base no arquivo atualmente presente.

## Orquestração e estado administrativo

Arquivos de raiz observados:

```text
connected.json
context.json
feedback.json
governance.json
governance_history.json
manager_tasks.json
messages.json
submit_queue.json
task_index.txt
task_orchestrator.json
```

`task_orchestrator.json`, os planos e `governance_history.json` são superfícies
administrativas. Eles não devem ser expostos pela API limitada nem confundidos
com `guard/to-do_list.json` e `guard/governance_error.json`.

## Configuração, transição e problemas

Configuração legado observada:

```text
ant_delirium_config/default_hierarchy.toml
ant_delirium_config/default_rules.toml
```

Transição observada:

```text
global/transition_task.json
```

Problemas e modelos observados:

```text
problems/10.1_need_for_libraries.json
project_state/problems/dom_ambiguous.template.json
project_state/problems/problem.template.json
project_state/problems/selector_changed.template.json
```

O manifesto gerado `project_state/AutoBrowser_props/Logs/artifacts/problem-manifest.json`
é um artefato, não uma segunda fonte de problemas.

## Schemas e contratos observados

A árvore `project_state/schemas/` está separada nos domínios:

- `automation/`: artifact, lifecycle workflow, bindings, compatibilidade e package;
- `dataset/`: manifest e record abstratos;
- `manager/`: evidência de execução, workflow local e startup;
- `manager_executor/`: página abstrata e mensagem;
- `manager_extension/`: snapshot DOM e mensagem;
- `manifests/`: proposal, submission, governance, problem e query;
- `media/`: autorização, evidência e saída sanitizada;
- `server_manager/`: mensagem;
- `storage/`: classificação, migrações SQL de manager/server e validação.

Também existem scripts de validação dentro de alguns desses domínios e a matriz
`V1-COMPATIBILITY-MATRIX.md`. Eles devem ser classificados separadamente dos
schemas JSON na T8.2; não devem ser executados durante o inventário.

## Artefatos e evidências observados

Manifestos derivados em `project_state/AutoBrowser_props/Logs/artifacts/`:

```text
automation-validation-manifest.json
evidence-manifest.json
governance-manifest.json
problem-manifest.json
proposal-violations-manifest.json
```

A pasta `project_state/AutoBrowser_props/evidence/` contém evidências nomeadas
para tarefas dos planos 0.0, 3.0, 5.0–9.0, 11.0 e 13.0–15.0. Ela inclui as
sequências de lifecycle/cleanup dos planos 13–15 e os registros de validação de
CLI, browser, redaction, sandbox, workflows, storage e fronteira do servidor.

Outros artefatos de bootstrap ou consulta observados:

```text
project_state/AutoBrowser_props/bootstrap-manifest.template.json
project_state/AutoBrowser_props/query-api-help.json
```

Esses arquivos são candidatos a histórico/evidência, não a estado canônico por
presença. A classificação final depende dos contratos e hashes da T8.2.

## Código e estado misturados em `project_state/`

No primeiro nível, `project_state/` contém:

```text
.agents/
.codex/
AutoBrowser_props/
autobrowser_config/
automation_proposals/
automations/
crates/
extension/
fixtures/
local/
problems/
references/
schemas/
server/
tmp/
tools/
.gitignore
Cargo.lock
Cargo.toml
README.md
build.rs
guard
```

Portanto, copiar `project_state/` inteiro como se fosse somente estado de
governança seria incorreto: a pasta contém o produto Rust, extensão, servidor,
ferramentas, fixtures e arquivos de build. O arquivo `project_state/guard` é um
arquivo legado, não o diretório de projeções `guard/` definido pela arquitetura
nova. `.agents/` e `.codex/` estavam presentes sem entradas visíveis até três
níveis; `tmp/` deve ser tratado como efêmero até prova contrária.

## Binários e scripts legado

Binários locais observados:

```text
bin/ac_ant_delirium
bin/guard
```

Scripts observados:

```text
scripts/fix_super_block_plugin_binary.sh
scripts/lock_governance_files.sh
scripts/lock_governance_files.template.sh
scripts/unlock_governance_files.sh
scripts/unlock_governance_files.template.sh
scripts/update_plugin_binary_and_lock.sh
scripts/update_plugin_binary_and_lock.template.sh
```

Há scripts ativos e templates lado a lado. Nenhum deles foi executado. Sua
presença não autoriza manter comandos de instalação local no modelo final.

## Riscos e decisões para a próxima tarefa

1. A origem suja precisa ser congelada por hashes e metadados sem restaurar ou
   normalizar o Git.
2. Planos removidos/substitutos não podem ser escolhidos por nome ou checkbox;
   a relação precisa ser registrada como parte do snapshot.
3. `project_state/` precisa ser particionado entre código, configuração,
   histórico, artefatos e temporários antes de qualquer staging.
4. Schemas JSON, SQL, Markdown e scripts de validação não podem receber o mesmo
   tratamento de importação.
5. Binários, scripts ativos e `.git/` permanecem recuperáveis na origem, mas não
   devem virar estado canônico automaticamente.
6. A T8.2 deve validar JSONs, registrar hashes, contagens, modos, ownership e
   links em uma evidência imutável antes da primeira cópia.

## Conclusão da T8.1

As superfícies solicitadas foram localizadas sem mutação: plugin, planos,
orquestrador, schemas, problemas, transição, artefatos e histórico. A origem foi
deliberadamente deixada no estado sujo em que foi encontrada. Este documento é
um mapa de escopo; não é prova de integridade nem autorização de migração.
