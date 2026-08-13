# Baseline da Onda 0

Esta baseline registra o estado encontrado antes das correções estruturais do AntDelirium global. Ela é a evidência da tarefa T0.1 da checklist de implementação.

## Identificação

- Data da captura: `2026-08-11T16:53:10-03:00`
- Repositório: `/home/dracker/Projects/AC-Ant-Delirium`
- Branch: `master`
- Commit base: `9a043454f6533f9f530d8e0734a7d5c3be7a6804`
- Commit base criado em: `2026-06-29T08:34:38-03:00`
- Mensagem: `chore: save current state before Task Orchestrator implementation`

O worktree já estava substancialmente modificado antes do início da Onda 0. Essas mudanças são tratadas como estado preexistente e não devem ser restauradas, descartadas ou reformatadas em massa.

Na captura inicial, `git status --porcelain` informou:

- 36 arquivos rastreados modificados;
- 10 entradas não rastreadas;
- 46 entradas no total;
- `git diff --stat`: 6.105 inserções e 377 remoções nos arquivos rastreados.

## Toolchain

```text
rustup: stable-x86_64-unknown-linux-gnu (default)
cargo: 1.93.1 (083ac5135 2025-12-15)
rustc: 1.93.1 (01f6ddf75 2026-02-11)
host: x86_64-unknown-linux-gnu
LLVM: 21.1.8
```

Targets instalados:

```text
aarch64-linux-android
wasm32-unknown-unknown
wasm32-wasip1
x86_64-linux-android
x86_64-unknown-linux-gnu
```

## Configuração Cargo efetiva relevante

O checkout contém `.cargo/config.toml` com:

```toml
[build]
target = "wasm32-unknown-unknown"
```

Por isso, toda validação nativa desta onda usa explicitamente:

```text
--target x86_64-unknown-linux-gnu
```

Nenhuma chave global `rustc`, `rustc-wrapper`, `rustc-workspace-wrapper`, `target` ou `runner` foi encontrada na configuração Cargo do usuário durante esta captura. A integração global proposta ainda não estava instalada.

## Manifesto atual

`cargo metadata --locked --no-deps --format-version 1` passou e confirmou:

- pacote `ac_ant_delirium` versão `0.1.0`;
- edition 2021;
- MSRV ainda não declarado;
- biblioteca `rlib`;
- binário `delirium_cli` em `src/main.rs`;
- binário `guard` em `src/guard_main.rs`;
- build script em `build.rs`;
- dependência externa `gix = 0.85.0`.

## Validações reproduzidas

### Formatação

```text
cargo fmt --all -- --check
resultado: passou
```

### Metadata

```text
cargo metadata --locked --no-deps --format-version 1
resultado: passou
```

### Compilação nativa

```text
cargo check --locked --all-targets --target x86_64-unknown-linux-gnu --message-format short
resultado: passou com dois warnings Rust e um aviso do build hook
tempo observado: 36,15 s
```

### Testes nativos

```text
cargo test --locked --all-targets --target x86_64-unknown-linux-gnu --no-fail-fast
resultado: falhou
```

Resumo por target:

| Target | Passou | Falhou |
|---|---:|---:|
| biblioteca | 92 | 8 |
| `delirium_cli` | 122 | 1 |
| `guard` | 4 | 0 |
| total | 218 | 9 |

## Inventário das nove falhas

1. `api::tests::info_fallback_points_to_hierarchy_file`
2. `tests::Aximon_Identity_test::tests::print_manifest_anchoring_violations_from_swarm_manifest`
3. `tests::Aximon_Identity_test::tests::print_recursion_violations_from_swarm_manifest`
4. `tests::Aximon_Sanitization_test::tests::print_dependency_purification_violations_from_swarm_manifest`
5. `tests::Aximon_Structural_test::tests::print_max_nesting_violations_from_swarm_manifest`
6. `tests::Logs_test::tests::recursion_jurisdiction_is_reported_as_p0`
7. `tests::Search_Engine_test::tests::test_taxonomy_parameter_resolution_with_fallback`
8. `tests::Swarm_Engine_test::tests::test_swarm_keeps_same_violation_totals_across_profiles`
9. `agent_runtime::tests::update_submit_status_updates_orchestrator_when_grade_is_passing`

O plano anterior mencionava oito falhas porque considerava apenas as oito falhas da biblioteca. A execução com `--all-targets --no-fail-fast` revelou também a falha do `delirium_cli`.

## Warnings e diagnósticos

Warnings Rust reproduzidos:

1. `src/Agent-Runtime_unitcore.rs`: valor inicial atribuído a `auto_adv` nunca é lido.
2. `src/Git_unitcore.rs`: variável `repo` não utilizada.

