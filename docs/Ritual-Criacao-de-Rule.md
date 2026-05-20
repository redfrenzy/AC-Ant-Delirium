# Ritual de Criacao de Rule (Ant-Delirium)

Este documento define o processo oficial para criar uma rule nova sem ambiguidade de diagnostico, sem acoplamento indevido no motor, e com cobertura completa de TOML, API, logs e testes.

## Objetivo do ritual

- Separar claramente:
  - erro de **autorizacao/politica** (ex.: `allowed_crates`)
  - erro de **contrato de implementacao** (ex.: `strict_wrapper_boundary`)
- Garantir que cada rule tenha:
  - configuracao propria no TOML
  - mensagens de correcao proprias
  - exemplos de erro nos 3 tipos (`type_1`, `type_2`, `type_3`)
  - entrada propria na API de regras
  - testes unitarios da rule

---

## Fase 0 - Premissas obrigatorias

1. Nao editar `src/engine/*` para adaptar comportamento de uma rule nova, salvo necessidade real de novo tipo/struct compartilhado.
2. Nao reciclar parametros de outra rule por conveniencia.
3. Nao usar `rule_id` generico se a semantica for diferente.
4. Antes de codar, classificar a rule em um dos 4 dominios:
   - `axiom_identity`
   - `axiom_structural`
   - `axiom_pedagogical`
   - `axiom_sanitization`

---

## Fase 1 - Simular erro primeiro (3 tipos)

Criar os cenarios de falha antes da implementacao final.

Local:
- `src/exemples_error/type1/...`
- `src/exemples_error/type2/...`
- `src/exemples_error/type3/...`

Para rules de sanitizacao, usar:
- `src/exemples_error/type1/sanitization_workspace/`
- `src/exemples_error/type2/sanitization_workspace/`
- `src/exemples_error/type3/sanitization_workspace/`

Padrao:
1. Criar um arquivo erro em cada tipo que viole explicitamente a regra.
2. Se a regra tiver modo "aceitavel", criar tambem caso valido em testes unitarios.
3. Nomear conforme DNA local (`_unit`, `_unite`, `_unidade`, etc.).

---

## Fase 2 - Definir contrato no TOML (type_1 primeiro)

## 2.1 Hierarchy (fonte semantica dos parametros)

Arquivo:
- `ant_delirium_config/type_1/default_hierarchy.toml`

Regra:
- Declarar parametros da rule em ramo proprio e semantico.
- Evitar depender de parametros de outra rule.

Exemplo de contrato proprio:
- `wrapper_boundary_contract = "explicit_ac_alias"`

## 2.2 Rules (binding da rule para parametros)

Arquivo:
- `ant_delirium_config/type_1/default_rules.toml`

Regra:
- Criar secao da rule e apontar para os parametros corretos.
- Nao reaproveitar `libres`/`restricted_tokens` se a semantica for diferente.

Exemplo:
- `[hierarchy.dependency_governance.strict_wrapper_boundary]`
- `operation_zone`
- `wrapper_boundary_contract`

## Diferenca entre `default_rules.toml` e `default_hierarchy.toml`

- `default_hierarchy.toml`:
  - define os valores/contratos da politica (o "que" da governanca).
- `default_rules.toml`:
  - define quais parametros cada rule resolve e aplica (o "como" da regra le a politica).

---

## Fase 3 - Expandir para type_2 e type_3

Replicar o mesmo contrato semantico para:
- `ant_delirium_config/type_2/default_hierarchy.toml`
- `ant_delirium_config/type_2/default_rules.toml`
- `ant_delirium_config/type_3/default_hierarchy.toml`
- `ant_delirium_config/type_3/default_rules.toml`

Objetivo da variacao:
- Forcar abstracao de resolucao no motor.
- Evitar regra estaticamente acoplada a um unico dialeto de taxonomia.

Tambem atualizar bootstrap de novos projetos:
- `get_toml_for_new/default_hierarchy.toml`
- `get_toml_for_new/default_rules.toml`

---

## Fase 4 - Escolher o tipo de rule (pasta + struct)

As rules entram por pasta de dominio:
- `src/rules/axiom_identity/`
- `src/rules/axiom_structural/`
- `src/rules/axiom_pedagogical/`
- `src/rules/axiom_sanitization/`

Arquivos minimos:
1. `Minha_Rule_unit.rs`
2. `Minha_Rule_unitcore.rs`

Padrao:
- `*_unit.rs`: adaptador da rule (id, name, jurisdiction, execute)
- `*_unitcore.rs`: logica de patrulha (`patrol(ctx)`)

Estruturas relevantes:
- `src/structs/Rules_Catalog_struct.rs`
  - `GovernanceAxiom` / `AuditContext`
- `src/structs/Project_Context_struct.rs`
  - arquivos indexados (`known_paths`, `files`)
- `src/structs/Global_Params_struct.rs`
  - config global carregada no runtime

---

## Fase 5 - Conectar rule ao catalogo

1. Registrar no modulo do dominio (`src/rules/axiom_*/mod.rs`).
2. Garantir que `get_full_axioms()` inclua a nova rule.
3. Conferir que `id()` da rule e estavel e unico.

