# Schemas normativos da T1.4

Os arquivos deste diretório congelam a primeira versão das fronteiras de dados e transporte:

- `canonical-state-v1.schema.json`: snapshot lógico privado do estado canônico;
- `submission-v1.schema.json`: evidência proposta sem poder de aprovação;
- `audit-event-v1.schema.json`: evento interno append-only;
- `agent-projection-v1.schema.json`: união discriminada dos dois JSONs visíveis em `guard/`.
- `limited-protocol-v1.schema.json`: envelopes aceitos no listener limitado.
- `admin-protocol-v1.schema.json`: envelopes aceitos somente no listener administrativo.
- `public-error-v1.schema.json`: erro sanitizado, código estável e exit code.
- `migration-manifest-v1.schema.json`: hashes, contagens, approvals e rollback de uma migração.
- `migration-attestation-request-v1.schema.json`: escopo opaco e digest exato que um CI verificador deve revalidar.
- `migration-verifier-attestation-v1.schema.json`: evidência privada produzida sob capability independente, sem aprovação implícita.
- `migration-attestation-receipt-v1.schema.json`: attestation e seu SHA-256 para a futura decisão do operador.
- `migration-approval-request-v1.schema.json`: decisão do operador vinculada ao candidato, attestation, hashes e rollback point exatos.
- `migration-approval-receipt-v1.schema.json`: recibo path-free da aprovação, ainda sem cutover ou deadline antecipado.
- `initial-capability-provisioning-v1.schema.json`: credencial de bootstrap
  entregue uma única vez pelo supervisor para criar os primeiros grants sem
  transformar o primeiro cliente em administrador.
- `legacy-source-inventory-v1.schema.json`: ledger read-only de paths relativos,
  hashes, modos, ownership, links, validação JSON e contagens anteriores ao staging.
- `legacy-stage-receipt-v1.schema.json`: recibo sem paths da cópia publicada no
  staging protegido.
- `legacy-normalization-request-v1.schema.json`: digest semântico da origem e
  bindings hash→ID opaco entregues somente pelo operador autenticado.
- `legacy-normalization-receipt-v1.schema.json`: prova sem paths de equivalência
  entre origem revalidada e derivação normalizada do staging.
- `physical-reference-bindings-v1.schema.json`: tabela administrativa protegida
  que persiste somente hash do valor legado e ID `ref_`, nunca o path original.
- `legacy-dry-run-request-v1.schema.json`: seleção administrativa path-free de
  uma geração exata de origem, staging, normalização e semântica.
- `legacy-dry-run-report-v1.schema.json`: diferenças bloqueadoras entre as
  contagens legadas e um SQLite sombra vazio, sem aprovação ou cutover.

Todos usam JSON Schema Draft 2020-12 e `additionalProperties: false`. Eles são contratos de domínio e API, não autorização, schema SQL nem descrição do transporte.

O serviço novo já produz e valida os contratos implementados pelas Ondas 2–8.
O runtime legado preservado para rollback não é autoridade desses formatos;
contratos ainda sem writer continuam sendo referência normativa e gate de testes.
