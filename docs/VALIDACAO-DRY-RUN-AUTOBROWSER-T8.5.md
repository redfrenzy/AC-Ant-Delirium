# Dry-run e diferenças do AutoBrowser — T8.5

## Resultado

O serviço executou um dry-run path-free sobre a geração normalizada da T8.4 e
produziu um relatório privado sem importar registros, criar aprovação ou fazer
cutover. O SQLite sombra aceitou todas as migrations até o schema 6, passou no
`quick_check` e permaneceu vazio. O manifesto original continuou byte a byte no
estado `staged`.

| Campo | Valor |
|---|---:|
| `migration_id` | `mig_4tdwqnh4dngbhaxhehbvzzsx4t` |
| `project_id` | `prj_19vszrww3v57118bqhakg704vd` |
| schema lógico alvo | `canonical-state-v1` |
| schema SQLite alvo | 6 |
| diferenças bloqueadoras | 6 |
| comparison | `differences_found` |
| cutover readiness | `blocked` |
| approval eligible/created | `false` / `false` |
| cutover performed | `false` |
| manifesto antes/depois | `staged` / `staged` |
| digest do manifesto | `d9cba477a77a4c1b0a6cef9c9521e53fa678a3a16507ae951160d9dcdfeb27ec` |

O request ficou vinculado simultaneamente aos digests da origem, staging,
normalização e representação semântica já congelados nas T8.2–T8.4. Qualquer
troca de geração, recibo, binding, byte staged ou byte normalizado falha antes
da criação ou revalidação do relatório.

## Diferenças encontradas

O dry-run não inventou um mapeamento que ainda não existe. Ele comparou as
contagens semânticas validadas do legado contra o banco sombra canônico vazio:

| Domínio | Legado | Projetado | Bloqueador |
|---|---:|---:|---|
| workspace | 1 | 0 | `workspace_identity_mapping_missing` |
| planos | 16 | 0 | `plan_identity_mapping_missing` |
| tarefas | 73 | 0 | `task_identity_and_state_mapping_missing` |
| histórico | 275 | 0 | `history_event_contract_mapping_missing` |
| problemas | 1 | 0 | `problem_projection_mapping_missing` |
| transições | 4 | 0 | `transition_event_mapping_missing` |
| submissões | 0 | 0 | nenhuma diferença de contagem |
| revisões | 0 | 0 | nenhuma diferença de contagem |

Os planos usam identificadores decimais legados, as tarefas carregam flags
mutáveis como `implemented`, `task_reviewed` e `review_status`, e o histórico
possui seu próprio encadeamento/revisões. O contrato canônico exige IDs opacos,
workspace explícito, revisões e estados fechados. Converter isso exige política
e implementação próprias; equivalência de bytes normalizados não decide essa
tradução.

Por esse motivo, as migrations e integridade do banco alvo passaram, mas a
importação ficou `not_materialized` e a projeção ficou
`blocked_by_unmapped_state`. Esse é o resultado seguro esperado da T8.5: expor
o trabalho restante antes de qualquer autorização, e não transformar um
dry-run em aprovação automática.

## Fronteira administrativa e atomicidade

`migration.dry-run` foi adicionada ao protocolo administrativo `ADA1`. Ela
exige capability global `Operator/Administrative/StateAdmin` e recebe apenas
`migration_id` e quatro digests. O wrapper, a API limitada e o agente não
recebem o socket administrativo, capability, locator de staging ou relatório.
A operação interna `migrate-plugin dry-run` não aparece na ajuda/API pública.

O serviço revalida integralmente staging e normalização usando os bindings
protegidos da T8.4. Depois cria `dry-run.partial`, executa migrations e
integridade em um SQLite sombra, grava o relatório fechado e publica por rename
atômico. Retry revalida toda a cadeia, o banco sombra e o conteúdo exato do
relatório. Diretório parcial preexistente, alteração concorrente do manifesto,
projeto canônico conflitante ou artifact divergente falham fechados.

O relatório não contém path físico. O digest do manifesto prova qual versão foi
avaliada, enquanto `approval_created=false` e `cutover_performed=false` tornam
explícito que nenhum efeito de autoridade foi confundido com a simulação.

## Evidência real

A prova ignorada por padrão foi executada explicitamente contra a derivação
definitiva da T8.4. Ela observou:

- origem, staging, normalização e semântica com os mesmos quatro digests das
  tarefas anteriores;
- 16 planos, 73 tarefas, 275 eventos de histórico, um problema e quatro
  transições na origem validada;
- zero projetos no registro administrativo usado pela prova;
- zero projetos no SQLite sombra;
- `PRAGMA quick_check = ok` e `PRAGMA user_version = 6`;
- relatório válido contra `legacy-dry-run-report-v1`;
- zero locator físico reconhecível no relatório;
- ausência de `dry-run.partial` após a publicação;
- manifesto ainda `staged`, com zero approvals e datas de aprovação, cutover e
  rollback nulas.

A origem AutoBrowser permaneceu com HEAD
`f7405872e980da505eadc53e40be6589a7507aa4` e hash do status
`f3b371a69fbb42474abbc4bb6c76324f601896b99f3be7d34e0933c062013033`,
idênticos às T8.2–T8.4.

O gate final `make ci` passou com 188 testes da biblioteca, 270 testes do
binário e três provas reais ignoradas por padrão, integrações 2+1+6, checks com
warnings negados, schemas JSON válidos e release sem paths físicos protegidos.

## Limite desta tarefa

A T8.5 não cria IDs de workspace/plano/tarefa, não traduz flags de lifecycle,
não importa histórico/problemas/transições, não gera projeção canônica e não
autoriza cutover. A T8.6 deve resolver os seis bloqueadores em staging,
reexecutar o dry-run até obter paridade e somente então tratar a troca de
autoridade como uma operação separada e explicitamente autorizada. A origem
continua preservada e recuperável.
