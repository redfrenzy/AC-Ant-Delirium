# Plano da API oficial de geração de tarefas e migração — Onda 11

## 1. Objetivo

Criar um fluxo oficial e genérico para transformar uma origem revisada em
tarefas canônicas e, quando necessário, associar um candidato migrado à
identidade real observada pelo Cargo.

O fluxo não pode depender do nome AutoBrowser, do primeiro projeto no banco, do
primeiro workspace, de paths fornecidos pelo agente ou de edição direta das
projeções públicas.

## 2. Fronteiras de autoridade

### Wrapper e `cargo check`

Continuam responsáveis somente por:

- descobrir fingerprints do projeto e workspace atuais;
- registrar ou reutilizar a identidade exata;
- observar a compilação e produzir o backup privado;
- atualizar as duas projeções públicas.

Compilar nunca gera, importa, aprova, associa ou ativa tarefas administrativas.

### API limitada do agente

Permanece com as quatro ações existentes: tarefa atual, regra relacionada,
próxima ação e submissão de evidência. Nenhuma operação da Onda 11 aparece na
ajuda ou no protocolo limitado.

### API administrativa

Geração, inspeção, attestation, approval, instalação e associação usam o canal
administrativo, capabilities escopadas e descritores somente leitura. O serviço
recebe IDs opacos e bytes; nunca recebe `--project-root`, path de plano ou
localização da origem.

## 3. Dois fluxos oficiais

### 3.1 Importação somente de tarefas

Este é o caminho preferido quando se quer trazer objetivos e critérios, mas não
o histórico canônico antigo:

1. `cargo check` registra a identidade real do projeto;
2. o operador fornece uma origem por descritor ao gerador;
3. o gerador produz uma proposta imutável e atribui IDs opacos;
4. um verificador independente atesta o resumo e o digest;
5. o operador aprova a proposta exata;
6. `governance.task-package.install` instala atomicamente no projeto/workspace
   explicitamente selecionados;
7. outro `cargo check` apenas atualiza as projeções.

Checkboxes, grades ou `implemented=true` da origem são evidência histórica e
nunca viram conclusão canônica automaticamente.

### 3.2 Migração com histórico

Quando é necessário preservar múltiplos planos, histórico e bindings:

1. reutilizar stage, normalize, dry-run, conversion, attestation, approval e
   cutover existentes;
2. resolver todas as diferenças antes do approval;
3. associar a identidade observada somente por uma nova operação administrativa
   explícita;
4. preservar ou quarentenar o projeto provisório conforme política aprovada;
5. nunca modificar a seleção de tarefa produzida pela conversão.

## 4. Contratos versionados

### `task-generation-source-v1`

Entrada genérica e limitada contendo:

- `request_id` e formato de origem declarado;
- objetivos, critérios, ordem e paths relativos permitidos;
- referências lógicas à origem por digest, nunca path físico;
- nenhum estado canônico, capability, ID de banco ou decisão de conclusão.

### `task-package-proposal-v1`

Artefato protegido gerado pelo serviço contendo:

- `proposal_id`, digest da origem e digest do payload;
- planos e tarefas em ordem explícita, nunca `rowid`;
- IDs opacos atribuídos uma vez;
- seleção proposta de plano/tarefa inicial explicitamente representada;
- contagens, limites e avisos de conversão;
- estado `prepared`, sem autoridade para instalação.

### `task-package-attestation-v1` e `task-package-approval-v1`

Vinculam proposta, origem, projeto, workspace, revisão esperada e identidade do
verificador/operador. Reutilizar os padrões de digest, expiração, replay e
separação de papéis já aplicados à migração protegida.

### `governance-task-package-v2`

Evolução compatível do pacote atual para suportar, de forma limitada:

- um ou mais planos ordenados;
- tarefas com ordinal explícito;
- um único plano ativo e uma única tarefa corrente declarados;
- seleção inicial igual ao primeiro plano e à primeira tarefa na ordem aprovada,
  sem permitir que o instalador pule objetivos anteriores;
- instalação somente sobre projeto vazio na revisão esperada;
- vínculo obrigatório à proposta atestada e aprovada.

