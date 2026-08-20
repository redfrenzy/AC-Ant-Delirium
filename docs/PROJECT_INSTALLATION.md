# AntDelirium: binário único e superfície limitada do agente

Este documento descreve o estado implementado até a T6.6. O projeto declara um único executável autoritativo chamado `antdelirium`; a instalação global e a configuração persistente do Cargo ainda pertencem às ondas posteriores.

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

## Materialização local de `guard/`

Antes de um `ProjectProjectionWriter` poder ser construído, a fronteira
`lifecycle-v1` materializa `<project-root>/guard/` de forma idempotente. Ela
exige uma raiz de projeto regular, rejeita symlinks em `guard/` e
`guard/.gitignore` e nunca altera `.gitignore` raiz, configuração Git ou index.

Em projeto Git, uma projeção já rastreada bloqueia o bootstrap. Se regras
existentes já ignoram os dois JSONs, elas são preservadas. Caso contrário, e
somente se `guard/.gitignore` ainda não existir, o bootstrap cria atomicamente o
arquivo autocontido mínimo:

```gitignore
/to-do_list.json
/governance_error.json
/.gitignore
```

Um `guard/.gitignore` preexistente pertence ao projeto e nunca é sobrescrito,
mesmo quando não ignora as projeções. Em projeto sem Git, cria-se somente a
pasta vazia. A T5.1 não gera os JSONs; as fronteiras internas de projeção de
tarefa e de erros foram implementadas, respectivamente, nas T5.2 e T5.3.

## Projeção canônica da tarefa atual

`AtomicProjectProjectionWriter::refresh_current_task_projection` gera
`guard/to-do_list.json` sem receber plano, filename, step legado ou tarefa
escolhida pelo agente. A operação consulta no banco somente o `current_task_id`
do projeto e exige que a tarefa pertença ao plano ativo. A revisão da linha e o
estado do ciclo de vida precisam corresponder à ação permitida.

Objetivo, ação, critérios de aceitação e paths relevantes vêm do payload
privado versionado `canonical-task-projection-v1` da própria tarefa. Esse
payload usa framing binário limitado e rejeita versão desconhecida, truncamento,
dados excedentes, UTF-8 inválido, path absoluto, traversal, duplicatas e limites
do contrato antes de qualquer arquivo ou evento. O plano e as outras tarefas
não são lidos para montar a projeção.

O serviço deriva geração, fingerprint SHA-256 e IDs de auditoria; o writer
confirma novamente, dentro da transação, o ID e a revisão da tarefa atual antes
da escrita. Sem tarefa atual, o documento contém `task: null` e fila opaca, sem
procurar uma tarefa futura. O artefato continua sendo uma projeção descartável:
ele nunca é usado para atualizar o banco canônico.

Esta etapa prova geração e materialização pela fronteira interna tipada. O
daemon instalado não recebe mount do projeto e o wrapper ainda não possui o
transporte `projection.refresh`; separar a resposta sanitizada do serviço da
escrita local do cliente permanece na T6.4. Portanto a T5.2 não instala socket,
wrapper ou configuração Cargo e não apresenta essa integração futura como já
ativa.

## Projeção canônica das violações corrigíveis

`GovernanceLogs::write_artifacts` continua sendo o único adaptador do conjunto
de violações produzido pela auditoria. Para a projeção pública, ele seleciona
somente violações em paths corrigíveis dentro do projeto e exclui as raízes
administrativas `.git/`, `guard/`, `plugins/`, `plan/`, `plans/`,
`ant_delirium_config/` e `AC_Ant-Delirium_props/`. A ausência de `quick_fix`
não descarta uma violação de código: a correção pode exigir julgamento, e a
referência pública da regra continua disponível.

Antes de gerar `guard/governance_error.json`, o serviço persiste no SQLite
protegido um snapshot binário limitado `canonical-governance-snapshot-v1`,
vinculado a projeto, workspace, revisão e fingerprint SHA-256. O refresh aceita
inclusive um snapshot vazio explícito, mas falha sem escrever arquivo ou evento
de projeção se o snapshot estiver ausente, adulterado, pertencer a outra revisão
ou não corresponder ao workspace ativo. O writer revalida essa vinculação na
mesma transação da materialização; uma projeção tipada construída diretamente,
sem snapshot canônico, é recusada.

