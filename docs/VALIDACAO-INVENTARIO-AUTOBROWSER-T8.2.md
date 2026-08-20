# Validação e snapshot da origem AutoBrowser — T8.2

## Resultado

A origem legado foi lida duas vezes consecutivas sem mutação e produziu o mesmo
payload canônico nas duas passagens. O ledger versionado está em
[`autobrowser-legacy-source-inventory-v1.json`](./evidence/autobrowser-legacy-source-inventory-v1.json)
e possui o digest:

```text
7d11b66e162a10313ae3601982ce234653329743031d4a1a9ec874c34b2ccbcd
```

O contrato fechado está em
[`legacy-source-inventory-v1.schema.json`](./contracts/v1/legacy-source-inventory-v1.schema.json).
O gerador não grava na origem, não segue symlinks e recusa publicar um resultado
se um objeto muda entre os `stat` anterior/posterior ao hash ou se as duas
passagens divergem.

Este resultado é um snapshot consistente, não um freeze. Uma alteração legítima
posterior na origem cria outro digest. A T8.3 deve reexecutar a validação e
vincular a cópia exatamente ao digest aprovado antes de iniciar staging.

## Validação dos JSONs

Todos os paths com sufixo `.json` eram arquivos regulares e passaram em `jq
empty` nas duas passagens:

| Verificação | Resultado |
|---|---:|
| JSONs encontrados | 144 |
| JSONs sintaticamente válidos | 144 |
| JSONs inválidos | 0 |
| documentos `*.schema.json` | 24 |
| documentos JSON Schema válidos | 24 |

Os 24 schemas também passaram em `jsonschema.validators.validator_for(...).check_schema`.
O próprio ledger passou contra o schema Draft 2020-12 com format checker.

Os arquivos centrais tiveram ainda suas coleções obrigatórias interpretadas para
as contagens: `.tasks` em cada plano, `.queue` em submissões, `.entries` no
histórico e `.transition_tasks` na transição. Isso não significa que cada um dos
144 documentos possui um schema de domínio correspondente; a base legado ainda
mistura contratos formais, templates e formatos implícitos. Não foi inventado um
mapeamento de schema inexistente.

## Contagens registradas

O diretório raiz é descrito separadamente e não entra na contagem de entries:

| Tipo | Quantidade |
|---|---:|
| entries | 3.102 |
| arquivos regulares | 2.722 |
| diretórios | 380 |
| symlinks | 0 |
| outros tipos | 0 |

Contagens semânticas do legado:

| Coleção | Quantidade |
|---|---:|
| planos | 16 |
| tarefas nos planos | 73 |
| submissões | 0 |
| revisões na fila | 0 |
| eventos de histórico | 275 |
| problemas autoritativos em `problems/` | 1 |
| transições globais | 4 |
| schemas JSON | 24 |
| manifestos derivados em `Logs/artifacts/` | 5 |
| evidências de tarefas | 33 |

Templates sob `project_state/problems/` não foram contados como problemas
autoritativos. Da mesma forma, manifestos derivados e evidências não foram
promovidos a plano, tarefa ou histórico apenas por existirem.

## Modos, ownership e links

A raiz observada usa modo `775`, UID `1000` e GID `1000`. O ledger registra modo,
UID, GID, tamanho e SHA-256 individual de cada arquivo regular.

| Modo | Entries |
|---|---:|
| `444` | 2.171 |
| `555` | 2 |
| `664` | 541 |
| `755` | 8 |
| `775` | 380 |

O ownership não é homogêneo:

- 3.098 entries pertencem a UID/GID `1000:1000`;
- quatro pertencem a `65534:65534`;
- os quatro divergentes são `default_hierarchy.toml`, `default_rules.toml`,
  `bin/ac_ant_delirium` e `bin/guard`;
- os TOMLs estão `444` e os dois binários estão `555`.

Esses quatro objetos não serão `chown` ou normalizados implicitamente. A
divergência é parte do snapshot e precisa ser preservada/decidida pelo operador
na migração.

Não havia symlink. Mesmo assim, o contrato registra links de forma fechada:
target relativo pode ser guardado; target absoluto seria redigido e representado
somente por tipo e hash. Isso impede que a evidência carregue um locator físico.

## Paths e Git

Cada entry possui um path relativo, ordenado e único. O verificador recusa path
absoluto, componente `..`, duplicata e qualquer literal `/home/`. A localização
física permanece na documentação administrativa T8.1, fora do ledger que será
vinculado ao manifesto de migração.

Estado Git capturado:

```text
branch=main
head=f7405872e980da505eadc53e40be6589a7507aa4
worktree_clean=false
status_entries=34
```

O SHA-256 da lista de status registrada foi comparado com uma nova consulta
read-only após a geração e permaneceu igual:

```text
f3b371a69fbb42474abbc4bb6c76324f601896b99f3be7d34e0933c062013033
```

Não foi executado checkout, reset, add, commit, limpeza ou inferência de rename
na origem.

## Verificações executadas

```bash
bash -n packaging/migration/validate-legacy-source-inventory.sh
bash -n packaging/migration/verify-legacy-source-inventory.sh
bash packaging/migration/verify-legacy-source-inventory.sh
jq empty docs/contracts/v1/legacy-source-inventory-v1.schema.json
jq empty docs/evidence/autobrowser-legacy-source-inventory-v1.json
git diff --check
```

O validador portátil recompõe o SHA-256 do payload, todas as contagens de tipo,
histogramas, paths e invariantes de arquivo/link sem precisar acessar novamente
a origem. O gerador completo exige a origem explicitamente fornecida pelo
operador e recusa escrever a evidência dentro dela.

## Limite desta tarefa

Nenhum conteúdo foi copiado para staging, serviço ou state root externo. Não
houve mudança de formato, permissão, ownership, configuração Cargo, wrapper,
plano ou orquestrador. O snapshot não autoriza cutover e não substitui a futura
comparação origem/staging.
