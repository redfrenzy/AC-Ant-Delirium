# Validação de overhead do wrapper — T6.7

## Escopo

Esta evidência compara o compilador direto e o mesmo compilador executado pelo
`antdelirium` como `RUSTC_WORKSPACE_WRAPPER`. A medição usa a fixture Cargo da
T6.6 e cobre:

- uma invocação isolada de `rustc --version`;
- primeiro `cargo check` com target vazio;
- `cargo check` incremental depois de uma mudança real em fonte;
- `cargo check` no-op sem alteração posterior.

O endpoint e o descriptor de capability são removidos do ambiente. Assim a
medição isola o custo do wrapper audit-only quando o serviço está indisponível,
sem confundir a T6.7 com instalação, transporte ativo ou desempenho do serviço.

## Método reproduzível

O teste [`Rustc_Wrapper_Overhead_test.rs`](../tests/Rustc_Wrapper_Overhead_test.rs):

1. cria três cópias independentes do workspace virtual bloqueado por
   `Cargo.lock`;
2. usa targets Cargo separados para a rota direta e encapsulada;
3. alterna qual rota executa primeiro em cada rodada;
4. compara exit status, stdout e stderr de cada par;
5. calcula a mediana de três pares para cada cenário Cargo;
6. calcula a mediana de 21 pares para a invocação isolada de `rustc`;
7. considera overhead somente a parte positiva de `wrapped - direct`.

Comando da amostra Cargo:

```text
cargo check --quiet --locked --offline --workspace --all-targets \
  --target x86_64-unknown-linux-gnu -j 4
```

Comando de reprodução da medição:

```text
RUSTFLAGS="-D warnings" cargo test --locked --offline \
  --test Rustc_Wrapper_Overhead_test \
  --target x86_64-unknown-linux-gnu -- --nocapture
```

## Ambiente observado

- Linux `7.0.0-29-generic`, `x86_64`, quatro CPUs disponíveis;
- `rustc 1.97.1 (8bab26f4f 2026-07-14)`;
- `cargo 1.97.1 (c980f4866 2026-06-30)`;
- serviço limitado não instalado e não simulado.

## Resultado

Tempos de parede medianos observados em 20 de agosto de 2026:

| Cenário | Direto | Wrapper | Overhead positivo | Limite T6.7 |
|---|---:|---:|---:|---:|
| invocação de `rustc` | `13,935 ms` | `16,156 ms` | `2,220 ms` | `25 ms` |
| check limpo | `596,938 ms` | `645,691 ms` | `48,752 ms` | `2 s` |
| check incremental | `180,607 ms` | `185,821 ms` | `5,214 ms` | `1 s` |
| check no-op | `29,466 ms` | `30,605 ms` | `1,139 ms` | `250 ms` |

Todos os pares tiveram o mesmo exit status e bytes idênticos em stdout e
stderr. Todos os limites passaram.

## Decisão

Os limites acima passam a ser tetos de regressão automatizados, não metas de
latência. O limite de `25 ms` fecha o orçamento adicional aceitável por entrada
do wrapper nesta fase. Os limites Cargo são deliberadamente maiores para
absorver variação de scheduler, filesystem e criação de processos sem esconder
uma regressão material.

No cenário no-op, Cargo pode não invocar o wrapper. Portanto os `1,139 ms`
observados comparam o comando Cargo configurado e não provam uma nova auditoria.
Isso é o comportamento correto definido na seção de cache do plano.

O benchmark não mede a rota com serviço protegido ativo. Embora as operações
de governança ocorram no filho audit-only e não sejam aguardadas pelo pai, o
custo de `fork`, transporte e contenção do serviço deverá ser medido na matriz
integral T10.6, depois da instalação e dos gates de isolamento.
