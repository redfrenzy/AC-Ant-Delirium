# Plano de Correção - Prioridade Decimal e Metadata de Violações

## Objetivo
Executar 2 correções estruturais no AC-Ant-Delirium:

1. Trocar o modelo de prioridade textual (`P0`, `P1`, ...) para prioridade decimal ordenável (`P0.0`, `P0.2`, `P1.0`, ...), com até 4 casas decimais.
2. Evoluir o schema dos artefatos (`violations_manifest.json` e `shadow_context.sarif.json`) para expor metadados explícitos de medição/remediação, evitando inferências erradas por outras AIs.

---

## Correção 1 - Prioridade Decimal (`Pn.xxxx`)

### Regra funcional
- Formato base: `P<bucket>.<ordem>`
- Exemplo: `P0.0000`, `P0.0019`, `P0.2000`, `P1.0000`
- Escala: até 4 casas decimais (0.0000 a 0.9999 por bucket).
- Ordenação: numérica pelo bucket e depois pelo decimal.

### Semântica
- Descrições de severidade continuam as mesmas (`error`, `warning`, `note`).
- O decimal define ordem explícita de correção dentro do mesmo bucket.
- Exemplo de precedência:
  - `P0.0000` corrige antes de `P0.2000`
  - `P0.0019` pode ser inserido entre prioridades já existentes sem quebrar o bucket.

### Pontos de implementação
0. **Fonte de prioridade por TOML (novo contrato)**
   - Arquivos:
     - `AC-Ant-Delirium/ant_delirium_config/type_1/default_rules.toml`
     - `AC-Ant-Delirium/ant_delirium_config/type_2/default_rules.toml`
     - `AC-Ant-Delirium/ant_delirium_config/type_3/default_rules.toml`
   - Decisão: cada regra passa a declarar `priority = 0.1234` (float até 4 casas).
   - Status atual: os 3 arquivos já foram atualizados com `priority` por regra.

1. **Classificador de prioridade**
   - Arquivo: `AC-Ant-Delirium/src/Logs/Logs_unitcore.rs`
   - Função: `classify_priority(...)`
   - Trocar de prioridade hardcoded por `rule_id` para prioridade lida de `default_rules.toml`.
   - Formatar saída como `P<bucket>.<ordem_4_casas>`.

2. **Contadores por prioridade**
   - Arquivo: `AC-Ant-Delirium/src/Logs/Logs_unitcore.rs`
   - Funções: `count_priorities(...)`, `count_entry_priorities(...)`
   - Decisão de compatibilidade:
     - manter agregação por bucket (`P0..P4`) para terminal resumo, e
     - adicionar ordenação detalhada por prioridade decimal nos artifacts.

3. **Ordenação de violações**
   - Arquivo: `AC-Ant-Delirium/src/Logs/Logs_unitcore.rs`
   - Garantir ordenação estável por prioridade decimal antes de escrever manifest/shadow.

4. **Testes**
   - Arquivo: `AC-Ant-Delirium/src/tests/Logs_test.rs`
   - Ajustar asserts para prioridade decimal.
   - Incluir caso que valide ordenação (`P0.0000` antes de `P0.0019`).

### Critério de aceite
- API/artifacts exibem prioridade decimal.
- Ordem de correção é determinística dentro do bucket.
- Resumo terminal continua legível (agregado por P0..P4 ou adaptado sem perda de clareza).

### Lista de tarefas (execução)
- [x] Adicionar `priority = ...` em todas as regras de `type_1/default_rules.toml`.
- [x] Adicionar `priority = ...` em todas as regras de `type_2/default_rules.toml`.
- [x] Adicionar `priority = ...` em todas as regras de `type_3/default_rules.toml`.
- [x] Adicionar validação de conflito de prioridade no preflight do motor (duas rules com o mesmo `priority`).
- [x] Fazer o conflito abortar a patrulha e gerar erro explícito pedindo decisão manual no TOML.
- [x] Implementar resolução de prioridade por regra em `Logs_unitcore.rs` (usando nó da regra no hierarchy carregado).
- [x] Definir fallback quando `priority` ausente (ex.: manter mapeamento antigo).
- [x] Atualizar `to_violation_manifest` e `to_shadow_context` para expor prioridade decimal.
- [ ] Atualizar ordenação por prioridade decimal antes da emissão dos artifacts.
- [x] Ajustar `Logs_test.rs` para validar prioridade decimal + ordenação.
- [x] Rodar suíte interna (`cargo test`) e fixar regressões.

