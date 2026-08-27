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

A validação instalada será executada, quando a fixture sintética tiver uma
tarefa canônica, com a forma:

```bash
sudo bash packaging/systemd/run-agent-submission.sh \
  --project-root <fixture-sintetica-absoluta> -- \
  --api 'hierarchy.task.current.submit Evidência verificável'
```

Esta entrega ainda não conclui T8G.3. Permanecem provar o launcher de submissão
contra o release instalado, ligar um cliente/supervisor de revisor à rota já
implementada (incluindo a entrega das capabilities separadas de leitura e
decisão) e implementar avanço e histórico no serviço.