---

## Fase 6 - API e documentacao da rule

Atualizar:
- `src/rules/Rule_Docs_unit.rs`

Campos minimos:
- `id`
- `name`
- `category`
- `summary`
- `detection_model`
- `failure_semantics`
- `remediation`
- `example`

Regra de qualidade:
- `allowed_crates` e `strict_wrapper_boundary` devem ter texto de remedicao distinto.
- A mensagem precisa indicar o passo de correcao sem ambiguidade.

---

## Fase 7 - Logs de correcao proprios

Cada rule deve ter `rule_id` especifico e mensagens especificas.

Onde validar:
- `src/Logs/Logs_unitcore.rs`
  - classificacao de prioridade
  - agrupamento (`grouped_rule_key`)
  - serializacao JSON
  - resumo terminal

Checklist:
1. O `rule_id` da rule nova nao conflita com outro.
2. `message` nao reaproveita semantica de outra rule.
3. `quick_fix` orienta exatamente o que fazer para aquela rule.
4. JSON final separa claramente os tipos de violacao.

---

## Fase 8 - Integracao com Swarm e Search (sem editar engine desnecessariamente)

Interacao real:
- Swarm:
  - `src/engine/Swarm_Engine_unit.rs`
  - executa todos axiomas para cada arquivo do `ProjectContext`.
- Search:
  - `src/engine/Search_Engine_unitcore.rs`
  - resolve jurisdicao e varre arquivos fisicos.

Contrato de jurisdicao por rule:
- no `*_unit.rs`, `resolve_jurisdiction(...)` tipicamente usa:
  - `SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID)`

Quando editar engine:
- Somente se precisar novo comportamento transversal (novo index global, novo modo de scan, novo tipo de binding).
- Nao editar engine para "encaixar" uma unica rule mal desenhada.

---

## Fase 9 - Testes obrigatorios

1. `cargo check --target x86_64-unknown-linux-gnu`
2. Testes da rule:
   - adicionar em `src/tests/Sanitization_Axiom_test.rs` (ou suite correspondente)
3. Validar 3 casos minimos:
   - erro esperado
   - caso valido
   - erro de configuracao (parametro ausente/invalido)

Comando sugerido:
- `cargo test --target x86_64-unknown-linux-gnu <nome_da_rule> -- --nocapture`

---

## Fase 10 - Gate final antes de usar no projeto real

1. Build do binario:
   - `cargo build --release --target x86_64-unknown-linux-gnu --bin delirium_cli`
2. Troca no plugin (fluxo protegido do projeto).
3. Rodar auditoria no projeto real (`cargo check` no workspace alvo).
4. Confirmar:
   - contagem de erros coerente
   - mensagens separadas por rule
   - `violations_manifest.json` com `rule_id` e `quick_fix` corretos

---

## Anti-padroes proibidos

1. Reusar parametro de outra rule por atalho.
2. Criar rule sem exemplo em `src/exemples_error`.
3. Criar rule sem docs na API.
4. Corrigir falso negativo afrouxando regra.
5. Alterar `engine` quando o problema e local da rule.

---

## Checklist curto (operacional)

1. Simule erro em `type1/type2/type3`.
2. Defina contrato no `default_hierarchy.toml` (type_1).
3. Faça binding no `default_rules.toml` (type_1).
4. Expanda para `type_2` e `type_3`.
5. Atualize `get_toml_for_new`.
6. Crie `*_unit.rs` e `*_unitcore.rs` no dominio correto.
7. Registre no modulo de regras.
8. Atualize `Rule_Docs_unit.rs`.
9. Garanta `rule_id` e `quick_fix` especificos.
10. Escreva testes e rode `cargo check` + `cargo test`.
11. Atualize binario e valide no projeto real.

---

## Validacao final da API (obrigatoria)

Antes de considerar a rule concluida, validar a API diretamente no binario local para garantir que:
- o texto da rule esta correto
- os caminhos de taxonomia estao corretos
- nao houve mistura semantica entre rules diferentes

Comandos:

```bash
cd "/home/dracker/Antigravity Projects/playground/aesthetic_core_project/AC-Ant-Delirium"
./target/x86_64-unknown-linux-gnu/release/delirium_cli --api hierarchy.rule.allowed_crates
./target/x86_64-unknown-linux-gnu/release/delirium_cli --api hierarchy.rule.strict_wrapper_boundary
./target/x86_64-unknown-linux-gnu/release/delirium_cli --api hierarchy.permissions.libres
./target/x86_64-unknown-linux-gnu/release/delirium_cli --api hierarchy.permissions.wrapper_boundary_contract
```

Checklist de coerencia:
1. `allowed_crates` deve falar de autorizacao de crates e allow-list.
2. `strict_wrapper_boundary` deve falar de contrato de boundary (`AC_*`, sem wildcard/full forwarding).
3. `hierarchy.permissions.libres` deve conter apenas a lista de crates permitidas.
4. `hierarchy.permissions.wrapper_boundary_contract` deve conter apenas o contrato da boundary.
5. Se qualquer comando retornar texto de outra rule, existe contaminacao semantica e a entrega nao esta concluida.
