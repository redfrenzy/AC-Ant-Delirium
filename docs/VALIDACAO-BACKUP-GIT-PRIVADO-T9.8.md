# Backup Git privado global — T9.8 parcial

## Estado honesto

O commit `a8ba8e4` implementa o núcleo de armazenamento Git protegido da T9.8.
Ele não conclui o backup automático: o wrapper ainda não transmite o snapshot
do workspace e as transições canônicas ainda não criam os quatro checkpoints
exigidos. Por isso T9.8 permanece aberta.

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

O contrato `protected-git-snapshot-v1` contém somente transição, IDs opacos,
revisões, horário lógico, fingerprint, contagem de entradas e bytes. Ele não
contém path do projeto, path do state root, capability, segredo ou comando.

## Validação executada

```text
cargo check --locked --offline --all-targets --target x86_64-unknown-linux-gnu
cargo test --locked --offline --target x86_64-unknown-linux-gnu protected_git_backup
cargo test --locked --offline --target x86_64-unknown-linux-gnu initializes_private_registry
```

Resultado:

- check sem warnings;
- cinco testes focados do backup passaram;
- o teste de inicialização e recuperação do storage passou;
- repositório pai deliberadamente presente não recebeu a referência privada;
- dois commits sintéticos produziram parent chain e fingerprints distintos;
- somente `source/Cargo.toml`, `source/src/lib.rs` e `snapshot.json` apareceram
  na árvore do teste positivo;
- nenhuma configuração Cargo, serviço instalado ou projeto externo foi
  alterado.

## O que falta para concluir T9.8

1. entregar ao serviço um snapshot do workspace por canal limitado e sem
   locator físico;
2. definir e validar a allowlist automática de arquivos do projeto;
3. persistir o vínculo entre commit, projeto, workspace, tarefa, revisão e
   fingerprint atual;
4. criar checkpoints reais no baseline, submissão, revisão e avanço;
5. derivar diff e contexto somente dos commits privados;
6. testar concorrência, falha parcial, corrupção e recuperação;
7. provar o ciclo completo em fixture sem `.git`;
8. integrar o commit atual à decisão local da T9.7.