A versão v1 permanece aceita somente em sua operação atual. Não converter v1
implicitamente nem enfraquecer sua validação.

### `migration-identity-association-v1`

Pedido administrativo que contém exatamente:

- migration, cutover e approval IDs;
- projeto/workspace provisórios observados;
- projeto/workspace candidatos de destino;
- fingerprints esperados já presentes no registry;
- digests do candidato e do binding de cutover;
- política explícita para o estado provisório.

O pedido não contém prefixo, slug, path ou seleção por posição.

### `migration-identity-rollback-v1`

Pedido administrativo separado, vinculado ao ID e digest da associação, aos
quatro IDs de projeto/workspace e à revisão esperada do registry. A única
política aceita é `restore-provisional-preserve-candidate`: restaurar o vínculo
original mantendo os dois históricos. O recibo
`migration-identity-rollback-receipt-v1` identifica a reversão, o vínculo
restaurado e o candidato preservado, sem informar caminhos privados.

### Recibos

Cada operação produz recibo path-free, serializável e auditável com request ID,
resultado, IDs opacos, revisões anterior/posterior e digests. Retry idêntico
devolve o mesmo recibo; payload conflitante falha.

## 5. Operações administrativas

Preservar `operator task-package install` e acrescentar uma gramática fechada:

- `operator task-package prepare`;
- `operator task-package inspect`;
- `operator task-package attest`;
- `operator task-package approve`;
- `operator task-package install` para v1/v2;
- `operator migration identity-associate`;
- `operator migration identity-rollback`.

Entradas grandes chegam por `ANTDELIRIUM_*_FD` herdado, limitado, regular,
somente leitura e `CLOEXEC`. Argumentos de linha de comando nunca carregam
paths, JSON, tokens ou fingerprints.

### 5.1 Transporte implementado na T11.4

`prepare`, `inspect`, `attest` e `approve` usam o mesmo canal ADA1. O cliente
consome `ANTDELIRIUM_TASK_PACKAGE_INPUT_FD`, com arquivo regular somente leitura
na posição zero, e `ANTDELIRIUM_TASK_PACKAGE_OUTPUT_FD`, com arquivo regular
vazio, privado (0600), pertencente ao usuário e com um único link. Entrada,
saída e `ANTDELIRIUM_ADMIN_CAPABILITY_FD` devem ser descritores distintos.
A saída é validada antes do envio da operação; todos os descritores consumidos
recebem `CLOEXEC`. O limite de leitura é 4 MiB menos 64 KiB, e a proposta gerada
deve caber em 4 MiB menos 128 KiB antes de ser persistida.

O descritor de saída recebe a proposta e seu recibo em `prepare/inspect`, ou
somente o recibo em `attest/approve`. O terminal mostra IDs, digests, contagens
e estado, sem texto da proposta. O cliente confere o contrato, escopo, revisão,
operação e digest da resposta. Se a saída falhar depois do commit, repetir a
mesma entrada com uma saída válida recupera o recibo persistido.

As credenciais administrativas mantêm o envelope `admin-capability-v1`.
`prepare/approve` exigem operador com `StateAdmin`, `inspect` exige operador
com `DiagnosticPrivate` e `attest` exige CI com `AttestationCreate`; todos com
projeto/workspace exatos e sem escopo de tarefa. O gerenciador permite emitir
as novas credenciais de operador somente para o próprio UID autorizado.

Os campos `verifier_subject_sha256` e `operator_subject_sha256` precisam ser
iguais ao SHA-256 de `task-package-subject-v1`, seguido de um byte NUL e do UID
autenticado em quatro bytes big-endian. O serviço verifica esse vínculo; trocar
o JSON, o papel ou o grant não muda a identidade. Verificador e aprovador
precisam ter UIDs distintos. Os launchers da T11.8 deverão aplicar esse contrato.

Reenvios idênticos das quatro operações são aceitos somente após nova checagem
de autorização, expiração, revogação, escopo e revisão. Reutilizar o mesmo ID de
requisição com payload diferente é recusado. O `request_id` dos recibos de
decisão acompanha a requisição de origem da proposta; o envelope ADA1 identifica
cada troca de mensagens. A inspeção tem seu próprio `request_id`.