O JSON contém somente ID opaco da violação, regra, resumo sanitizado, path
relativo, linha, correção pública opcional e referência pública da regra. Logs,
snippet, taxonomia detectada, DNA, comandos de instalação, plano e configuração
administrativa não atravessam essa fronteira. Editar ou apagar o JSON não muda o
snapshot nem o estado canônico.

Assim como na T5.2, esta etapa implementa e testa a fronteira interna. A entrega
serviço→wrapper por socket e a chamada automática durante o fluxo Cargo ainda
pertencem à T6.4; a T5.3 não instala binário, wrapper ou configuração global.

## Escrita atômica e projeções inalteradas

Os dois refreshes calculam o fingerprint sobre os bytes públicos versionados,
sem incluir horário de renderização ou uma geração candidata. Em governança, o
ID `vio_` também é derivado de forma determinística por SHA-256 do escopo e dos
campos públicos da violação; ele continua opaco, mas deixa de transformar duas
auditorias iguais em documentos artificialmente diferentes.

O primeiro conteúdo novo avança uma única geração, registra um único evento e
passa pelo writer sincronizado: temporário exclusivo no próprio `guard/`, modo
restrito, `write_all`, `flush`, `sync_all`, `rename` e sincronização do
diretório. Refreshes concorrentes com bytes iguais resolvem para o mesmo
fingerprint, reutilizam geração e sequência persistidas e não criam lacunas de
auditoria.

Antes de materializar, o writer abre sem seguir symlink e compara somente um
arquivo regular, com modo `0600`, aos bytes validados. Se forem idênticos, não
abre temporário, não executa `rename` e não altera inode ou timestamp. Ausência,
tipo/modo inesperado ou conteúdo diferente não entram nessa otimização e seguem
a rota atômica.

## Regeneração sem autoridade local

Excluir ou editar qualquer uma das projeções não altera tarefa, plano, snapshot,
revisão, geração ou auditoria canônicos. No refresh seguinte,
`to-do_list.json` é reconstruído da tarefa vinculada ao plano ativo no SQLite;
`governance_error.json` é reconstruído do snapshot binário protegido da mesma
revisão e workspace. Nenhum ID, checkbox, path, regra ou texto encontrado no
JSON local participa dessa decisão.

Arquivo ausente, bytes divergentes e modo diferente de `0600` são substituídos
pela rota atômica. Um symlink no path público é substituído como entrada de
diretório, sem ser seguido e sem alterar seu alvo. Um diretório inesperado no
lugar do JSON falha fechado e é preservado: o AntDelirium não executa remoção
recursiva de conteúdo desconhecido. Depois que o conflito é removido, o refresh
normal recria a projeção canônica.

## Fronteira de conteúdo de `guard/`

Os dois builders e o writer final aplicam a mesma política de não divulgação.
O código release compara fingerprints SHA-256 dos identificadores protegidos;
o dicionário legível fica somente no contrato e nos testes, sem acrescentar os
nomes administrativos à tabela de strings do executável.
Paths públicos precisam ser canônicos e relativos ao projeto, mas também não
podem começar por raízes administrativas como `plan/`, `plugins/`, `guard/` ou
diretórios de configuração do AntDelirium. Nomes conhecidos de orquestrador,
login administrativo, histórico, manifests, TOMLs internos e filas também são
recusados mesmo quando aparecem em outro subdiretório.

Texto livre não pode transportar identificadores de estado como os usados para
plano ativo, plano completo, implementação, revisão ou localização física do
workspace. A checagem do JSON serializado decodifica escapes, recusa chaves de
plano, listas de tarefas futuras, configuração, login e histórico, e falha antes
de iniciar auditoria ou escrita. O contrato versionado mantém
`additionalProperties: false`; a tarefa projetada continua singular e ligada à
tarefa canônica corrente.

A regra não bloqueia vocabulário comum do produto. Uma tarefa pode falar sobre
uma tela de login, histórico ou configuração e indicar, por exemplo,
`src/login/controller.rs`; o que permanece proibido são os nomes e caminhos da
infraestrutura administrativa do AntDelirium.

## Logs e mensagens normais