Aviso adicional do build hook:

- o hook procura um binário de governança em um caminho relativo externo, permanece em modo advisory e imprime o caminho resolvido;
- esse aviso é evidência do acoplamento e do vazamento de caminho que as ondas posteriores removerão;
- ele não é contado como warning Rust das duas linhas acima.

## Classificação inicial das falhas

Esta classificação é investigativa; uma falha só será encerrada depois de um teste focado e da suíte completa.

| Grupo | Falhas relacionadas | Hipótese respaldada pelo código atual |
|---|---|---|
| Expectativa de teste desatualizada | fallback da API, fallback taxonômico, submit aprovado | o comportamento atual produz informação válida mais específica ou usa o schema canônico novo |
| Jurisdição incompleta | `manifest_anchoring`, `dependency_purification` | o scanner comum materializa apenas `.rs`, enquanto essas regras precisam de `.md`, `.json` ou `Cargo.toml` |
| Filtro de fixture incorreto | `recursion` | a violação gerada possui sufixo do companion ou `jurisdiction`, mas o teste retém somente IDs terminados em `recursion` |
| Configuração divergente | `max_nesting`, prioridade de `recursion`, paridade entre perfis | limites e prioridade no perfil type 1 não representam o mesmo contrato dos outros perfis |

## Próxima ação autorizada

Corrigir cada causa com teste focado, preservar as mudanças preexistentes do worktree e executar novamente formatação, check e toda a suíte antes de concluir T0.2 ou T0.3.

## Resultado das correções T0.2 e T0.3

As nove falhas foram corrigidas sem remover cobertura:

- a jurisdição comum passou a materializar `.rs`, `.md`, `.json` e `.toml`, que são os formatos efetivamente consumidos pelos axiomas;
- fixtures equivalentes de manifesto e `else_free` foram adicionadas aos três perfis;
- os limites `max_nesting` do type 1 foram alinhados ao contrato semântico dos demais perfis;
- a prioridade de `recursion` do type 1 foi corrigida de `2.0600` para `0.0600`;
- o parser de PascalCase passou a remover o DNA composto completo antes de validar o nome-base;
- o filtro de integração de `recursion` passou a aceitar os IDs concretos de companion e jurisdição;
- asserts de API, fallback taxonômico e submit foram alinhados ao comportamento e ao schema canônicos atuais;
- foi adicionado um teste de regressão específico para DNA composto;
- os dois warnings Rust foram eliminados;
- o target `make ci` passou a executar formatação, check e testes com `RUSTFLAGS="-D warnings"`.

Validação final desta etapa:

```text
cargo fmt --all -- --check
resultado: passou

cargo check --locked --all-targets --target x86_64-unknown-linux-gnu
resultado: passou sem warnings Rust

cargo test --locked --all-targets --target x86_64-unknown-linux-gnu --no-fail-fast
resultado: 228 passaram, 0 falharam

make ci
resultado: passou; 228 testes sob -D warnings
```

O aviso advisory do build hook continua presente de forma intencional e catalogada. Ele será removido junto com a integração legada; não é um warning emitido pelo compilador Rust sobre o código do crate.

## Resultado da implementação T0.4

Foram adicionados snapshots explícitos com versão `v1` para quatro contratos da base madura:

- catálogo ordenado dos 27 axiomas, incluindo IDs e nomes públicos;
- 23 templates embarcados, congelados por nome, tamanho em bytes e hash FNV-1a de 64 bits;
- campos obrigatórios dos JSONs gerados, incluindo o `context.json` materializado em runtime;
- árvore obrigatória do bootstrap de um projeto `AutoBrowser`, separando os artefatos no projeto dos artefatos no plugin.

O `bin/guard` foi documentado no teste como artefato opcional: ele só é copiado quando há um executável `guard` ao lado do binário que realiza o bootstrap. O snapshot exige `bin/ac_ant_delirium`, que é o binário obrigatório no comportamento atual. Os três scripts `.template.sh` também são verificados como arquivos não executáveis.

Os contratos JSON ainda são implícitos nos templates existentes; portanto, o snapshot `json_schema_contract.tsv` congela os campos obrigatórios observados sem fingir que já existe uma camada formal de schemas. A criação e o versionamento de schemas canônicos permanecem na T1.4.

Validação desta etapa:

```text
cargo fmt --all -- --check
resultado: passou

cargo test --target x86_64-unknown-linux-gnu
resultado: 232 passaram, 0 falharam
```

O aviso advisory do build hook continua sendo a única mensagem de build conhecida e não alterou o resultado.

