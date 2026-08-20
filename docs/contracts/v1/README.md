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

Todos usam JSON Schema Draft 2020-12 e `additionalProperties: false`. Eles são contratos de domínio e API, não autorização, schema SQL nem descrição do transporte.

O runtime legado ainda não produz estes formatos. Essa compatibilidade será implementada pelas tarefas T2.4 a T2.7; até lá, estes schemas servem como referência normativa e gate de testes.
