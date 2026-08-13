# Contratos de dados separados e versionados

Tarefa: T1.4  
Estado: decisão arquitetural da Onda 1  
Versões iniciais: `canonical-state-v1`, `submission-v1`, `audit-event-v1` e `agent-projection-v1`

## Resultado

O AntDelirium terá quatro contratos independentes. Uma alteração em um deles não altera implicitamente os demais:

| Contrato | Autoridade | Consumidores | Representação inicial |
|---|---|---|---|
| estado canônico | serviço protegido | serviço e operador autorizado | SQLite, validado pelo modelo lógico `canonical-state-v1` |
| submissão | agente, revisor ou CI como proponente; serviço como registrador | serviço e revisor atribuído | envelope imutável `submission-v1` |
| auditoria | serviço protegido | serviço, operador e verificador autorizado | evento append-only `audit-event-v1` |
| projeção do agente | derivada, sem autoridade | wrapper e agente | `guard/to-do_list.json` ou `guard/governance_error.json`, ambos `agent-projection-v1` |

Os schemas normativos estão em [`contracts/v1/`](./contracts/v1/README.md). Os exemplos neles são lógicos: não definem o schema SQL, o transporte ou o algoritmo final de IDs.

## Invariantes comuns

1. Cada documento carrega sua própria versão em um campo constante; não existe uma versão global que acople os quatro contratos.
2. IDs são opacos. O formato definitivo, colisões, worktrees e projetos movidos pertencem à T1.5.
3. Cwd, nome do executável, caminho físico e conhecimento de um ID não concedem autorização.
4. Caminhos presentes em submissões ou projeções são relativos à raiz do projeto, usam `/` e não contêm `..`.
5. Campos desconhecidos são rejeitados em `v1`. Evolução aditiva exige uma versão nova ou um campo extensível explicitamente previsto.
6. Timestamps e IDs enviados pelo cliente nunca substituem valores atribuídos pelo serviço.
7. Nenhum contrato público contém state root, caminho do banco, plano físico, TOML, socket administrativo, credencial ou descritor.

## Estado canônico: `canonical-state-v1`

Este é um modelo lógico privado, não um arquivo JSON gravado ao lado do projeto. O serviço o materializa em SQLite e mantém na mesma transação:

- revisão monotônica do estado;
- plano e tarefa atuais por IDs opacos;
- estados internos de tarefa e revisão;
- última sequência auditável confirmada;
- geração conhecida das projeções.

O contrato não inclui paths físicos. O registro privado que resolve identidade para localização pertence à configuração administrativa e tem autorização distinta. Toda mutação canônica válida também insere seu evento de auditoria na mesma transação.

Versão de storage e versão lógica são diferentes:

- `PRAGMA user_version` seleciona migrations físicas do SQLite;
- `canonical-state-v1` seleciona as invariantes e os campos lógicos;
- alterar índices sem alterar semântica não força uma nova versão lógica;
- mudar significado, cardinalidade ou transição exige `canonical-state-v2` e migration explícita.

## Submissão: `submission-v1`

Uma submissão é proposta imutável e deduplicável. Ela pode conter mudanças de arquivo, resultados de comandos, digests de artefatos e observações textuais. Ela não pode conter:

- `implemented`, `task_reviewed`, `review_status`, `grade` ou `active_plan`;
- decisão de aprovação da própria submissão;
- SQL, path de storage, nome de tabela ou comando administrativo;
- caminho absoluto ou traversal.

`request_id` permite retry idempotente no escopo autenticado. O serviço atribui `submission_id`, horário de recebimento, fingerprint canônico e estado de processamento fora do payload apresentado pelo cliente.

## Auditoria: `audit-event-v1`

O evento é append-only, recebe sequência monotônica somente quando efetivamente inserido e registra ator, ação, resultado e correlação. O payload é interno e tipado pelo `event_kind`; clientes nunca fornecem SQL ou um evento canônico pronto.

Falhas de autorização também geram eventos, mas respostas públicas não confirmam a existência de recurso fora do escopo. Correções não reescrevem eventos anteriores: geram outro evento correlacionado.

## Projeção: `agent-projection-v1`

Uma única versão cobre dois artefatos discriminados por `artifact_kind`:

- `current_task`, destinado a `guard/to-do_list.json`;
- `governance_errors`, destinado a `guard/governance_error.json`.

A projeção de tarefa contém somente tarefa liberada, objetivo, ação permitida, critérios, paths relativos e instrução de submissão. A projeção de erros contém somente violações corrigíveis, referência pública da regra e localização relativa.

As projeções nunca contêm plano completo, tarefas futuras, contadores que revelem a fila, estados internos de aprovação, histórico ou localização administrativa. `source_revision` serve apenas para detectar projeção obsoleta; editar esse número não altera a autoridade.

## Compatibilidade com o manifesto legado

| Campo ou comportamento antigo | Destino em `v1` |
|---|---|
| `current_task.step_id` | `task.task_id` opaco |
| `current_task.task_prompt` | `task.objective` e `task.acceptance_criteria` mínimos |
| `copy_paste_command` que marca `implemented=true` | substituído por `submission.instruction`, que apenas submete evidência |
| `plan_id` físico e `plan_title` | não projetados |
| `complete_plan` | proibido na projeção |
| `implemented`, `task_reviewed`, `review_status`, explicações e grade | somente estado canônico protegido |
| `queue_status` com quantidade ou próximos itens | texto opaco, sem cardinalidade |
| comandos de instalação, override ou configuração | proibidos na projeção e na API do agente |

O writer legado não passa a satisfazer este contrato apenas renomeando o arquivo. A adaptação da T2.5 deve construir tipos separados e validar o objeto antes do rename atômico.

## Evolução e negociação

- O produtor declara exatamente uma versão por payload.
- O serviço mantém uma janela explícita de versões aceitas; versão ausente ou desconhecida falha antes de interpretar o corpo.
- Downgrade nunca ocorre silenciosamente.
- Migrations canônicas são monotônicas e administrativas.
- Submissões e eventos antigos permanecem legíveis por adaptadores puros; não são reescritos para parecer novos.
- Projeções podem ser regeneradas na versão suportada a partir do estado canônico.
- A versão escolhida no protocolo será negociada na T1.6; este documento não usa headers ou argumentos como autorização.

## Critérios de implementação derivados

1. T2.4 cria tipos e stores separados, sem um `serde_json::Value` compartilhado entre fronteiras.
2. T2.5 e T2.6 validam os dois artefatos contra `agent-projection-v1` e rejeitam qualquer path absoluto.
3. T2.3 persiste mutação e `audit-event-v1` na mesma transação.
4. T3.4 e T4.3 aceitam `submission-v1`, nunca flags de conclusão.
5. Testes devem provar que campos proibidos do manifesto legado não aparecem nas projeções.