`source_sha256` é uma referência declarada à origem externa, a ser conferida
pelo verificador; o serviço não lê essa origem. O digest `payload_sha256` é
calculado sobre a proposta gerada e persistida. A instalação v1 continua em
`ANTDELIRIUM_TASK_PACKAGE_FD`; a instalação v2 usa o transporte descrito abaixo.

### 5.2 Instalação v2 implementada na T11.5

`operator task-package install` recebe v2 por
`ANTDELIRIUM_TASK_PACKAGE_INPUT_FD` e grava o recibo em
`ANTDELIRIUM_TASK_PACKAGE_OUTPUT_FD`, mantendo a entrada v1 separada no
descritor legado. Fornecer ambas as entradas é erro. O serviço exige CI com
`InstallationAdmin` limitado ao projeto/workspace exatos; a credencial usada
pelo operador para aprovar não permite instalar.

A transação compara o pacote completo com a proposta persistida e confere a
cadeia de attestation/approval, seus IDs, digests, identidades e revisão. Não
aceita projeto com plano, tarefa, submissão ou revisão anteriores. Planos,
tarefas, seleção inicial, revisão canônica, continuidade dos erros de governança,
evento de auditoria e recibo de instalação são gravados na mesma transação.
Qualquer falha desfaz todas essas alterações.

A ordem dos planos fica em `plan_ordinal`. Ao concluir a última tarefa de um
plano por revisão e avanço autorizados, o motor passa ao próximo plano ordenado.
O ponteiro `active_plan_id` identifica o único plano selecionado; o status
interno `active` continua significando plano ainda não concluído. Importar um
pacote deixa todas as tarefas sem conclusão; somente a primeira fica corrente.

O recibo imutável permite repetir a instalação idêntica até depois de tarefas
terem avançado, sem repetir eventos ou restaurar o estado antigo. Um pacote
conflitante é recusado. A instalação não escreve os JSONs do projeto: a projeção
existente os atualiza a partir do estado confirmado pelo serviço.

### 5.3 Launcher operacional implementado na T11.8

[`run-official-task-operation.sh`](../packaging/systemd/run-official-task-operation.sh)
oferece uma entrada única para as sete operações. Seu supervisor Python usa o
binário instalado existente; não compila, instala, cria usuários, aprova
propostas nem migra projetos automaticamente.

| Operação | Sujeito do cliente | Autoridade temporária |
| --- | --- | --- |
| `prepare` | operador que chamou sudo | `StateAdmin` no projeto/workspace |
| `inspect` | operador | `DiagnosticPrivate` no projeto/workspace |
| `attest` | `--ci-user` distinto do operador | `AttestationCreate` no projeto/workspace |
| `approve` | operador | `StateAdmin` no projeto/workspace |
| `install` | `--ci-user` distinto do operador | `InstallationAdmin` no projeto/workspace |
| `identity-associate` | operador | `StateAdmin` no candidato exato |
| `identity-rollback` | operador | `StateAdmin` no candidato exato |

O usuário CI deve existir, ser não-root, diferente da conta do serviço e ter
acesso ao socket administrativo pelo grupo instalado. O supervisor verifica
isso antes de emitir permissões; não altera grupos para fazer o teste passar.
As identidades declaradas na attestation/approval devem corresponder ao UID
autenticado conforme §5.1; o launcher recusa divergência, sem corrigir decisões.

O JSON chega por **stdin regular 0600**, pertencente ao operador, e o recibo sai
por **stdout regular 0600, vazio**, do mesmo operador. Não são aceitos pipes,
terminal, `.env`, paths de origem, tokens ou JSON como argumentos. Exemplo de
uso **depois do aceite da instalação atualizada na T11.10**, com entrada já
revisada e privada:

```bash
umask 077
set -o noclobber
sudo bash packaging/systemd/run-official-task-operation.sh prepare < origem.json > proposta-nova.json
```

