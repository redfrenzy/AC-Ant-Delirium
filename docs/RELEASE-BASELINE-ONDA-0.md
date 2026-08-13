# Release reproduzível da Onda 0

Esta é a evidência da T0.8. O objetivo é permitir comparação futura com o binário único sem depender de artefatos voláteis em `target/`.

## Identidade da entrada

- commit Git reportado pelo build: `9a04345`;
- `SOURCE_DATE_EPOCH`: `1782732878`;
- Rust: `1.93.1`;
- target: `x86_64-unknown-linux-gnu`;
- perfil: `release`;
- lockfile: obrigatório, resolução offline;
- hash agregado dos arquivos de entrada do build: `5bdfb36e849ab28f9fa36b777b61316da8a3d5e7dbe03a48272f66c448b632b3`.

O worktree contém mudanças posteriores ao commit informado. Por isso o hash agregado cobre `Cargo.toml`, `Cargo.lock`, `build.rs`, `src/`, `get_toml_for_new/` e `ant_delirium_config/`, exceto artefatos de log. Ele é a identidade correta desta baseline até que o usuário decida criar outro commit.

## Método

Foram criados dois `CARGO_TARGET_DIR` inicialmente vazios sob `tmp/`. Cada um executou:

```text
SOURCE_DATE_EPOCH=1782732878
ANT_DELIRIUM_SKIP_BUILD_HOOK=1
cargo build --offline --locked --release --bins --target x86_64-unknown-linux-gnu
```

O `build.rs` foi corrigido para usar `SOURCE_DATE_EPOCH` como `ANT_DELIRIUM_BUILD_UNIX` quando ele contém um inteiro válido, mantendo o relógio do sistema apenas como fallback. A variável também participa de `rerun-if-env-changed`.

## Resultado reproduzível

| Binário | Tamanho | SHA-256 nos builds A e B | `cmp` |
|---|---:|---|---:|
| `delirium_cli` | 5.675.584 bytes | `bd2759ecc59380eb3c0ac43df84e307d7f2ffb214f2394d2d47489be602dc155` | 0 |
| `guard` | 558.544 bytes | `0ffdc75bd54561d8b618f2af34f7fa23503de0f9e230b3b9805c2e5c6d0b9e69` | 0 |

Os pares são idênticos byte a byte.

Build IDs:

- `delirium_cli`: `955897d399ce41110462eb5e7f76471ed5597b04`;
- `guard`: `5e67b663a882195ed60e123a19bd8b7156cd1910`.

Os dois `delirium_cli --version` reportaram a mesma versão `0.1.0`, Git hash `9a04345` e Build Unix `1782732878`.

## Exposição residual do release

Remover o debug reduziu as ocorrências do prefixo `/home/dracker` no `delirium_cli` de 10.243 para 165, mas não resolveu a descoberta estrutural:

- os dois nomes de logs globais previsíveis continuam presentes;
- as 33 referências de topologia de plugin continuam presentes;
- as 26 referências aos TOMLs continuam presentes;
- as 39 referências aos JSONs administrativos continuam presentes;
- o `guard` ainda contém cinco ocorrências do prefixo absoluto do usuário e uma referência explícita a plugin/configuração.

Portanto, este release é reproduzível, mas deliberadamente não é apresentado como seguro contra engenharia reversa. Ele é o ponto de comparação para as fases de separação de domínio, binário único e hardening.

## Validação final associada

Após a correção de reprodutibilidade:

```text
cargo fmt --all -- --check
resultado: passou

Rust 1.93.1, cargo test --offline --locked --all-targets --target x86_64-unknown-linux-gnu
resultado: 232 passaram, 0 falharam

Rust 1.88.0, cargo check --offline --locked --all-targets --target x86_64-unknown-linux-gnu
resultado: passou
```
