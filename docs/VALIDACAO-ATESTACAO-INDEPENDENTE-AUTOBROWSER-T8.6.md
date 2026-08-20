# Validação da atestação independente do AutoBrowser — T8.6 parcial

Data da execução: 2026-08-20.

## Escopo desta etapa

Esta etapa atesta mecanicamente o candidato canônico criado na T8.6 sem lhe
conceder autoridade de produção. Ela não cria aprovação de operador, não altera
o manifesto, não instala ou ativa o serviço/wrapper global e não executa
cutover. A origem continua recuperável e a T8.6 permanece aberta.

## Separação de autoridade

`migration.attest` é uma operação administrativa fechada, ausente da API do
agente. Ela exige exatamente uma capability
`Ci/Administrative/AttestationCreate`, vinculada ao projeto e workspace do
candidato e sem escopo de tarefa. A capability global de operador usada para
staging, normalização, dry-run e conversão não satisfaz esse contrato; uma
capability CI também não representa `StateAdmin`, aprovação ou cutover.

O serviço deriva `verifier_subject_sha256` da identidade autenticada e do grant
confiável. O request público contém somente IDs opacos e o digest exato do
relatório de conversão. O locator da geração candidata permanece no storage
privado do serviço.

Três contratos fechados foram adicionados:

- `migration-attestation-request-v1` vincula migração, projeto, workspace e o
  relatório de conversão exato;
- `migration-verifier-attestation-v1` descreve evidência, identidade hash do
  verificador, checks, digests, contagens e limites de autoridade;
- `migration-attestation-receipt-v1` vincula a evidência ao seu SHA-256.

## Revalidação e persistência

Antes de persistir a atestação, o verificador:

1. reabre e valida o relatório de conversão pelo digest esperado;
2. repete integralmente a conversão idempotente, incluindo origem normalizada,
   dry-run, manifesto, bindings, projeções e contagens;
3. abre o SQLite candidato e exige schema 7, `quick_check=ok`, foreign keys
   ativas, zero violações e `trusted_schema` desativado;
4. calcula um fingerprint lógico determinístico sobre projeto, workspace,
   planos, tarefas, registros legados protegidos e snapshot de governança;
5. grava atestação e evento de auditoria na mesma transação imediata.

Eventos e atestações não participam do fingerprint lógico, portanto o próprio
journal não muda o estado atestado. Uma repetição pela mesma identidade e pelo
mesmo relatório retorna a evidência existente somente se todo o payload ainda
for idêntico; divergência é recusada.

Os seis checks registrados passaram:

- revalidação da conversão;
- integridade do storage;
- fingerprint lógico do estado;
- bindings de identidades opacas;
- não divulgação nas projeções;
- paridade das contagens de domínio.

## Resultado real

Geração atestada:

- migration: `mig_4tdwqnh4dngbhaxhehbvzzsx4t`;
- projeto: `prj_19vszrww3v57118bqhakg704vd`;
- workspace: `wsp_7er1pg0mn2cj0mwd7a8jf3sq4k`;
- relatório de conversão:
  `4c6382ed324b304fe1ee94082b81f3685eaf5973055592e37088aeed5e5b38a5`;
- identidade hash do verificador local:
  `fa14b31249e47a9882e516c270c3bc3b13c2de094bd9acfcce379b5f315ac604`;
- atestação: `att_6dy1g04n17nw1ygr3b6s5yw6jc`;
- SHA-256 da atestação:
  `ae9e966be9b19e22eec5a18ba5a700500bcb160dd40ef8eb912e9c32e0117617`;
- fingerprint lógico do candidato:
  `6c7f433a86998f3c26c461135265756d798f9a370d779bab4619d8343aa6655a`;
- instante confiável da prova: `unix:1787207200`;
- schema do candidato: 7.

As contagens verificadas foram 16 planos, 73 tarefas, zero submissões, zero
revisões, 275 eventos legados protegidos, um problema e quatro transições. Duas
execuções consecutivas retornaram a mesma atestação, enquanto o banco manteve
uma única linha em `attestations` e um único evento de auditoria. A sequência do
projeto avançou para 1, sem duplicação no replay.

## Limite da prova

A execução real deste relatório chamou o núcleo mecânico com um hash explícito
de verificador local, porque ainda não há runner CI protegido instalado. A
separação `Ci/Administrative/AttestationCreate` e o binding exato de escopo foram
comprovados separadamente pelos testes da capability e do protocolo
administrativo. Portanto esta evidência não deve ser descrita como identidade
de CI implantada nem como assinatura criptográfica.

O próprio artefato registra:

- `authentication=ci_capability` para o contrato que a rota de serviço exige;
- `signature=absent`;
- `decision=attested`;
- `manifest_state=staged`;
- `approval_created=false`;
- `cutover_performed=false`;
- `state=verifier_attested`.

Após a prova, o manifesto continuou com digest
`d9cba477a77a4c1b0a6cef9c9521e53fa678a3a16507ae951160d9dcdfeb27ec`,
estado `staged`, approvals vazias e timestamps de aprovação, cutover e rollback
nulos. A origem permaneceu no HEAD
`f7405872e980da505eadc53e40be6589a7507aa4`, com hash do status
`f3b371a69fbb42474abbc4bb6c76324f601896b99f3be7d34e0933c062013033`.

O próximo ato permitido é uma aprovação explícita e separada por operador, que
deve consumir esta atestação exata. Mesmo essa aprovação ainda não deve executar
cutover; troca de autoridade e instalação permanecem passos posteriores.

## Gate final

`make ci` passou com:

- 189 testes da biblioteca;
- 275 testes do binário aprovados e cinco provas reais ignoradas por exigirem
  locators fornecidos pelo operador;
- integrações 2+1+6 aprovadas;
- check com warnings negados no target `x86_64-unknown-linux-gnu`;
- contratos JSON e inventário T8.2 válidos;
- release sem paths físicos protegidos ou nomes globais previsíveis de log.

A inspeção final do candidato confirmou schema 7, `quick_check=ok`, zero
violações de foreign key, uma atestação e um evento de auditoria. A inspeção
final do manifesto confirmou `staged`, approvals vazias e ausência de timestamps
de aprovação, cutover e rollback.
