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

Esta entrega ainda não conclui T8G.3. Permanecem a submissão pública ligada ao
canal limitado, revisão, avanço e histórico no serviço instalado.
