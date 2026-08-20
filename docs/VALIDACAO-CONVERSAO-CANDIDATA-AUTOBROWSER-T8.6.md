# Validação da conversão candidata do AutoBrowser — T8.6 parcial

Data da execução: 2026-08-20.

## Escopo desta etapa

Esta etapa resolve os seis bloqueadores encontrados pela T8.5 dentro de uma
cópia privada candidata. Ela não troca a autoridade, não instala ou ativa o
wrapper global, não cria aprovação, não executa cutover e não arquiva, move ou
apaga a origem. A T8.6 permanece aberta até uma atestação independente e uma
aprovação explícita autorizarem a troca em uma operação posterior.

## Implementação

`Legacy-Conversion_unit` adiciona a operação administrativa fechada
`migration.convert`, autorizada somente por capability global
`Operator/Administrative/StateAdmin`. O request vincula os quatro digests da
geração normalizada e também o SHA-256 do relatório T8.5. O serviço exige o
manifesto `staged`, sem approvals, e publica `converted/` por diretório parcial
privado, sincronização e rename atômico.

O schema SQLite 7 contém `legacy_migration_records`, uma tabela privada para
histórico, problemas e transições. Esses registros não são promovidos à API do
agente. Planos e tarefas recebem IDs aleatórios `pln_` e `tsk_`; o documento de
bindings guarda somente hashes das chaves antigas e IDs opacos. Uma tarefa
legada não concluída nunca é inferida como concluída ou revisada. A conversão
recusa estados legados concluídos que exigiriam uma decisão de lifecycle ainda
não autorizada.

## Resultado real

Geração validada:

- migration: `mig_4tdwqnh4dngbhaxhehbvzzsx4t`;
- projeto: `prj_19vszrww3v57118bqhakg704vd`;
- workspace candidato: `wsp_7er1pg0mn2cj0mwd7a8jf3sq4k`;
- relatório T8.5: `f9b98760a41801ab5476e3fb4391352d90076a588843fd01d4b418fd20b1505a`;
- manifesto: `d9cba477a77a4c1b0a6cef9c9521e53fa678a3a16507ae951160d9dcdfeb27ec`;
- bindings opacos: `c2b4b865ca7da88f3194c73c362f94d4e79a8b903aa48f5cef67c5fa0639764d`;
- projeção de tarefa: `a12c06d4b5b6c34194148d5b4c8239a91de2712e0cac94e2a8cde93a03d0a2cc`;
- projeção de erros: `c6223cbd2f1ff1903e723ed1f10ae1f3daed4e9ab14eb366011cc2ab57c64a0f`.

O SQLite candidato retornou `user_version=7`, `quick_check=ok` e nenhuma
violação de foreign key. As contagens de origem/importação coincidiram:

| Domínio | Origem | Candidato |
|---|---:|---:|
| projetos | — | 1 |
| workspaces | — | 1 |
| planos | 16 | 16 |
| tarefas | 73 | 73 |
| submissões | 0 | 0 |
| revisões | 0 | 0 |
| histórico protegido | 275 | 275 |
| problemas protegidos | 1 | 1 |
| transições protegidas | 4 | 4 |

O plano ativo foi resolvido. `current_step_id` estava vazio, portanto nenhuma
tarefa atual foi inventada. As 73 metas públicas foram extraídas do campo
`Concrete goal` e validadas pelo encoder canônico, sem fallback. O problema
legado não possui path de projeto corrigível e ficou retido para o operador; a
projeção do agente contém zero violações em vez de divulgar estado
administrativo.

Uma busca fechada nas duas projeções rejeitou path físico, locator de staging,
`task_orchestrator` e referências `plan/`. A segunda execução retornou o mesmo
candidato e os mesmos digests, comprovando idempotência e validação de
adulteração dos artefatos vinculados.

## Preservação da origem e limite da evidência

Após a conversão, a origem continuou em HEAD
`f7405872e980da505eadc53e40be6589a7507aa4`, com hash do status
`f3b371a69fbb42474abbc4bb6c76324f601896b99f3be7d34e0933c062013033`,
iguais às T8.2–T8.5. O manifesto permaneceu byte-idêntico em `staged`, com
`approval_eligible=false`, `approval_created=false` e
`cutover_performed=false`.

O estado final desta etapa é
`candidate_ready_for_independent_attestation`. Isso comprova conversão e
projeções simuladas, não comprova serviço instalado, wrapper global ativo,
aprovação ou cutover. Esses atos precisam permanecer separados.

O gate final `make ci` passou com 188 testes da biblioteca, 273 testes do
binário e quatro testes reais ignorados por exigirem locators fornecidos pelo
operador, além das integrações 2+1+6. O check estrito, os contratos JSON, a
reverificação do inventário e o release sem paths físicos protegidos também
passaram.