Próxima tarefa: T0.5, medir o overhead de `cargo check` limpo, incremental e sem mudanças.

## Resultado da medição T0.5

A medição foi feita sobre uma cópia do estado corrente do checkout em `/tmp`, sem `.git`, `target/` e `scratch/`, usando um `CARGO_TARGET_DIR` novo e isolado. Isso preservou o cache de 1,6 GiB do checkout original e evitou confundir a troca de `RUSTFLAGS` com overhead normal do Cargo.

Comando comum às amostras:

```text
cargo check --quiet --locked --all-targets --target x86_64-unknown-linux-gnu
```

Resultados de tempo de parede:

| Cenário | Amostras | Referência |
|---|---:|---:|
| target vazio, compilação limpa | `45,05 s` | `45,05 s` |
| mudança real e inócua em `src/lib.rs` na cópia temporária | `2,38 s`, `1,66 s`, `1,24 s` | mediana `1,66 s` |
| execução sequencial sem qualquer mudança | `0,28 s`, `0,29 s`, `0,29 s` | mediana `0,29 s` |

Orçamento de regressão para a base atual, nesta máquina e com o cache de dependências do Cargo já disponível:

- limpo: até `60 s`;
- incremental de um arquivo do crate: até `5 s`;
- no-op: até `1 s`.

Esses limites são uma margem de regressão do projeto atual, não uma decisão sobre o overhead adicional aceitável do futuro wrapper. A T3.9 deverá medir a diferença com e sem wrapper sob a mesma matriz; o limite específico do wrapper continua sendo uma decisão arquitetural aberta.

Todas as amostras terminaram com código `0`. A cópia e o target temporários foram usados somente para esta medição.

Próxima tarefa: T0.6, determinar e declarar o MSRV real e validar a toolchain 1.93.1.

## Resultado da validação T0.6

Os metadados das dependências indicavam inicialmente Rust 1.85: `gix 0.85.0`, a família `gix-*` resolvida e `hashbrown 0.17.1` declaram esse requisito. A compilação real mostrou que essa informação não era suficiente:

```text
Rust 1.85.0
resultado: falhou em human_format 1.2.1
causa: uso de let-chains, ainda instáveis nessa toolchain
```

O próximo candidato foi Rust 1.88.0, no qual let-chains estão estabilizadas. Com target isolado em `tmp/` e resolução offline:

```text
Rust 1.88.0
cargo check --offline --locked --all-targets --target x86_64-unknown-linux-gnu
resultado: passou

cargo test --offline --locked --all-targets --target x86_64-unknown-linux-gnu
resultado: 232 passaram, 0 falharam
```

O manifesto agora declara:

```toml
rust-version = "1.88"
```

A toolchain corrente também foi revalidada após a declaração:

```text
Rust 1.93.1
cargo test --offline --locked --all-targets --target x86_64-unknown-linux-gnu
resultado: 232 passaram, 0 falharam
```

Assim, o MSRV comprovado para o grafo atualmente travado no `Cargo.lock` é 1.88. Uma atualização de dependências deverá repetir esta matriz, principalmente para crates que não declaram `rust-version` corretamente.

Próxima tarefa: T0.7, capturar a baseline adversarial de strings, ELF, syscalls e caminhos expostos.

## Resultado da captura T0.7

A inspeção de `strings`, ELF, ajuda pública, syscalls de arquivo e logs globais foi consolidada na [baseline adversarial da Onda 0](./BASELINE-ADVERSARIAL-ONDA-0.md).

Além das exposições já esperadas, o ensaio encontrou um defeito operacional concreto: `config init` recebeu uma fixture explícita sob `tmp/`, mas selecionou silenciosamente a raiz Git real para uma operação mutável. Os artefatos comprovadamente criados pelo ensaio foram movidos para a área temporária ignorada pelo Git, preservando os arquivos anteriores.

Próxima tarefa: T0.8, produzir o release-base reproduzível e registrar seus hashes.

## Resultado do release T0.8

Dois builds release limpos, em targets isolados, produziram pares byte a byte idênticos de `delirium_cli` e `guard`. O `build.rs` passou a respeitar `SOURCE_DATE_EPOCH`, eliminando o timestamp de relógio como fonte de não determinismo quando um release controlado é solicitado.

Método, hashes, build IDs, tamanhos e exposição residual estão registrados em [release reproduzível da Onda 0](./RELEASE-BASELINE-ONDA-0.md).

Com T0.1 a T0.8 concluídas, o gate G0 está satisfeito. A próxima tarefa ordenada é T1.1, formalizar a matriz de atores e permissões antes das mudanças estruturais.