Usar sempre uma saída nova: `noclobber` impede que o shell trunque um artefato
existente antes que o launcher possa validá-lo. Para `attest/install`, acrescentar
`--ci-user NOME` antes dos redirecionamentos. Cada operação é uma decisão
separada; o launcher não transforma uma proposta em aprovação automaticamente.
`bash packaging/systemd/run-official-task-operation.sh --help` não precisa de sudo.

O supervisor decripta a credencial gerencial instalada em memória. Cada cliente
recebe somente os descritores necessários, com entrada selada/somente leitura,
ambiente reconstruído e privilégios de root removidos. A credencial gerencial
não acompanha o cliente da operação; este recebe apenas seu grant escopado.
Os arquivos de saída herdados têm dono igual ao UID que executa cada cliente.

O grant dura no máximo 600 segundos e é revogado ao terminar, inclusive após
erro, timeout ou interrupção tratável. Falha de emissão com descritor produzido
também exige revogação, pois o pedido pode ter chegado ao serviço. Não se declara
sucesso nem se publica recibo quando a revogação falha. `SIGKILL`, queda de energia
ou indisponibilidade podem impedir essa confirmação. `started.json` é persistido
antes da emissão; sua presença sem `result.json` significa execução interrompida,
não sucesso. Falhas tratáveis ficam no resultado como falhas/não confirmadas.
O limite de validade do grant continua aplicável.

O comando Rust de revogação agora pode confirmar uma revogação já existente —
necessário após `identity-rollback`. Conserva ID, data e motivo originais e
acrescenta auditoria da confirmação autorizada. Não reativa grants nem ignora
divergência de operador ou epoch.

Após o pré-check, resumos sanitizados ficam em
`target/t118-official-task-operation-logs/<run>/result.json`, legíveis no projeto.
Não contêm propostas, texto de tarefas, tokens, stderr bruto ou caminhos de
planos externos. Artefatos de recuperação ficam no subdiretório privado 0700,
sem limpeza de diretórios; o arquivo transitório do grant é esvaziado ao fechar.
Logs e artefatos não são autoridade de conclusão nem são executados como shell.
Validação local cobre orquestração e descritores; troca real de UID e integração
com systemd permanecem no aceite instalado da T11.10.

## 6. Semântica da associação

A associação deve executar em uma única transação do registry:

1. autenticar capability `StateAdmin` escopada;
2. localizar todos os recursos por IDs exatos;
3. confirmar que o destino possui binding de cutover válido;
4. confirmar que a origem provisória corresponde aos fingerprints esperados;
5. recusar origem com tarefa/histórico não vazio até haver política explícita de
   reconciliação;
6. inserir um binding novo sem sobrescrever colisão;
7. registrar evento e recibo na mesma transação;
8. tornar o vínculo antigo inativo ou quarentenado sem apagá-lo;
9. preservar a tarefa corrente já validada no candidato;
10. permitir replay idêntico e rejeitar qualquer associação concorrente.

Não usar `ON CONFLICT DO UPDATE` para identidade. Não listar candidatos para
escolher o primeiro. Não usar `project_key` como autoridade.

### 6.1 Associação implementada na T11.6

`operator migration identity-associate` usa ADA1, com entrada em
`ANTDELIRIUM_MIGRATION_ASSOCIATION_INPUT_FD`, recibo privado em
`ANTDELIRIUM_MIGRATION_ASSOCIATION_OUTPUT_FD` e a capability administrativa no
descritor existente. Valem os mesmos limites, validação de arquivos e separação
de descritores da T11.4. O terminal recebe somente IDs, revisão e estado.

A autoridade é operador `StateAdmin` no **projeto/workspace candidato exatos**.
O UID autenticado também deve ser o proprietário do registro provisório; um
grant global, CI ou de outro workspace não autoriza a associação. O daemon
confere IDs e digests do binding de cutover tanto no registry como no banco do
candidato, incluindo a identidade do serviço. Antes da primeira associação,
recalcula o digest do estado lógico para recusar candidato alterado após cutover.

