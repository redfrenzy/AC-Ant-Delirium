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

Esta entrega ainda não conclui T8G.3. O próximo passo é expor o contrato no
canal administrativo autenticado e então conectar submissão, revisão, avanço e
histórico ao serviço instalado.