---

## Correção 2 - Metadata explícita de medição e remediação

### Problema atual
Hoje o consumidor do JSON infere demais pelo `rule_id`, `message` e `observed_in`.  
Isso levou à inferência errada de que `logical_density` exigia quebrar em arquivos, quando muitas vezes a remediação correta é fragmentar função/impl.

### Estratégia
Evoluir schema com campos novos **opcionais** (backward-compatible), sem remover os campos atuais.

### Campos propostos (por violação)
- `measurement_scope`: `function | impl | struct | enum | file`
- `measured_target_kind`: `fn | impl_block | type | module`
- `measured_target_name`: nome exato do alvo
- `measured_target_span`: `{ start_line, end_line }`
- `metric_name`: ex. `logical_density`
- `metric_unit`: ex. `statements`
- `measured_value`: número observado
- `threshold_value`: limite ativo
- `count_method`: descrição curta do método de contagem
- `primary_offender_kind`: `single_function | impl_aggregate | file_aggregate`
- `remediation_hints`: lista ordenada de ações (do menor impacto para maior impacto)

### Pontos de implementação
1. **Modelo base**
   - Arquivo: `AC-Ant-Delirium/src/structs/Global_Params_struct.rs`
   - Struct: `AuditViolation`
   - Adicionar novos campos opcionais.
   - Status atual: **adiado para fase 2.1** para evitar quebra transversal; metadata está sendo serializada no log layer de forma backward-compatible.

2. **Preenchimento na rule de logical_density**
   - Arquivo: `AC-Ant-Delirium/src/rules/axiom_structural/Logical_Density_Axiom_unitcore.rs`
   - Na criação da violação, preencher:
     - alvo medido (`func.name`)
     - métrica (`statements`)
    - valor medido e limite
    - hint de remediação ordenado
   - Status atual: **implementado no layer de serialização** via derivação por `rule_id + message` para `logical_density`.

3. **Serialização no manifest**
   - Arquivo: `AC-Ant-Delirium/src/Logs/Logs_unitcore.rs`
   - Função: `to_violation_manifest(...)`
   - Incluir os novos campos no JSON.
   - Status atual: **implementado**.

4. **Serialização no shadow_context**
   - Arquivo: `AC-Ant-Delirium/src/Logs/Logs_unitcore.rs`
   - Função: `to_shadow_context(...)`
   - Incluir os novos campos em `results`.
   - Status atual: **implementado**.

5. **Testes de schema**
   - Arquivo: `AC-Ant-Delirium/src/tests/Logs_test.rs`
   - Adicionar asserts para presença dos novos campos quando existirem.
   - Garantir que regras antigas sem metadata específica continuam serializando normalmente.
   - Status atual: **implementado** (`logical_density_metadata_is_serialized_in_artifacts`).

### Critério de aceite
- Consumidor externo consegue identificar explicitamente:
  - o que foi medido,
  - em qual escopo foi medido,
  - e qual remediação vem primeiro.
- Redução de inferência ambígua em `logical_density`.

---

## Ordem sugerida de execução
1. Implementar prioridade decimal.
2. Ajustar testes de logs para prioridade decimal.
3. Evoluir `AuditViolation` com metadados opcionais.
4. Popular metadados em `logical_density`.
5. Expandir serialização `manifest` + `shadow_context`.
6. Atualizar testes de schema/artefatos.
7. Gerar binário novo e validar via `cargo check` no projeto consumidor.

---

## Observações de compatibilidade
- Não remover campos já existentes dos artifacts.
- Adicionar novos campos como opcionais para evitar quebra em pipelines atuais.
- Manter a leitura humana do terminal objetiva, com detalhe avançado no JSON.

## Regra adicional de governança (prioridade única por rule)
- Cada seção de rule em `default_rules.toml` deve possuir `priority` único.
- Se houver duplicidade (mesmo valor em duas ou mais rules), o motor gera violação crítica e interrompe a patrulha.
- Motivo: sem unicidade, a ordem de correção deixa de ser determinística, enfraquecendo a fila de remediação dirigida por prioridade decimal.