O storage-v17 grava associação imutável, recibo e auditoria na mesma transação
do registry. `expected_registry_revision` é o contador exclusivo dessas
mudanças de associação, inicialmente zero; não é a revisão das tarefas nem a quantidade
de projetos registrados. Os bancos de origem e destino permanecem bloqueados
contra escrita durante a conferência e não são modificados pela associação.

A consulta efetiva do wrapper passa a usar o vínculo explícito. A linha
provisória original permanece intacta, mas inativa para resolução, e suas
capabilities são recusadas. A tarefa corrente do candidato não é reativada,
reordenada ou substituída. Repetir a entrada idêntica recupera o mesmo recibo,
inclusive após alterações posteriores no candidato, sem retroceder seu estado.

Na entrega da T11.6 somente `require-empty` era executável: nenhum plano, tarefa,
submissão, revisão ou registro legado pode existir na origem, e a revisão deve
ser zero. Observações de compilação e backup inicial são preservados. Origens
com workspaces adicionais, identidades concorrentes ou destino já associado
são recusadas; nenhum worktree é capturado implicitamente. Um novo worktree da
identidade já associada também exige reconciliação explícita. A política
`quarantine-preserve` e o rollback foram acrescentados na T11.7, conforme abaixo.

### 6.2 Quarentena e rollback implementados na T11.7

`require-empty` mantém a recusa de origem com estado canônico. A política
explícita `quarantine-preserve` permite associar uma origem não vazia, mas
mantém seu banco, tarefas, eventos e backup Git intactos e separados do
candidato. Não há fusão de tarefas nem conclusão inferida. Workspaces adicionais
e identidades conflitantes continuam recusados; essa política não autoriza
capturar worktrees não especificados no pedido.

Antes de uma nova associação ou reversão, o serviço bloqueia os três bancos
contra escrita e faz cópias SQLite consistentes do registry, origem e candidato,
incluindo o estado confirmado no WAL. Os snapshots ficam em diretórios privados
0700, com arquivos 0600 e manifesto de hashes; nenhum caminho é devolvido pela
API. O vínculo só muda depois da persistência dessas cópias. Uma falha aborta a
transação e mantém qualquer cópia já produzida para diagnóstico, sem limpeza.
Essas cópias são pontos de recuperação; o rollback normal não as sobrepõe aos
bancos em uso. O histórico Git privado existente não é reescrito.

`operator migration identity-rollback` usa os mesmos descritores privados da
associação. Exige operador `StateAdmin` no candidato exato e o UID proprietário
da origem. Valida associação, digest, IDs e revisão do registry; grava um novo
evento e recibo imutável e restaura a resolução para o registro original.
Não restaura uma revisão antiga de tarefas: submissões, revisões e avanços
posteriores no candidato permanecem intactos.

A associação revoga os grants da origem. O rollback revoga os grants dos dois
projetos, inclusive aquele que autorizou a operação. Replay exige autorização
atual: depois do rollback, emitir nova capability escopada ao candidato para
recuperar o mesmo recibo. A repetição não duplica eventos nem revoga a nova
capability. Credenciais anteriores nunca voltam a funcionar por restauração do
vínculo. A API limitada do agente não recebe operações novas.

O storage-v18 conserva as associações anteriores e acrescenta registros
imutáveis de reversão e de recuperação. A consulta efetiva considera somente
associações sem rollback. Repetir a associação depois de revertida é recusado;
o histórico único também impede reassociar silenciosamente os mesmos IDs.
Uma nova migração após reversão requer um fluxo futuro explicitamente aprovado,
não edição de registros. A atualização v17→v18 não cria nem desfaz vínculos.

## 7. Geração de tarefas

O gerador é determinístico para a mesma requisição e não é um agente autônomo.
Ele valida e normaliza uma proposta fornecida pelo operador. Se futuramente um
modelo produzir propostas, sua saída entra pelo mesmo contrato sem capability
de aprovação ou instalação.

Regras mínimas:

- preservar texto e ordem aprovados;
- gerar IDs opacos sem incorporar nomes ou paths;
- rejeitar duplicatas, ciclos, tarefas vazias e paths proibidos;
- limitar quantidade, bytes e profundidade antes de alocar;
- não inferir conclusão, revisão, grade ou plano ativo de checkboxes antigos;
- registrar diferenças e itens não convertidos em relatório protegido;
- expor ao operador apenas resumo sanitizado e digests.

## 8. Recuperação e compatibilidade

- detectar registros contaminados pelo daemon de 7 de setembro antes de migrar;
- nunca associar um projeto provisório não vazio silenciosamente;
- manter snapshots SQLite consistentes do registry e bancos afetados antes da
  operação, com verificação de integridade e hashes;
- preservar candidatos preparados e rejeitados;
- suportar rollback da associação sem apagar eventos posteriores;
- manter storage-v13 legível e migrá-lo aditivamente para storage-v14, necessário
  para os registros protegidos e separados de proposta, attestation e approval;
- aplicar em seguida storage-v15 para ampliar os tipos aceitos na auditoria de
  emissão/revogação das capabilities escopadas, copiando integralmente os eventos
  anteriores na mesma transação; v14 existente também segue essa migração;
- aplicar storage-v16 para a ordem explícita de novos planos e os recibos
  imutáveis de instalação; planos anteriores mantêm ordinal nulo e seu fluxo
  existente, sem inferir ordem por nome, ID ou posição física;
- aplicar storage-v17 aditivamente para associações, revisão do registry e
  consulta efetiva de identidade, sem reatribuir fingerprints nos registros
  originais ou criar associações durante a atualização do banco;
- aplicar storage-v18 para reversões e pontos de recuperação imutáveis,
  preservando vínculos v17 e todo o histórico, sem rollback automático;
- manter o wrapper e a API limitada binariamente compatíveis.

## 9. Matriz obrigatória de testes

### Unidade e protocolo

- contrato válido, truncado, grande demais, desconhecido e com campo extra;
- IDs, ordinais, digests e paths inválidos;
- capabilities erradas, expiradas, revogadas ou de outro papel;
- replay idêntico e request ID conflitante;
- nenhuma operação nova disponível no canal do agente.

### Identidade

- zero, um e vários candidatos;
- zero, um e vários workspaces;
- dois projetos homônimos;
- linked worktrees compartilhando projeto e separando workspace;
- fingerprint já vinculado a outro projeto;
- associação concorrente;
- ausência de seleção por ordem SQL, prefixo ou slug.

### Estado canônico

- instalação v1 permanece compatível;
- v2 instala múltiplos planos em ordem explícita;
- somente uma tarefa fica corrente;
- projeto não vazio falha sem mutação;
- checkbox legado não conclui tarefa;
- falha entre escrita e evento faz rollback integral;
- projeções só mudam depois do commit canônico.

### Instalação

- serviço, wrapper, bundle e release com hash idêntico;
- nenhuma compilação como root;
- backup e rollback reais;
- ELF e logs sem paths protegidos;
- fixture Rust interna primeiro;
- projeto externo somente no aceite operacional autorizado posterior.

## 10. Ordem de implementação

1. concluir a reversão e recuperar o daemon instalado;
2. congelar os contratos e invariantes da Onda 11;
3. implementar o gerador puro e seus testes sem tocar no serviço;
4. persistir proposta, attestation e approval em storage protegido;
5. estender o protocolo administrativo por descritores;
6. implementar instalação v2 transacional;
7. implementar associação explícita e quarentena reentrante;
8. criar launchers operacionais sem paths nos argumentos;
9. executar matrizes hostis e `make ci`;
10. provar o ciclo instalado com fixture sintética;
11. somente então preparar o aceite do AutoBrowser real.

## 11. Critério de conclusão

A API está pronta quando um projeto Rust vazio recebe tarefas por pacote oficial
e um candidato migrado pode ser associado à identidade exata somente após
attestation e approval, enquanto qualquer tentativa automática, ambígua ou
originada do perfil agente falha sem alterar registry, tarefa ou projeções.

O aceite do AutoBrowser é uma operação separada: ele comprova o primeiro uso da
API, mas não autoriza código específico do AutoBrowser dentro do daemon.
