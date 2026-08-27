# Pacote genérico de tarefas — T8G.3

Esta etapa cria a entrada canônica necessária para que um projeto registrado
pelo wrapper deixe de ter `task: null`. Ela pertence ao AntDelirium e não lê,
converte ou modifica qualquer projeto externo.

O contrato fechado
[`governance-task-package-v1`](./contracts/v1/governance-task-package-v1.schema.json)
identifica projeto, workspace, plano e tarefas somente por IDs opacos. Cada
tarefa contém objetivo, critérios de aceite e caminhos relativos permitidos no
projeto; localizadores físicos e referências administrativas são rejeitados
pela mesma política usada nas projeções públicas.

`CanonicalStateStore::install_task_package` grava em uma única transação:

- o plano inicial;
- todas as tarefas e seus payloads canônicos;
- o plano ativo e a primeira tarefa atual;
- o incremento de `state_revision`;
- o evento de auditoria `state_transitioned` vinculado ao operador.

A operação aceita somente projeto canônico vazio. Ela não substitui plano,
tarefa ou estado existente. Retry com o mesmo evento é idempotente; uma segunda
requisição diferente é recusada sem persistência parcial.

Os testes focados comprovaram:

- instalação atômica de duas tarefas sintéticas;
- projeção exclusiva da primeira tarefa, sem revelar a futura;
- retry idempotente;
- rejeição de path privado;
- rejeição de overwrite;
- rollback do evento, plano e tarefas quando qualquer validação falha.

O canal administrativo `ADA1` agora aceita a operação fechada
`governance.task-package.install`, vincula o corpo inteiro à autorização e
exige capability global `Operator/Administrative/StateAdmin`. O serviço resolve
o projeto somente pelo ID opaco no registry, abre o banco protegido sob sua
própria autoridade e devolve recibo sem localizador físico. Capability limitada
ou escopo divergente são recusados e auditados.

O cliente administrativo fechado `operator task-package install` recebe o JSON
somente pelo descriptor já aberto indicado por `ANTDELIRIUM_TASK_PACKAGE_FD`.
Ele não aceita nome de arquivo na linha de comando, exige arquivo regular,
somente leitura, no offset zero e menor que o frame ADA1. O descriptor recebe
`CLOEXEC`; as variáveis do socket e do pacote são apagadas antes da conexão, e a
capability continua chegando pelo descriptor separado já existente. O recibo
exibe somente IDs opacos, revisão, contagens e estado — nunca o caminho do
pacote, do banco ou do projeto.

O listener limitado agora reconhece `submission.create` somente com capability
`Agent/Limited/SubmissionCreate` vinculada exatamente ao projeto, workspace e
tarefa do payload `submission-v1`. O serviço valida o contrato antes de consumir
a autorização, resolve o banco apenas pela capability, atribui `submission_id`,
fingerprint e horário e grava submissão mais `submission_recorded` na mesma
transação. A operação não altera revisão do estado, tarefa atual, plano ativo ou
status de conclusão. O teste ADA1/ADP1 sintético comprovou ainda que o recibo
limitado contém somente ID opaco, sequência e estado de deduplicação.
O cliente agente preserva o `submission_id` desse recibo e imprime somente
`antdelirium[AD-SUBMISSION]: sub_<id>`. Assim o operador pode atribuir a
submissão exata ao revisor sem consultar banco protegido, plano, TOML ou
localizador físico; sequência e estado interno de deduplicação continuam fora
da saída pública.

O transporte real do perfil agente usa o endpoint limitado fornecido pela
instalação e uma capability efêmera `agent-submission-capability-v1` entregue
pelo supervisor em descriptor herdado. A variável
`ANTDELIRIUM_AGENT_CAPABILITY_FD` contém somente o número do descriptor. O
cliente apaga as duas variáveis antes de descobrir o projeto, fecha descriptors
ausentes ou não consumidos e envia o mesmo `request_id` no envelope e no
`submission-v1`. O emissor administrativo gera esse contrato apenas para a
combinação exata `agent/limited/submission.create`; o leitor administrativo
recusa usá-lo como credencial de operador.

O supervisor revisável
[`run-agent-submission.sh`](../packaging/systemd/run-agent-submission.sh) fecha o
ciclo sem criar uma segunda API para o agente. Ele aceita somente a forma
pública já existente `--api 'hierarchy.task.current.submit …'`, lê projeto,
workspace e tarefa da projeção pública, emite uma capability de cinco minutos
vinculada aos três IDs e executa o mesmo binário sob o UID não-root que chamou
`sudo`. O plaintext existe apenas em diretório `0700` sob `/run`, nunca entra no
projeto ou argv, e a capability é revogada logo após a chamada. Se qualquer
etapa posterior ao grant falhar, o trap tenta novamente a revogação antes de
remover os descritores. Os logs sanitizados ficam neste checkout em
`target/t8g-agent-submission-launcher-logs/`.

O listener limitado também reconhece `review.decide.assigned` somente com uma
capability `Reviewer/Limited/ReviewDecideAssigned` vinculada ao mesmo projeto,
workspace e tarefa. Depois da autorização, o serviço consulta a submissão no
banco protegido e exige que sua vinculação canônica coincida com todos esses
IDs antes de registrar `accepted` ou `rejected`. A decisão e o evento
`review_recorded` são gravados atomicamente e o recibo contém apenas `review_id`,
sequência e deduplicação. O teste ADP1 sintético percorre instalação de pacote,
submissão e revisão e confirma que a revisão aceita não muda a tarefa atual nem
o `state_revision`; avanço continua sendo uma operação separada do serviço.

