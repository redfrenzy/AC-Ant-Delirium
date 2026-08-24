# Validação da gestão administrativa de capabilities — T8.6

## Problema fechado no código

A atestação independente da migração exige uma capability
`Ci/Administrative/AttestationCreate` vinculada ao projeto e workspace do
candidato. Antes desta implementação não existia handler de produção para
`capability.grant` ou `capability.revoke`. Além disso, `storage-v8` vinculava
`capability_grants.project_id` por foreign key a `projects`, embora o projeto só
seja registrado depois da atestação, no cutover. As duas condições formavam um
ciclo impossível no fluxo real.

## Implementação

`storage-v9` remove somente a foreign key prematura de projeto dos grants. A
foreign key composta de epoch por UID/ator, hashes de segredo, recibos de
request, revogações, índices e validação de IDs opacos permanecem. A migração
reconstrói as três tabelas relacionadas em uma transação e preserva os grants,
revogações e recibos existentes.

O listener `ADA1` agora reconhece corpos fechados para `capability.grant` e
`capability.revoke`. Ambas exigem uma capability global
`Operator/Administrative/InstallationAdmin` fresca. O handler gerenciável
aceita apenas `wrapper`, `agent`, `reviewer` e `ci` nas operações operacionais
já previstas; ele recusa `Operator`, `Service`, `StateAdmin`,
`InstallationAdmin` e `DiagnosticPrivate`, impedindo amplificação do bootstrap.
A validade emitida é calculada pelo relógio do serviço e limitada a 60–86400
segundos.

Grant/revogação e o respectivo evento em `capability_management_events` usam a
mesma transação e a sequência do journal de segurança. O evento protegido
contém somente IDs opacos, UID, ator, canal, operação, escopo e horário. Não há
path, cwd, project key, token, nonce ou payload. Somente SHA-256 do token é
persistido.

O cliente interno `capability grant` gera ID e token com a fonte aleatória do
sistema, grava `admin-capability-v1` exclusivamente em um descriptor de saída
regular, vazio, `0600`, owner do processo e link único, e nunca imprime o token.
`capability revoke` recebe o descriptor-alvo por FD. Os dois comandos ficam fora
de `--help` e `--api`; socket administrativo, capability de instalação e peer
UID continuam obrigatórios.

## Provas executadas

- grant CI para projeto/workspace ainda não registrados, autorização exata e
  revogação posterior;
- dois eventos auditáveis, sequência contínua e ausência de segredo bruto no
  SQLite;
- recusa atômica de criação de capabilities administrativas privilegiadas;
- request/response ADA1 completo sem token no response;
- descriptor de saída recusado quando não é arquivo regular privado `0600`;
- upgrade simulado de registry `storage-v8` preenchido para `storage-v9`, com
  preservação e autorização do grant anterior;
- suíte completa verde com 192 testes da biblioteca, 291 testes do binário
  (mais sete cenários reais corretamente ignorados) e integrações 2+1+6;
- `cargo check --locked --all-targets --target
  x86_64-unknown-linux-gnu` com `RUSTFLAGS=-D warnings`, formatação, schemas,
  scripts e `git diff --check` verdes;
- release reconstruído com remapeamento de paths e gate de não divulgação
  verde. O bundle local contém o mesmo ELF e registra seu SHA-256 no manifesto
  renderizado.

## Implantação real concluída

O bundle `storage-v9` foi instalado pela rotina
[`packaging/systemd/upgrade-installed-service.sh`](../packaging/systemd/upgrade-installed-service.sh):
ela não recompilou como root, parou os dois sockets antes da substituição,
manteve backup recuperável de binário e estado e gravou os diagnósticos sob
`target/t86-capability-upgrade-logs/20260824T235551Z/`. O recibo terminou com
`exit_code=0` e `upgrade_committed=true`. O registry real passou em schema 9,
`quick_check`, foreign keys, recibo único de provisionamento, dois grants
iniciais e presença da tabela de gestão. Serviço e os dois sockets permaneceram
ativos após a segunda validação; o hash do ELF instalado coincide com o release
e o bundle revisados.

Se esse gate interromper a implantação, a rotina read-only
[`packaging/systemd/diagnose-capability-upgrade.sh`](../packaging/systemd/diagnose-capability-upgrade.sh)
inspeciona somente o `failed-state` preservado, copia cada SQLite e seus
sidecars para um diretório privado efêmero sob `/run` e registra no projeto
apenas rótulos lógicos, versões, contagens e invariantes. Ela não reinicia o
serviço, não abre os bancos protegidos para escrita e não publica bytes do
storage.
O primeiro upgrade real revelou uma corrida no gate: `systemctl is-active`
retornou em 35 ms, enquanto a transação de migração ainda estava em andamento.
O rollback preservou um `failed-state` já íntegro em schema 9, comprovado pelo
diagnóstico read-only. A rotina agora espera explicitamente até dez segundos
por schema, integridade, foreign keys, recibo de provisionamento, grants e tabela
de gestão; depois exige um segundo passe após um segundo de estabilidade antes
de confirmar a implantação. A repetição concluiu a implantação sem acionar
rollback e preservou o novo backup sob autoridade do operador.

## Limite operacional restante

Com o serviço v9 validado, o supervisor pode emitir a capability CI e executar
a atestação real com
[`packaging/systemd/manage-ci-attestation-capability.sh`](../packaging/systemd/manage-ci-attestation-capability.sh).
Essa rotina descriptografa a authority de instalação apenas em diretório root
sob `/run`, entrega FDs já abertos a um processo rebaixado ao UID do operador,
cifra o novo descriptor antes de remover o plaintext e também executa a
revogação sem imprimir tokens. Todos os logs sanitizados ficam sob
`target/t86-capability-operator-logs/`. A troca operacional do AutoBrowser e a
T8.7 continuam posteriores a essa implantação.
