# Validação do gate CI/supervisor — T7.7

## Resultado

O alvo `make authoritative-ci-gate` passou repetidamente sem reutilizar ou
limpar os artefatos de execuções anteriores. O modo protegido também passou com
um SHA-256 esperado fornecido externamente, e o mesmo gate recusou um digest
divergente antes de iniciar o Cargo.

Essa validação fecha a mecânica do gate. Ela não afirma que esta cópia local do
script, o runner ou uma política de branch já estão protegidos contra o agente.
Essa ativação externa e a assinatura de attestations pertencem à T9.7.

## Fronteira de confiança

O gate possui dois modos explícitos:

- `local-mechanics`: usa o release deste checkout e prova somente o
  funcionamento reproduzível;
- `protected-digest`: exige `ANTDELIRIUM_TRUSTED_WRAPPER` e
  `ANTDELIRIUM_EXPECTED_WRAPPER_SHA256` entregues pelo CI/supervisor protegido.

No segundo modo, path e digest não podem vir do workspace não confiável. O gate
valida o formato do SHA-256, compara o binário antes de executar Cargo e repete
o hash ao final para detectar troca durante a execução. Derivar o digest no
mesmo PR não cria autoridade e não é aceito como implantação protegida.

## Cobertura do wrapper local

A fixture possui `build.rustc-workspace-wrapper = "/bin/false"`. O Cargo home
descartável também contém configuração divergente e um `bin/antdelirium` decoy.
Um check de controle sem política protegida precisa falhar.

O check autoritativo remove os seis overrides oficiais de compilador/wrapper e
injeta por `cargo --config`:

- o `rustc` selecionado pelo runner;
- `build.rustc-wrapper=""`;
- o path do wrapper confiável em `build.rustc-workspace-wrapper`.

Como `--config` possui a precedência usada pelo gate e o script não aceita
argumentos livres do agente, o check seguinte ignora as escolhas divergentes do
workspace e do Cargo home. Um probe isolado com profundidade inválida exige
`AD-CONFIG-0001`, comprovando que o executável selecionado é o AntDelirium.

## Target limpo e cache

Cada execução cria uma raiz exclusiva sob `target/t77-authoritative-ci/` e usa
um `CARGO_TARGET_DIR` inicialmente vazio. O primeiro `cargo check -vv` precisa
conter uma invocação do wrapper confiável com `CARGO_PRIMARY_PACKAGE=1` e
`--crate-name t77_authoritative_ci`.

Um segundo check usa o mesmo target para representar o caminho atendido pelo
cache. Ele deve passar e não pode conter nova compilação primária. Isso é
advisory: o wrapper não é um hook de início de comando. A garantia autoritativa
vem do fato de que uma nova execução do gate sempre escolhe outro target vazio,
em vez de confiar no cache controlado pelo agente.

## Evidência mecânica

O gate grava `ci-gate-evidence-v1.json` com somente estados fechados e hashes,
sem paths. O contrato está em
[`ci-gate-evidence-v1.schema.json`](./contracts/v1/ci-gate-evidence-v1.schema.json)
e rejeita campos adicionais.

A execução protegida validada registrou:

```text
authority_mode=protected-digest
result=passed
clean_target=true
primary_wrapper_participation=verified
local_wrapper_override=superseded
cached_followup=passed-without-primary-recompile
signature=none-local-mechanics-only
```

`signature=none-local-mechanics-only` é deliberado: esse JSON prova a mecânica
e pode ser arquivado pelo runner, mas não deve ser aceito como attestation
assinada. A capability `attestation.create`, armazenamento externo, proteção de
branch e política fail-closed continuam fora desta tarefa.

## Comandos validados

```bash
make authoritative-ci-gate
bash -n packaging/ci/validate-authoritative-gate.sh
jq empty docs/contracts/v1/ci-gate-evidence-v1.schema.json
cargo +1.88.0 check --locked --offline --all-targets \
  --target x86_64-unknown-linux-gnu
make ci
```

Nenhum Cargo home real, serviço, socket, plano ou estado canônico foi alterado.
