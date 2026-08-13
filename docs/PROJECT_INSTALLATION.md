# AntDelirium: binário único e superfície limitada do agente

Este documento descreve o estado implementado até a T4.5. O projeto declara um único executável autoritativo chamado `antdelirium`; a instalação global e a integração automática com Cargo ainda pertencem às ondas posteriores.

## Compilar

Na raiz deste repositório:

```bash
make binary
```

O único artefato executável produzido pelo pacote fica em:

```text
target/x86_64-unknown-linux-gnu/release/antdelirium
```

Esse alvo aplica os remapeamentos de paths exigidos pelo release. Para também
rejeitar qualquer caminho físico protegido ou do ambiente do mantenedor que
tenha chegado ao ELF, execute:

```bash
make release-nondisclosure
```

## Usar durante o desenvolvimento

Enquanto os handlers autenticados, o instalador e o wrapper ainda não estiverem concluídos, execute o artefato diretamente a partir deste checkout:

```bash
./target/x86_64-unknown-linux-gnu/release/antdelirium --help
./target/x86_64-unknown-linux-gnu/release/antdelirium --api hierarchy.task.current
./target/x86_64-unknown-linux-gnu/release/antdelirium --api hierarchy.rule.related
./target/x86_64-unknown-linux-gnu/release/antdelirium next-action
./target/x86_64-unknown-linux-gnu/release/antdelirium --version
```

A superfície do agente já deixou de encaminhar argumentos à CLI madura. Ela possui parser próprio e reconhece somente tarefa atual, regra relacionada, próxima ação e a forma tipada de submissão:

```bash
./target/x86_64-unknown-linux-gnu/release/antdelirium --api \
  'hierarchy.task.current.submit Evidência verificável'
```

As consultas conferem somente os artefatos públicos regulares `guard/to-do_list.json` e `guard/governance_error.json` do projeto descoberto, rejeitam symlinks, versão/tipo inesperado e conteúdo com referência de path privado, e então emitem apenas `AD-TASK` ou `AD-GOV`. Elas não procuram plugin, plano, TOML, banco ou state root externo.

A submissão agora lê a vinculação opaca da tarefa atual, gera um `request_id` no formato do protocolo limitado e constrói um payload `submission-v1` contendo somente projeto, workspace, tarefa, revisão e evidência. O handler autorizado adapta esse payload ao `SubmissionStore` transacional existente: inserção e auditoria são atômicas, retries idênticos são deduplicados e nenhuma chamada de transição canônica faz parte desse fluxo.

O cliente do agente deliberadamente não abre o banco protegido. Enquanto o parser de requests e os handlers tipados ainda não estiverem ligados à fronteira autenticada, o comando real termina com `AD-SERVICE-0001`; em testes, um transporte injetado comprova a entrega tipada e o handler de serviço comprova a persistência. Isso não é um fallback para SQLite local, arquivo de fila no projeto ou descoberta de state root.

`--version` é uma consulta local de metadados e não conta como uma quinta ação de governança. Ela retorna somente em stdout, com exit `0`:

```text
antdelirium 0.1.0
commit: <hash hexadecimal ou unknown>
schema: agent-projection-v1
```

O commit é o `HEAD` incorporado durante o build e o build script acompanha mudanças no `HEAD` e em sua referência Git. Ele identifica o commit-base, mas não afirma que a árvore de trabalho estava limpa. O schema publicado é somente o contrato visível ao agente; schemas administrativos não são enumerados.

## Revisão autorizada

A decisão de revisão usa uma operação distinta, `review.decide.assigned`. O contexto autorizado fornece projeto, identidade do revisor e uma única submissão atribuída; o corpo aceita somente `accepted` ou `rejected` e uma justificativa sanitizada. A primeira decisão é gravada de forma imutável junto com `review_recorded`; retry idêntico é deduplicado e uma decisão conflitante é recusada.

