# Backup Git privado global — T9.8 parcial

## Estado honesto

Os commits `a8ba8e4`, `d9ea687`, `4c5532c` e `7a33ce9` implementam o núcleo de
armazenamento Git protegido e os quatro checkpoints locais da T9.8. O
`cargo check` comum entrega o workspace por descritor para o baseline; a
submissão entrega nova imagem pelo mesmo canal; revisão e avanço reutilizam a
árvore protegida associada à transição anterior sem receber acesso ao
workspace. T9.8 permanece aberta pelos itens de integração e recuperação
listados abaixo.

## O que foi implementado

`ProtectedGitBackup` é criado dentro do storage privado de cada projeto novo e
recuperado na inicialização de projetos já registrados. O módulo:

- valida ownership e diretórios `0700` sem seguir symlink;
- abre `backup/repository.git` diretamente com `gix::open`, nunca com descoberta
  ascendente;
- inicializa um repositório bare, sem `.git` no projeto governado;
- escreve blobs e árvores Git diretamente, sem executar `git`, `rsync` ou shell;
- limita a árvore versionada a `source/` e `snapshot.json`;
- usa identidade fixa do AntDelirium, tempo explícito e IDs opacos na mensagem;
- encadeia commits em `refs/heads/snapshots` e confirma o target publicado;
- rejeita paths absolutos, traversal, separador Windows, `.git`, `target`,
  `guard`, `AC_Ant-Delirium_props`, duplicatas, metadados inválidos e limites
  excedidos;
- endurece diretórios do repositório para `0700` e arquivos para `0600`.

O incremento `d9ea687` acrescenta o transporte e o baseline reais:

- o wrapper abre somente o diretório já descoberto, com `O_DIRECTORY`,
  `O_NOFOLLOW` e `O_CLOEXEC`;
- o bootstrap retorna apenas `backup_required`, nunca um path;
- o protocolo limitado envia o descritor por `SCM_RIGHTS`, com framing,
  deadline, marcador, formato exato e fechamento de descritores inesperados;
- o serviço recalcula a identidade do workspace usando o fingerprint do
  projeto e `dev/inode` obtidos pelo kernel, recusando substituição do diretório;
- a varredura ocorre com `fdopendir`, `fstatat` e `openat`, sem descoberta de
  repositório, sem seguir symlink e com limites de profundidade, entradas,
  arquivo e bytes totais;
- `.git`, `target`, `guard`, `plugins` e `AC_Ant-Delirium_props` na raiz não
  entram no snapshot;
- o primeiro `compile.observe` autorizado grava um commit `baseline` com o
  fingerprint da compilação, e novos bootstraps deixam de solicitar o mesmo
  baseline depois que a referência privada existe.

Os incrementos `4c5532c` e `7a33ce9` completam os gatilhos locais:

- `submission.create` exige `source_descriptor=true` e um único descritor
  `SCM_RIGHTS`; sem ele a requisição é rejeitada antes de escrever a submissão;
- a imagem do código é gravada com `transition=submission` e o `submission_id`;
- replay idêntico retorna o mesmo commit, enquanto o mesmo ID com fonte ou
  metadados diferentes falha fechado;
- a revisão localiza pelo histórico a submissão exata que avaliou, reutiliza
  seu objeto `source/` e cria um commit apenas de metadados;
- o avanço localiza a revisão exata e também reutiliza a mesma árvore, sem
  abrir ou descobrir o projeto;
- IDs de transição são procurados em toda a cadeia de primeiro pai, portanto
  uma revisão de submissão anterior não captura por engano a fonte do HEAD.

O incremento `dbc5c1a` introduziu o schema `storage-v12` e os recibos tipados
`backup_checkpoints`. O incremento de recuperação seguinte passou a inspecionar
o primeiro commit da cadeia quando o Git privado já contém o baseline, mas o
respectivo recibo SQLite está ausente. O serviço valida contrato, IDs opacos,
workspace, tarefa, revisões, fingerprint, limites, timestamp e a ausência de pai
antes de reconstruir o recibo. Um baseline existente deixa de ser confundido
com um ciclo completo: inconsistência ou corrupção falha fechada.

