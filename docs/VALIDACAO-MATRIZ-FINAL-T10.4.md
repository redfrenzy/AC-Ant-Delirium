# Matriz final de validação — T10.4

## Escopo

A T10.4 foi executada somente com fixtures Rust mantidas neste repositório e
diretórios descartáveis sob `target/`. Nenhum projeto externo, incluindo
AutoBrowser, foi lido ou modificado.

O release avaliado corresponde ao código do commit `34465de` e ao SHA-256
`cd68d322d37f897c9ddc82324d73d953ae748c1249f921b6afdc412d5b7cdedb`.
Esse digest foi idêntico no ELF local, bundle revisado, wrapper Cargo do usuário
e serviço instalado.

## Defeito encontrado e corrigido

A primeira execução da matriz mostrou que duas worktrees ligadas recebiam
`project_ref` diferentes. A causa estava no contrato interno: o wrapper enviava
um único fingerprint derivado do caminho físico do checkout e o registry
impunha `project_id UNIQUE` à inscrição provisória. Assim o modelo não conseguia
representar um projeto com mais de uma workspace.

A correção separou:

- `project_identity_fingerprint`, derivado da identidade do Git common-dir
  quando disponível;
- `workspace_identity_fingerprint`, derivado do checkout Cargo específico.

O storage-v11 mantém uma tabela de identidade de projeto e permite várias
inscrições de workspace para o mesmo `project_id`. Registros v10 são migrados
sem mudar IDs, chaves ou timestamps; o fingerprint anterior é usado como as
duas identidades iniciais.

Os testes focados passaram:

- 12 testes do protocolo limitado;
- 20 testes do storage do serviço;
- 10 testes do armazenamento transacional;
- teste explícito de projeto compartilhado com workspaces distintas;
- migração v9→v10→v11 e inicialização concorrente.

## Prova instalada do upgrade

O upgrade final está registrado em
`target/t86-capability-upgrade-logs/20260902T133542Z`:

- exit code `0` e `upgrade_committed=true`;
- 28 bancos com schema `11`;
- schema anterior `11`, confirmando também o caminho idempotente v11→v11;
- preservação das identidades de projeto/workspace `ready`;
- `quick_check`, foreign keys, capabilities e transições válidos;
- wrapper do usuário, serviço e bundle iguais ao release;
- `antdelirium doctor` com `result=ready`.

A execução anterior, em
`target/t86-capability-upgrade-logs/20260902T132418Z`, comprovou a migração real
de 25 bancos v10→v11 com `project_workspace_identity_migration=ready`.

## Identidade, cache e concorrência

`make installed-identity-matrix` passou. A evidência está em
`target/t104-installed-identity-logs/20260902T133037Z-27828`:

- dois projetos independentes com o mesmo nome receberam projetos distintos;
- dois `cargo check` paralelos terminaram sem perda ou JSON parcial;
- um segundo check atendido pelo cache preservou os bytes das projeções;
- duas worktrees ligadas compartilharam `project_ref`;
- cada worktree recebeu seu próprio `workspace_ref`;
- projetos não-Git receberam somente os dois JSONs públicos;
- worktrees Git receberam os dois JSONs e a política `guard/.gitignore`;
- nenhum temporário de publicação permaneceu;
- nenhuma projeção publicou localizador físico.

O gate usa polling de até dez segundos, não uma espera fixa: o wrapper delega o
`rustc` imediatamente e o worker de auditoria materializa as projeções em
paralelo. A validação prossegue assim que ambos os JSONs aparecem.

## Workspaces, targets e comandos Cargo

O teste `Rustc_Wrapper_Cargo_Matrix_test` passou com uma workspace virtual
bloqueada, incluindo:

- `cargo check --workspace --all-targets -j 4` e repetição em cache;
- build script e código gerado;
- proc-macro;
- testes unitários e de integração;
- binário e exemplo nativos;
- target WASM;
- comparação da execução direta e encapsulada.

`make cargo-command-matrix` também passou na fixture descartável
`target/t76-command-matrix/1410-7163`, executando literalmente `cargo check`,
`build`, `test`, `run` e `clippy`. A prova de profundidade inválida confirmou
que o Cargo chamou o wrapper; nenhum proxy `cargo` foi substituído.

## Aceitação do serviço instalado

`make installed-generic-acceptance` passou com exit `0`. A evidência está em
`target/t8g-generic-installation-logs/20260902T133609Z-32220`:

- release, bundle, wrapper e serviço com o mesmo digest;
- `doctor=ready`;
- `cargo check`, `cargo build` e `cargo test` aprovados;
- exatamente `guard/to-do_list.json` e `guard/governance_error.json` no projeto
  sintético não-Git;
- contratos públicos válidos e arquivos `0600`;
- nenhuma divulgação de caminho físico;
- nenhum layout obsoleto criado.

## Resultado

A matriz mínima da T10.4 está satisfeita. Ela cobre projetos descartáveis,
comandos Cargo normais, cache, nomes repetidos, worktrees, workspace virtual,
concorrência, target nativo/WASM, build scripts, proc-macros e o serviço real
instalado. Enforcement de branch em provedor remoto permanece exclusivamente
na T9.7 e não é reivindicado por esta prova local/host.