Registrar a revisão não altera `implemented`, `task_reviewed`, `review_status`, `grade`, `active_plan`, tarefa atual ou `state_revision`. Uma eventual transição continuará exigindo política e capability próprias do serviço. A fronteira autenticada não torna `review` um comando do executável comum: invocá-lo sem o cliente limitado e uma capability estreita continua retornando `AD-AUTH-0001`.

Os erros próprios agora passam pelo mesmo roteador público. A ajuda contém
exatamente as quatro formas públicas mostradas acima. Qualquer operação fora
dessa allowlist, seja administrativa conhecida ou um nome inventado, retorna a
mesma negação `AD-AUTH-0001` e o mesmo exit `77`. Somente uma forma pertencente
à gramática pública, porém incompleta ou malformada, retorna `AD-USAGE-0001` e
exit `64`. Nenhuma dessas respostas ecoa a operação, argumento, ID ou caminho
fornecido.

## Limite de não divulgação do release

`make release-nondisclosure` remapeia o checkout, o sysroot e o Cargo home para
prefixos lógicos durante a compilação e audita o executável com `strings`. O
gate rejeita paths de usuários Unix/macOS/Windows e os paths físicos reservados
do estado e socket do serviço. O mesmo gate faz parte de `make ci`.

Isso é defesa em profundidade, não uma autorização baseada em obscuridade. A
CLI madura e o encaminhador antigo ainda estão compilados exclusivamente como
rollback interno; por isso nomes genéricos da implementação antiga podem
continuar recuperáveis por engenharia reversa. Eles não possuem rota a partir
do perfil agente e não revelam a localização do plano protegido. A remoção do
código legado e a auditoria ELF abrangente continuam nas tarefas posteriores.

Uma forma pública malformada como `--version extra` permanece em stderr com
`AD-USAGE-0001`/exit `64`. `operator --version` não transforma o perfil em
agente e permanece em stderr com `AD-AUTH-0001`/exit `77`. Da mesma forma,
`antdelirium <rustc-qualificado> --version` continua sendo uma invocação de
wrapper e não intercepta a consulta do compilador como versão do AntDelirium.

## Modo de serviço protegido

O mesmo executável reconhece `antdelirium service`, mas o argumento seleciona
apenas o modo: ele não concede autoridade. Antes de acessar qualquer recurso, o
bootstrap compara o UID efetivo reportado pelo kernel com o UID da conta de
sistema dedicada `antdelirium`. A correspondência precisa ser única e não-root;
conta ausente, leitura ambígua, root ou qualquer outro usuário falham fechados
com `AD-AUTH-0001`/exit `77` sem revelar UID, conta ou configuração.

Somente essa verificação produz o token Rust privado que permite entrar no
handler do serviço. Antes de abrir listeners ou storage, o bootstrap exige uma
segunda prova interna de hardening: recusa `TracerPid` diferente de zero, aplica
`PR_SET_NO_NEW_PRIVS`, torna o processo não-dumpable, zera os limites soft e
hard de core dump, define `umask 0077` e usa `close_range(CLOEXEC)` sobre todo
descriptor herdado não padrão. A fronteira de canais exige o token produzido
por essa etapa, impedindo que uma mudança posterior inicie o serviço antes do
hardening. Depois dela, o serviço aceita somente o `STATE_DIRECTORY`
único e absoluto fornecido pela unidade. O diretório precisa existir, pertencer
ao UID verificado, ser diretório regular e não ser symlink. O runtime aplica
`0700` à raiz e aos diretórios privados, inicializa um registry SQLite separado
e recupera cada banco canônico já registrado com `storage-v4`, WAL, `FULL`,
foreign keys, `trusted_schema=OFF`, `quick_check` e checkpoint.

`storage-v4` preserva as migrations anteriores, incluindo grants de capability,
epochs por sujeito/ator, revogações e recibos de request, e acrescenta o journal
de segurança do serviço. O segredo bruto nunca é persistido: somente seu
SHA-256 entra no banco. O bootstrap da conexão e a migration usam retry limitado
apenas para contenção transitória de SQLite, inclusive quando inicializadores
disputam a criação do banco; corrupção, schema inválido e qualquer outro erro
continuam falhando imediatamente.