O incremento `a16b610` fechou a janela inconsistente do avanço de tarefa. O
serviço agora cria ou reutiliza primeiro o commit `advance` e, em uma única
transação SQLite, grava seu recibo, o evento de auditoria e a mudança do estado
canônico. O horário original do commit recuperado alimenta o evento e o recibo,
e o ID do evento é derivado da transição; por isso reentrar antes ou depois do
commit SQLite não cria outro commit nem outro avanço. Falha posterior à inserção
do recibo dentro da transação reverte também evento e estado.

O incremento `0d2d13d` ampliou a recuperação do baseline para toda a cadeia
linear protegida. O bootstrap usa primeiro uma consulta SQLite barata; somente
quando falta um recibo ele percorre e valida os commits. Recibos de submissão e
revisão só são restaurados quando o respectivo registro canônico já existe. Um
commit `advance` ainda pendente não é promovido pelo bootstrap e permanece para
a retomada atômica da própria transição. Commits de outros workspaces do mesmo
projeto são preservados sem misturar seus recibos.

O contrato `protected-git-snapshot-v1` contém somente transição, IDs opacos,
revisões, horário lógico, fingerprint, contagem de entradas e bytes. Ele não
contém path do projeto, path do state root, capability, segredo ou comando.

## Validação executada

```text
cargo check --locked --offline --all-targets --target x86_64-unknown-linux-gnu
cargo test --locked --offline --all-targets --target x86_64-unknown-linux-gnu
cargo test --locked --offline --target x86_64-unknown-linux-gnu --bin antdelirium limited_protocol_tests
cargo test --locked --offline --target x86_64-unknown-linux-gnu --test Rustc_Wrapper_Cargo_Matrix_test
```

Resultado:

- check sem warnings;
- 198 testes de biblioteca e 9 testes de integração passaram; a suíte integral
  mais recente do binário passou com 318 testes e
  manteve 8 testes instalados condicionais ignorados;
- nove testes focados do backup, o ciclo limitado de submissão/revisão e o
  teste de avanço CI com checkpoint passaram após o incremento de revisão;
- os testes do protocolo limitado e o teste de vínculo do descritor à
  identidade persistida passaram;
- repositório pai deliberadamente presente não recebeu a referência privada;
- dois commits sintéticos produziram parent chain e fingerprints distintos;
- somente `source/Cargo.toml`, `source/src/lib.rs` e `snapshot.json` apareceram
  na árvore do teste positivo;
- uma fixture sem `.git` recebeu o baseline dentro do storage protegido sem
  criar `.git` ou outro estado Git no projeto;
- a mesma fixture removeu somente o recibo `baseline` do SQLite, preservou o
  commit Git e comprovou que o bootstrap seguinte restaurou o vínculo exato sem
  solicitar nova cópia da fonte;
- o teste de avanço criou o commit `advance`, interrompeu antes do SQLite,
  confirmou estado e recibo inalterados e recuperou usando exatamente o mesmo
  commit; uma segunda reentrada após a conclusão retornou o evento duplicado e
  o mesmo estado;
- uma tentativa com revisão inexistente falhou depois de iniciar a transação e
  comprovou rollback conjunto do recibo, evento e estado;
- o ciclo ADP/1 removeu os recibos de submissão e revisão, preservou registros e
  commits, e comprovou a reconstrução exata dos dois no bootstrap seguinte;
- o caminho de reinício encontrou um `advance` ainda sem estado canônico e
  comprovou que não o registrou antecipadamente;
- a matriz Cargo isola seu Cargo home e força endpoint indisponível, evitando
  que a instalação global contamine o cenário fail-open;
- nenhuma configuração Cargo, serviço instalado ou projeto externo foi
  alterado.

## O que falta para concluir T9.8

1. definir e validar a política definitiva de inclusão de arquivos do projeto;
2. tratar sem depender do retry do cliente a interrupção de uma submissão entre
   seu registro SQLite e a criação do commit Git;
3. derivar diff e contexto somente dos commits privados;
4. testar concorrência e corrupção das transições ainda abertas;
5. provar o ciclo completo instalado em fixture sem `.git`;
6. integrar o commit atual à decisão local da T9.7.