O runtime não escreve logs globais de debug ou erro em diretório temporário.
Falhas internas do agente usam somente os códigos públicos estáveis; diagnóstico
detalhado pertence ao journal do serviço protegido quando essa integração
estiver concluída. Não existe fallback para arquivo global quando o serviço ou
um writer falha.

As respostas bem-sucedidas do CLI são tipadas. Ajuda e versão são respostas
informativas separadas; toda operação normal de tarefa, regra, próxima ação ou
submissão só pode produzir uma das duas notificações fechadas:

```text
antdelirium[AD-TASK]: leia guard/to-do_list.json
antdelirium[AD-GOV]: leia guard/governance_error.json
```

O gate de release rejeita caminhos físicos protegidos e os antigos nomes
previsíveis de log caso sejam reintroduzidos no executável.

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

## Prova dinâmica de não divulgação do cliente

Em Linux com `strace` e permissão de `ptrace`, a fronteira de syscalls pode ser
reproduzida sem `sudo`:

```bash
make syscall-client-nondisclosure
```

O alvo segue todos os descendentes de um `cargo check` isolado, confirma o
wrapper e o filho audit-only e falha se qualquer processo abrir, testar,
resolver ou enumerar as raízes sentinelas de estado protegido. Exit `77`
significa que o ambiente negou o tracing ou não ofereceu a ferramenta; não é
tratado como prova positiva. O método, os sentinelas e o resultado observado
estão em
[`VALIDACAO-SYSCALLS-WRAPPER-T6.8.md`](./VALIDACAO-SYSCALLS-WRAPPER-T6.8.md).

## Protocolo do wrapper e proteção de recursão

A classificação do wrapper agora pertence a um módulo próprio e ocorre antes
de qualquer parser de agente ou legado. A forma só é reconhecida quando o
primeiro argumento é um path qualificado terminado em `rustc` ou `rustc.exe`,
há pelo menos um marcador típico de compilação ou consulta do compilador e não
existe subcomando AntDelirium na posição de comando. Um nome `rustc` solto, um
path sem marcador ou um comando ambíguo não ativa o perfil wrapper.

Essa detecção é deliberadamente lexical: ela não abre, resolve, faz `stat` ou
canonicaliza o path recebido. Assim, uma tentativa de classificação não vira
um oráculo de filesystem para um path fornecido pelo agente.

`ANTDELIRIUM_WRAPPER_DEPTH` possui somente três estados válidos para uma forma
wrapper já reconhecida:

- variável ausente ou valor exato `0`: rota inicial `Governed`;
- valor exato `1`: rota recursiva `DelegationOnly`, que não poderá reentrar na
  governança;
- qualquer outro valor, inclusive vazio ou maior que `1`: rejeição fechada com
  `AD-CONFIG-0001`/exit `78` antes de qualquer handler.

A variável isolada nunca transforma um comando de agente ou operador em
wrapper. Ela também não é autorização: no modelo de mesmo usuário, uma pessoa
pode tentar defini-la para evitar a governança, e a garantia contra esse bypass
continua dependendo do gate externo previsto nas ondas posteriores.

A T6.2 liga ambas as rotas ao compilador real. No target Unix autoritativo, o
wrapper constrói `Command` diretamente com `OsString`, mantém a ordem integral
dos argumentos, força a próxima profundidade para `1`, herda explicitamente
stdin/stdout/stderr e usa `exec` para substituir o próprio processo. Assim o
PID passa a ser o `rustc`, e exit status ou sinal pertencem diretamente ao
compilador, sem tradução por um supervisor intermediário. O fallback compilado
para plataformas não Unix herda os mesmos streams e status normal. A T6.6
validou o wrapper host no Linux também ao produzir código para WASM; ela não
afirma equivalência de sinais em um host não Unix.

Falha `NotFound` anterior ao `exec` produz somente `AD-TOOLCHAIN-0002`/exit
`127`; as demais falhas de execução produzem somente
`AD-TOOLCHAIN-0001`/exit `126`. Nenhuma delas inclui o path ou o erro bruto do
sistema. Na T6.2 a rota `Governed` ainda apenas delegava; a T6.4 acrescentou a
negociação limitada e a T6.5 ligou as requisições de governança em um filho
audit-only, preservando a precedência do resultado do compilador. Nenhuma
configuração Cargo foi instalada nesta etapa.