A estrutura interna usa apenas nomes relativos à raiz recebida:

```text
<state-directory>/
  registry/state/state.db
  projects/<project-key>/state/state.db
```

O banco do registry, bancos canônicos, journals, WAL e SHM são `0600` e precisam
pertencer ao UID verificado; sidecars que sejam symlink ou não sejam arquivos
regulares são recusados antes da abertura SQLite. Cada `project-key` precisa
corresponder ao token do `project_id` opaco e não pode conter traversal. Um projeto novo é
materializado de forma idempotente; uma interrupção antes do banco pode retomar
somente um layout vazio conhecido. Banco registrado ausente ou corrompido,
symlink, owner divergente e conteúdo inesperado são preservados e causam falha
fechada, sem criação silenciosa de estado vazio.

Os canais da T4.3 são agora o primeiro chamador de produção do bootstrap do
storage, mas não chamam `materialize_project`: materialização continua sendo
uma operação administrativa, nunca consequência de bytes enviados pelo
cliente. O serviço exige exatamente dois
descriptors ativados pelo systemd, chamados `agent` e `admin`. Ambos precisam
ser listeners `AF_UNIX`/`SOCK_STREAM` distintos e já em escuta; descriptor
ausente, extra, duplicado, fechado, de outro tipo ou entregue para outro PID
falha com `AD-CONFIG-0001`/exit `78`. Não existe bind alternativo nem endpoint
físico compilado no executável.

O listener limitado e o administrativo possuem tipos e magic bytes separados,
respectivamente `ADL1` e `ADA1`; nenhum campo do payload pode trocar de canal.
O endpoint limitado pertence ao grupo `antdelirium-client`, enquanto o
administrativo fica em diretório separado, root-owned, para o grupo
`antdelirium-admin`. Revisor continua no canal limitado com uma capability
estreita de revisão atribuída; ele não é operador e não recebe a superfície
administrativa.

Ao aceitar uma conexão, o daemon aplica `FD_CLOEXEC`, captura imediatamente
`SO_PEERCRED` do kernel e gera um nonce de 32 bytes com `getrandom`. UID e canal
confiáveis, assim como o nonce esperado, são mantidos fora do request do
cliente. A autorização exige uma capability válida para a combinação exata de
ator, operação, canal, UID e escopo opaco de projeto/workspace/tarefa, além da
janela temporal medida pelo relógio do serviço, epoch e ausência de revogação.
O cliente não fornece `now_unix`. Segredo incorreto, mismatch ou
grant inexistente convergem para o mesmo resultado público de negação; o tipo
de request e o contexto da conexão não implementam `Debug`, evitando imprimir
token ou nonce por acidente.

O recibo `(grant_id, request_id)` distingue uma primeira autorização de replay
idêntico pelo fingerprint e recusa a reutilização conflitante. Uma negação de
autorização é gravada na mesma transação que produz `Denied`, somente com UID e
canal obtidos da fronteira confiável, IDs de evento/correlação e horário do
serviço. Como o request não foi autenticado, o journal não copia ator, operação,
escopo, grant, request, token, nonce ou payload fornecido pelo cliente.
`project.bootstrap.observe` continua sendo a única operação provisória sem
capability pré-existente e não pode ser criada como um grant comum.

O resolver da T4.5 aceita somente o token `AuthorizedCapability` produzido pelo
motor de autenticação. Seus campos não podem ser construídos pelo cliente; a
entrada contém apenas IDs opacos e nunca aceita cwd, nome, `project-key` ou
path. Antes de tocar o diretório protegido, o serviço revalida no registry o
grant, receipt, prazo, epoch e revogação, procura uma correspondência exata de
`project_id`, deriva internamente o diretório pelo key privado persistido e
confirma owner, banco, projeto único, workspace ativo e tarefa pertencente ao
projeto. Capability administrativa sem projeto não seleciona um projeto
padrão. O handle resultante contém apenas o banco interno e os IDs opacos;
falhas de lookup e storage atravessam a fronteira como a mesma negação opaca.
Essas rejeições posteriores à autenticação são registradas no registry como
`request_rejected`, usando ator, operação e IDs opacos somente do token
`AuthorizedCapability`. A tabela não possui coluna livre de payload nem campos
de path, cwd, `project-key`, segredo, token, nonce, grant ou request.