Antes da decisão, `submission.read.assigned` permite ao revisor obter somente a
submissão indicada por uma capability `Reviewer/Limited/SubmissionReadAssigned`
presa ao mesmo projeto, workspace e tarefa. O serviço confirma a vinculação no
banco e desserializa o payload persistido em um tipo fechado `submission-v1`,
reaplicando as validações de IDs, evidência e não divulgação antes da resposta.
Não existe listagem, busca global, path de banco ou leitura de submissão por ID
sem o escopo completo.

O perfil revisor agora possui parser próprio para `review assigned sub_<id>` e
`review decide sub_<id> accepted|rejected '<justificativa>'`. O cliente lê
respectivamente `reviewer-assignment-capability-v1` e
`reviewer-decision-capability-v1` de descriptor herdado, fecha o descriptor e
usa somente ADP1. A saída mostra apenas a submissão atribuída ou o `review_id`;
sem endpoint, IDs de escopo e capability a chamada falha fechada. O agente não
ganhou esses comandos ou contratos.

O supervisor revisável
[`run-reviewer-operation.sh`](../packaging/systemd/run-reviewer-operation.sh)
recebe a raiz da fixture e um `submission_id` opaco, lê somente a projeção
pública para obter projeto/workspace/tarefa e emite uma capability de cinco
minutos para exatamente uma operação. O descriptor é entregue ao mesmo binário
sob o UID não-root que chamou `sudo` e revogado imediatamente depois; o `trap`
repete a revogação em falhas intermediárias. Segredos vivem somente em `/run`
durante a chamada, e os logs sanitizados ficam em
`target/t8g-reviewer-launcher-logs/`.

A rotina
[`validate-installed-task-cycle.sh`](../packaging/systemd/validate-installed-task-cycle.sh)
compõe esses contratos somente sobre uma fixture criada pelo gate genérico sob
`target/`. Ela recusa qualquer raiz fora deste checkout, instala uma tarefa
sintética com o descriptor `StateAdmin`, força um `cargo check` normal para
atualizar as projeções e percorre submissão, leitura atribuída e decisão. O
resultado exige que a tarefa continue disponível na mesma `source_revision`,
pois revisão e avanço permanecem operações diferentes.
Como o rollout atual preserva o filho audit-only assíncrono, o gate aguarda por
no máximo cinco segundos a publicação atômica esperada; ele não usa uma espera
fixa e encerra assim que a revisão correta aparece.

A primeira execução instalada revelou uma invariância ausente: a instalação do
pacote avançava o estado canônico para a revisão 1, mas o snapshot canônico de
erros continuava vinculado à revisão 0. O serviço conseguia renderizar a tarefa,
recusava a projeção de erros e, como o refresh devolve os dois documentos em uma
única resposta, o wrapper não materializava nenhum deles. A instalação agora
carrega o snapshot já validado para a nova revisão, recalcula seu fingerprint e
faz isso na mesma transação do plano, tarefas, estado e auditoria. Snapshot
ausente, adulterado ou concorrente aborta toda a instalação. Testes focados
comprovam o refresh limitado na revisão 1 e o rollback sem evento, plano ou
tarefa parcial. A repetição contra o serviço instalado continua necessária
depois da implantação do novo release e está registrada na prova abaixo.

Se a instalação canônica já foi confirmada e uma etapa posterior falhar, a
rotina retoma com `--resume-task-id tsk_<id>` e não abre novamente o descriptor
`StateAdmin` nem tenta reinstalar o pacote.

A validação instalada será executada, quando a fixture sintética tiver uma
tarefa canônica, com a forma:

```bash
sudo bash packaging/systemd/run-agent-submission.sh \
  --project-root <fixture-sintetica-absoluta> -- \
  --api 'hierarchy.task.current.submit Evidência verificável'

sudo bash packaging/systemd/run-reviewer-operation.sh \
  --project-root <fixture-sintetica-absoluta> -- assigned sub_<id>

sudo bash packaging/systemd/run-reviewer-operation.sh \
  --project-root <fixture-sintetica-absoluta> -- \
  decide sub_<id> accepted 'Justificativa verificável'

sudo bash packaging/systemd/validate-installed-task-cycle.sh \
  --project-root <fixture-sintetica-absoluta-sob-target>
```

## Prova contra a instalação real

O ciclo instalado passou em 27 de agosto de 2026 com o release de SHA-256
`baa3faf17670743cae836fef92b2d043c06071ef77446b172512cc6d5b752611`.
Serviço, wrapper Cargo, bundle e release local tinham exatamente esse hash. Em
uma fixture canônica nova, o gate comprovou:

- instalação de uma tarefa e avanço conjunto das duas projeções para a revisão 1;
- submissão pelo comando público existente, com retorno apenas do ID opaco;
- emissão e revogação da capability de submissão de uso único;
- leitura somente da submissão atribuída pelo revisor;
- decisão `accepted` sem conclusão ou avanço implícito da tarefa;
- emissão e revogação separadas das capabilities de leitura e decisão;
- ausência de acesso a projeto externo e de localizadores físicos nos logs de aceite.

O recibo reproduzível está em
`target/t8g-task-cycle-logs/20260827T201921Z-75927/acceptance.env` e registra
`task_package_install=pass`, `cargo_projection_refresh=pass`,
`agent_submission=pass`, `assigned_submission_read=pass` e
`assigned_review_decision=pass`. A tarefa permaneceu disponível na revisão 1,
confirmando que revisão e avanço são autoridades diferentes.

Esta entrega ainda não conclui T8G.3. Permanecem implementar e validar avanço
canônico e histórico protegido no serviço.