### Classificação do trabalho Cargo

A T6.3 classifica cada primeira entrada `Governed` antes de qualquer futura
operação de governança. A prioridade é conservadora:

1. `-vV`, versão, ajuda ou qualquer forma de `--print` é `Probe`, inclusive o
   probe complexo do Cargo que também contém `--crate-name ___`;
2. uma compilação exige `--crate-name <valor>` ou `--crate-name=<valor>` válido;
3. sem `CARGO_PRIMARY_PACKAGE`, uma compilação é `Dependency`;
4. com esse marcador, ela só é `PrimaryWorkspaceMember` quando
   `CARGO_MANIFEST_DIR` e `CARGO_MANIFEST_PATH` são absolutos, apontam para o
   mesmo `Cargo.toml` e `CARGO_CRATE_NAME` coincide com o argumento do `rustc`;
5. contexto ausente, relativo, divergente ou malformado é `Unclassified`.

Probe, dependência e forma não classificada delegam diretamente. Somente o
membro primário entra no hook das T6.4/T6.5; ele delega imediatamente no pai e
somente o filho audit-only negocia o canal e executa as operações limitadas. A
rota recursiva `DelegationOnly` nem consulta o contexto Cargo. Nenhuma variável
de ambiente concede autorização: esses sinais servem apenas para reduzir
escopo, e toda operação protegida exige canal e capability do serviço.