Submissões e revisões aceitas continuam usando o journal `events` do banco do
projeto: a mutação e seu evento são persistidos na mesma transação, com sequência
manual sem lacunas causadas por deduplicação. Strings e payloads do evento são
validados antes do insert; caminhos físicos diretos ou codificados em JSON
fazem toda a operação falhar e sofrer rollback. Nenhuma resposta pública recebe
path do banco ou da raiz protegida.

O parser de requests ainda não foi ativado nos listeners: depois de capturar o
peer e o nonce, a conexão continua sendo fechada antes de ler payload ou chamar
handler. Portanto o resolver existe somente dentro do serviço e ainda não há
operação remota, materialização de projeto ou diagnóstico privado alcançável.
A ligação aos handlers permanece para uma tarefa posterior; os journals da
T4.6 já existem internamente, mas não ativam uma rota parcial. O isolamento
de descriptors da T4.7 já impede sua herança por um futuro `exec`; o serviço
atual não cria processos filhos. A T4.8 acrescenta um probe adversarial externo
que executa o cliente sem capabilities em namespaces descartáveis, mantém apenas
o projeto visível e comprova falha de resolução, `stat`, `access`, `readlink`,
enumeração e abertura dos roots protegidos conhecidos. A repetição contra a
unidade e o serviço realmente instalados permanece na T9.4.

Os templates não instalados
[`packaging/systemd/antdelirium.service.in`](../packaging/systemd/antdelirium.service.in)
[`packaging/systemd/antdelirium-agent.socket.in`](../packaging/systemd/antdelirium-agent.socket.in)
e
[`packaging/systemd/antdelirium-admin.socket.in`](../packaging/systemd/antdelirium-admin.socket.in)
usam unidade de sistema, o mesmo binário físico, `Accept=no` e dois diretórios
de runtime independentes. O serviço exige as duas unidades socket e inclui o
baseline de hardening de `authority-v1`. Esta implementação não criou conta,
grupo, diretório protegido, socket ou unidade ativa e não deve ser iniciada
como serviço `systemd --user`.

Definir `STATE_DIRECTORY` manualmente como usuário do agente não cria storage:
o gate de UID ocorre antes da leitura dessa variável. O path físico não é
compilado no cliente nem aparece na ajuda, versão ou erro público.

## O que não fazer nesta fase

- não copiar ou renomear o executável para `plugins/<project>/bin/`;
- não criar novos binários `delirium_cli`, `ac_ant_delirium` ou `guard`;
- não alterar `$CARGO_HOME/config.toml` manualmente;
- não tratar o build hook legado como a futura integração `rustc-workspace-wrapper`.

O código-fonte do encaminhador `guard` antigo e a CLI madura permanecem preservados e compilados apenas como modos internos de rollback, sem rota pública. O dispatcher classifica wrapper, agente, revisor, operador e serviço antes de qualquer parser. Pedido de perfil não é autorização: wrapper permanece em `AD-SERVICE-0001`; revisor e operador permanecem em `AD-AUTH-0001`; e serviço só atravessa sua fronteira interna quando já está sob a conta dedicada, sem cair no parser legado.

## Estado do build hook deste repositório

O `build.rs` deste próprio checkout continua advisory durante a transição. A mensagem sobre a ausência do binário externo legado é esperada e não instala, copia ou descobre automaticamente uma nova integração.

A instalação em `$CARGO_HOME/bin/antdelirium`, a mesclagem segura da configuração do Cargo e a desinstalação pertencem à Onda 7 e só devem ser executadas depois dos gates anteriores.
