# Backup Git privado global — T9.8 parcial

## Estado honesto

Os commits `a8ba8e4` e `d9ea687` implementam o núcleo de armazenamento Git
protegido e o primeiro checkpoint automático da T9.8. O `cargo check` comum já
entrega o workspace por descritor ao serviço quando o projeto ainda não possui
baseline; submissão, revisão e avanço ainda não criam seus checkpoints. Por
isso T9.8 permanece aberta.

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
- 197 testes de biblioteca, 315 testes de binário e 9 testes de integração
  passaram; 8 testes instalados condicionais permaneceram ignorados;
- sete testes focados do backup, 14 do protocolo limitado e o teste de vínculo
  do descritor à identidade persistida passaram;
- repositório pai deliberadamente presente não recebeu a referência privada;
- dois commits sintéticos produziram parent chain e fingerprints distintos;
- somente `source/Cargo.toml`, `source/src/lib.rs` e `snapshot.json` apareceram
  na árvore do teste positivo;
- uma fixture sem `.git` recebeu o baseline dentro do storage protegido sem
  criar `.git` ou outro estado Git no projeto;
- a matriz Cargo isola seu Cargo home e força endpoint indisponível, evitando
  que a instalação global contamine o cenário fail-open;
- nenhuma configuração Cargo, serviço instalado ou projeto externo foi
  alterado.

## O que falta para concluir T9.8

1. definir e validar a política definitiva de inclusão de arquivos do projeto;
2. persistir o vínculo entre commit, projeto, workspace, tarefa, revisão e
   fingerprint atual;
3. criar checkpoints reais na submissão, revisão e avanço;
4. derivar diff e contexto somente dos commits privados;
5. testar concorrência, falha parcial, corrupção e recuperação;
6. provar o ciclo completo instalado em fixture sem `.git`;
7. integrar o commit atual à decisão local da T9.7.