O contrato acompanha a documentação oficial de [variáveis fornecidas pelo
Cargo](https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates)
e de [`rustc-workspace-wrapper`](https://doc.rust-lang.org/cargo/reference/config.html#buildrustc-workspace-wrapper).
Além dos testes sintéticos, a própria compilação de teste confirma os quatro
campos realmente injetados pelo Cargo. Um `cargo check` nativo com o wrapper
fornecido somente por `--config` também passou; nenhuma configuração global foi
gravada.

A T6.6 acrescenta uma matriz Cargo autocontida em
[`tests/fixtures/t66-cargo-matrix`](../tests/fixtures/t66-cargo-matrix). Ela usa
um workspace virtual bloqueado por `Cargo.lock`, com biblioteca, binário, build
script, proc-macro, teste de integração e exemplo, e fornece o binário de teste
somente por `RUSTC_WORKSPACE_WRAPPER`. A matriz prova a participação efetiva do
wrapper por uma profundidade deliberadamente inválida e depois executa, com a
profundidade normal, `check -j 4`, um segundo check em cache, `test`, `run`, o
exemplo e `build -j 4` para `wasm32-unknown-unknown`. A saída do binário nativo
direto e encapsulado é idêntica. Nenhum arquivo de configuração Cargo é escrito.

Essa matriz remove deliberadamente endpoint e capability do ambiente: ela
prova transparência, targets e ausência de deadlock quando o serviço limitado
está indisponível. O transporte contra serviço instalado e o isolamento do
state root continuam nos gates adversariais posteriores.

### Orçamento de overhead

A T6.7 transforma a comparação direto/wrapper em um gate reproduzível. A
[validação de overhead](./VALIDACAO-OVERHEAD-WRAPPER-T6.7.md) usa medianas,
targets isolados e ordem alternada. Os tetos adicionais são `25 ms` por entrada
do wrapper, `2 s` para check limpo, `1 s` para incremental e `250 ms` para
no-op. Na amostra registrada, os deltas foram respectivamente `2,220 ms`,
`48,752 ms`, `5,214 ms` e `1,139 ms`, com exit, stdout e stderr equivalentes.

Esses limites cobrem o wrapper com endpoint ausente. O no-op pode não chamar o
wrapper, e a rota com serviço ativo continua na matriz integral posterior.

### Negociação do protocolo limitado

A T6.4 liga exclusivamente o hook de `PrimaryWorkspaceMember` ao endpoint
limitado informado pela instalação em `ANTDELIRIUM_LIMITED_SOCKET`. O valor
precisa ser um pathname absoluto, mas não concede autoridade: o endpoint do
cliente pode ser conhecido e não é a localização do storage. Não há endpoint
padrão compilado, lookup de plugin, descoberta de plano ou fallback por cwd.
Se a variável estiver ausente, for inválida ou o handshake falhar no rollout
audit-only, o wrapper permanece silencioso e executa o `rustc` normalmente.

O cliente implementa somente frames `ADL1`, comprimento `u32` big-endian,
JSON tipado com campos desconhecidos recusados, limite de 4 MiB e wire
`adp/1`. Nonces têm 32 bytes de entropia do kernel e representação base64url
sem padding. O handshake usa `poll` com deadline total de 500 ms; a espera pela
resposta de operação, executada somente no filho audit-only, é limitada a cinco
segundos. O socket cliente recebe `FD_CLOEXEC` antes de qualquer troca. O serviço captura
`SO_PEERCRED` e gera o nonce antes de ler o `client_hello`, seleciona somente
uma versão suportada e devolve `server_hello` no mesmo magic. Magic cruzado,
frame vazio ou grande demais, JSON inválido, campo extra, versão incompatível
e nonce malformado encerram a conexão.

A T6.4 concluiu apenas essa negociação. A T6.5 acrescenta os três envelopes
tipados `project.bootstrap.observe`, `compile.observe` e `projection.refresh`.
O bootstrap envia somente um fingerprint SHA-256 e um slug descritivo: nenhum
path, manifesto ou plano atravessa o socket. O registry `storage-v6` deduplica o
registro provisório por UID do peer e fingerprint, aplica quota de 64 registros
provisórios por UID e emite duas capabilities de cinco minutos, uma para cada
operação posterior. O segredo bruto não entra no SQLite.

Uma capability previamente entregue pode chegar pelo descriptor indicado em
`ANTDELIRIUM_LIMITED_CAPABILITY_FD`, no contrato JSON fechado
`limited-capability-v1`. O wrapper limita a leitura a 4 KiB, valida IDs e tokens,
marca e fecha o descriptor. O processo que se transforma no `rustc` também
remove as duas variáveis de integração e nunca herda o descriptor. Na ausência
da capability, o próprio bootstrap provisório devolve o mesmo bundle estreito
para uso somente no ciclo audit-only corrente.

O fingerprint de compilação inclui argv, manifesto do membro, manifesto do
workspace e digest da fonte Rust regular de até 32 MiB. O serviço usa o recibo
de capability e o fingerprint do evento para tornar retries e compilações
idênticas idempotentes. `projection.refresh` devolve somente os dois documentos
`agent-projection-v1`; a escrita local revalida contrato e política pública,
materializa `guard/` pelo lifecycle já existente e usa replace atômico com
deduplicação de bytes. O protocolo administrativo continua não representável
no cliente.

Para não segurar o caminho crítico, todo esse trabalho ocorre em um filho
audit-only criado depois da classificação. O processo pai fecha o descriptor e
executa imediatamente o compilador real com `exec`; timeout, indisponibilidade,
negação ou violação detectada no filho não muda stdout, stderr, exit ou sinal do
`rustc` nesta fase.

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

Desde a T6.5, o listener limitado aceita, depois do hello, exatamente uma das
três operações de wrapper implementadas. Bootstrap sem capability cria somente
estado provisório vazio; compilação e projeção precisam passar por peer,
capability, nonce, request fingerprint, receipt e resolver opaco antes de abrir
o banco derivado pelo registry. As outras operações limitadas continuam
fechadas e o listener administrativo continua sem leitura. Os journals da T4.6
registram negações e rejeições sem payload ou localização física. O isolamento
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

O código-fonte do encaminhador `guard` antigo e a CLI madura permanecem preservados e compilados apenas como modos internos de rollback, sem rota pública. O dispatcher classifica wrapper, agente, revisor, operador e serviço antes de qualquer parser. Pedido de perfil não é autorização: somente a rota wrapper `Governed` de membro primário pode negociar `ADL1`, enquanto `DelegationOnly` encaminha diretamente ao compilador; nenhuma delas abre superfície administrativa. Revisor e operador permanecem em `AD-AUTH-0001`; e serviço só atravessa sua fronteira interna quando já está sob a conta dedicada, sem cair no parser legado.

## Estado do build hook deste repositório

O `build.rs` deste próprio checkout continua advisory durante a transição. A mensagem sobre a ausência do binário externo legado é esperada e não instala, copia ou descobre automaticamente uma nova integração.

## Primitiva de instalação do binário — T7.1

A T7.1 implementou a primeira etapa interna do instalador:

```bash
cargo build --release --locked --target x86_64-unknown-linux-gnu \
  --bin antdelirium
target/x86_64-unknown-linux-gnu/release/antdelirium install
```

Essa primitiva copia atomicamente o próprio release para
`$CARGO_HOME/bin/antdelirium`. Se `CARGO_HOME` estiver ausente, o destino é
`$HOME/.cargo`. Ambos precisam resultar em um path absoluto pertencente ao UID
efetivo não-root. O instalador recusa diretório ou destino symlink, fonte não
executável, ownership divergente e um `antdelirium` preexistente com bytes ou
modo diferentes. Uma repetição byte a byte idêntica é no-op.

A cópia usa um arquivo temporário no próprio `bin/`, sincroniza conteúdo e
diretório e publica o destino por hard link sem overwrite. O resultado final
tem modo `0755`. A primitiva não usa `sudo`, não substitui nem renomeia
`$CARGO_HOME/bin/cargo`. Builds debug recusam o comando administrativo; isso
evita instalar por engano o executável de `cargo test` ou `cargo run` sem
`--release`.

Os testes isolados da T7.1 continuam provando que a cópia do binário, por si só,
não altera `config.toml`. A T7.3 passou a compor essa primitiva com a transação
recuperável descrita abaixo. O argumento `install` seleciona a operação, mas não
concede autoridade: as permissões/sandbox externos ainda decidem se o Cargo home
real é gravável.

## Transação recuperável de configuração — T7.2

A camada interna de configuração agora lê `config.toml` com `toml_edit`, aplica
uma mutação tipada e reserializa o documento preservando comentários, ordem,
formatação e chaves desconhecidas. A dependência está fixada em `0.25.12` e
possui MSRV 1.85; o grafo completo foi recompilado com o MSRV 1.88 declarado
pelo AntDelirium.

Antes de qualquer alteração real, a transação cria sob o Cargo home um diretório
privado `.antdelirium-install/` com modo `0700`. Ele contém:

- `config.toml.original`, com os bytes anteriores exatos e modo `0600`;
- `config-backup-v1.json`, com presença anterior, modo original e hashes SHA-256
  dos bytes anterior e planejado, sem armazenar paths.

Se `config.toml` não existia, o payload é vazio e o recibo registra
explicitamente `original_present=false`; assim a desinstalação futura poderá
distinguir ausência de um arquivo originalmente vazio. Configuração nova usa
modo `0600`; configuração existente preserva seu modo.

O replace é atômico e revalida inode, modo e bytes imediatamente antes da
publicação. Symlink, TOML inválido, arquivo acima de 4 MiB, backup preexistente,
alteração concorrente ou o arquivo legado `$CARGO_HOME/config` causam recusa sem
sobrescrever configuração ou backup. Uma mutação que não muda bytes preserva o
inode e não cria backup.

A T7.3 conecta essa transação ao comando `install` através de uma mutação fechada,
descrita na próxima seção.

## Ativação exclusiva do wrapper de workspace — T7.3

O comando release `antdelirium install` agora executa um único fluxo: resolve o
Cargo home, faz um preflight somente leitura, instala ou reconhece o binário em
`bin/antdelirium` e mescla exclusivamente:

```toml
[build]
rustc-workspace-wrapper = "<CARGO_HOME>/bin/antdelirium"
```

O mutador não escreve `build.rustc`, `build.rustc-wrapper`, `target`, `runner` ou
qualquer outra chave. Um `rustc-workspace-wrapper` já igual ao path esperado é
um no-op e não cria um backup para reivindicar configuração escrita por outra
pessoa. Valor diferente, tipo TOML diferente de string, `build` malformado ou a
presença de qualquer `build.rustc-wrapper` causam conflito antes da instalação.
Isso inclui caches como `sccache`: composição automática entre wrappers ainda
não possui política aprovada e não é inferida pelo instalador.

Não existe `--force` nem troca automática do wrapper. A decisão explícita é do
operador, que deve preservar, remover ou reconfigurar conscientemente a
integração conflitante e depois repetir `install`. O preflight também recusa
backup preexistente quando seria necessária uma nova alteração. A transação
repete toda a validação depois da cópia do binário; se houver uma corrida nesse
intervalo, a configuração permanece sem overwrite e o binário inativo pode
permanecer instalado com segurança.

Os testes usam somente Cargo homes descartáveis sob `target/`. Eles cobrem
ativação, preservação de `rustc`, `jobs`, `runner` e proxy `cargo`, idempotência,
configuração idêntica não reivindicada, `sccache`, wrapper divergente, tipos
inválidos e preflight sem cópia do binário em conflito. Nenhum Cargo home real é
modificado pelos testes.

Um probe adicional do ELF release executou `install` duas vezes no mesmo Cargo
home descartável, confirmou os modos `0755` do binário, `0600` da configuração,
`0700` da raiz de backup e `0600` de seus dois arquivos. Em seguida, um projeto
novo sem dependências passou `cargo check` usando somente essa configuração. O
mesmo check em target separado com profundidade deliberadamente inválida foi
rejeitado pelo wrapper, provando que o Cargo carregou a integração instalada.

## Diagnóstico administrativo sem divulgação — T7.4

O release aceita `antdelirium doctor` como operação administrativa read-only.
Ela permanece ausente da ajuda e da API do agente. O relatório textual possui
contrato `doctor v1` e publica somente versões e estados fechados; nunca imprime
paths, valores de ambiente, nomes dos arquivos locais encontrados ou localização
do serviço.

O diagnóstico verifica:

- Cargo 1.50 ou posterior, piso do protocolo `rustc-workspace-wrapper`;
- rustc 1.88 ou posterior, MSRV declarado pelo pacote;
- bytes e modo do binário instalado em relação ao executável em execução;
- wrapper global ativo, backup íntegro, hash atual e modo pertencente à instalação;
- presença dos seis overrides `RUSTC*`/`CARGO_BUILD_RUSTC*`, inclusive valor vazio;
- `.cargo/config` e `.cargo/config.toml` desde o cwd até a raiz, sem seguir
  symlinks e distinguindo configuração comum de override do compilador;
- endpoint limitado disponível e serviço ativo, ou ambos os sockets systemd
  ativos para socket activation.

Uma configuração idêntica que já existia antes da instalação aparece como
`active-unclaimed`: ela funciona, mas não é considerada pertencente ao
AntDelirium e o futuro `uninstall` não poderá removê-la. Alteração posterior de
bytes ou modo do arquivo registrado aparece como conflito.

O `doctor` consulta `systemctl is-active` sem ativar unidades e apenas verifica o
tipo do endpoint configurado; não conecta ao socket. Configuração local comum é
informativa, enquanto `include`, `build.rustc`, `build.rustc-wrapper` ou
`build.rustc-workspace-wrapper` local produz `override-detected`. Arquivos
ambíguos, TOML inválido, entrada symlink ou mudança concorrente produzem
`invalid`, sempre sem revelar onde foram encontrados.

Variáveis de ambiente têm precedência sobre TOML. `--config`, por sua vez, tem
precedência ainda maior e só existe na execução específica do Cargo; por isso o
relatório declara `command_line_config=per-invocation-unobservable` em vez de
prometer uma detecção impossível antecipadamente. CI ou supervisor continua
necessário para detectar esse bypass durante o comando real.

O processo termina com exit `0` apenas quando versões, instalação, configuração
pertencente, ausência de overrides e serviço estão prontos. Qualquer atenção
termina com exit `78`, depois do relatório sanitizado. O comando não modifica
configuração, instalação, serviço ou projeto.

O probe do ELF release em Cargo home descartável confirmou Cargo/rustc 1.97.1,
binário `ready`, configuração `active-owned`, uma configuração local comum sem
override e serviço `unavailable`, estado esperado antes da instalação systemd;
o resultado foi `attention`/exit `78`. Uma segunda execução com override vazio
mudou somente a contagem sanitizada de zero para um, sem publicar nome, valor ou
path.

As fixtures Cargo T7.1, T7.2 e T7.4 agora combinam relógio, PID e sequência no
nome sob `target/`. A mesma seleção de testes passou duas vezes consecutivas sem
limpeza, portanto os testes não dependem mais de apagar artefatos anteriores.
