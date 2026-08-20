# Cópia protegida para staging do AutoBrowser — T8.3

## Resultado

A origem AutoBrowser foi revalidada contra o ledger T8.2 e copiada integralmente
para um staging privado controlado por `ProtectedServiceStorage`. A publicação
foi concluída com este recibo:

```text
migration_id=mig_4tdwqnh4dngbhaxhehbvzzsx4t
project_id=prj_19vszrww3v57118bqhakg704vd
source_snapshot_sha256=7d11b66e162a10313ae3601982ce234653329743031d4a1a9ec874c34b2ccbcd
staging_snapshot_sha256=859131fcac8810989ccfb594fedb77db452556355950c4d3879a0741e654ba82
entries=3102
regular_files=2722
directories=380
staged_bytes=15155427
state=staged
```

O staging desta validação fica sob `target/legacy-staging-tests/` deste checkout.
Ele é uma evidência local mantida, não a instalação final em um state root de
sistema. A ativação da unidade dedicada, provisionamento da capability e
cutover pertencem às tarefas posteriores.

## Fronteira implementada

O processo operador é o único componente que recebe os locators físicos da
origem e do inventário. Ele:

1. apaga do próprio ambiente os quatro locators depois de capturá-los;
2. lê a capability administrativa de um descriptor herdado com `FD_CLOEXEC`;
3. reenumera a origem sem seguir symlinks;
4. compara path, tipo, modo, UID, GID e tamanho com o ledger;
5. percorre cada componente por descriptor com `openat` e `O_NOFOLLOW`, abre o
   arquivo com `O_CLOEXEC`, compara metadata antes/depois e recalcula SHA-256;
6. envia ao socket administrativo somente o inventário sem paths físicos e o
   fluxo de bytes em ordem determinística.

O daemon não abre a origem sob `/home` e não recebe seu path. O listener `ADA1`
aceita somente `migration.stage`, autentica peer, nonce, request e uma
capability global `Operator/Administrative/StateAdmin`, e então entrega o fluxo
ao storage protegido. `ADL1` não é aceito no canal administrativo.

Frames e fluxo usam socket não bloqueante, `poll` e deadline de 120 segundos.
Isso evita `setsockopt`, indisponível no perfil endurecido, e impede espera sem
limite durante inventário ou cópia.

## Publicação e integridade

O serviço cria primeiro `<migration-id>.partial/source` com diretórios `0700` e
arquivos `0600`. Cada arquivo é re-hashado antes de `sync_all`; ao final o
serviço grava, também em `0600`:

- `legacy-source-inventory-v1.json`;
- `migration-manifest-v1.json`;
- `stage-receipt-v1.json`.

Depois de sincronizar conteúdo e diretórios, um único `rename` publica o
diretório final. Uma falha preserva o `.partial` privado para diagnóstico e
nunca promove bytes incompletos. O digest de staging é calculado sobre a lista
ordenada de path relativo, tipo e hash de conteúdo, sem locator físico.

O manifesto permanece em `state=staged`, sem approval, cutover ou deadline de
rollback. Portanto esta tarefa não seleciona plano ativo, não altera tarefa,
não instala Cargo e não autoriza o uso da cópia como estado canônico.

## Original preservado

Após a cópia, a origem continuava no mesmo commit:

```text
f7405872e980da505eadc53e40be6589a7507aa4
```

O SHA-256 de `git status --porcelain=v1 --untracked-files=all` continuou igual ao
capturado na T8.2:

```text
f3b371a69fbb42474abbc4bb6c76324f601896b99f3be7d34e0933c062013033
```

Não houve checkout, reset, add, commit, limpeza, rename, chmod ou escrita na
origem. O reader de produção também recusaria publicar se qualquer entry
divergisse do snapshot T8.2 durante a cópia.

## Testes e contratos

Os testes cobrem:

- digest canônico real do ledger T8.2, inclusive o LF normativo de `jq -cS`;
- cópia byte a byte e permissões privadas;
- recusa de digest incorreto, stream corrompido e capability revogada;
- handshake `ADA1`, recusa do magic `ADL1` e vínculo de nonce/operação/digest;
- roundtrip completo por `UnixStream`, autorização no registry e staging pelo
  serviço, inclusive a recusa de symlink em um diretório ancestral;
- staging real dos 3.102 entries da origem.

O request `migration.stage` e o recibo possuem schemas fechados em
[`admin-protocol-v1`](./contracts/v1/admin-protocol-v1.schema.json) e
[`legacy-stage-receipt-v1`](./contracts/v1/legacy-stage-receipt-v1.schema.json).

## Próxima etapa

A T8.4 deve trabalhar somente sobre esta cópia controlada para classificar e
normalizar referências. Ela não deve editar a origem nem transformar o staging
em estado canônico antes das comparações, approvals e cutover posteriores.
