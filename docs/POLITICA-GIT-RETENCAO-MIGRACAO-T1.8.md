# Política Git, retenção, migração e rollback

Tarefa: T1.8  
Versão da decisão: `lifecycle-v1`  
Schema de evidência: `migration-manifest-v1`

## Resultado

Esta política fecha quatro decisões:

1. as projeções em `guard/` são encontráveis, locais e descartáveis, mas não entram no histórico Git por padrão;
2. state roots governados nunca são apagados automaticamente;
3. migração exige operador autorizado e verificação independente;
4. cutover nunca remove o original e mantém rollback mínimo de 30 dias.

## Política Git de `guard/`

### Regra padrão

`guard/to-do_list.json` e `guard/governance_error.json` são derivados do estado protegido. Commitá-los criaria diffs ruidosos, conflitos entre worktrees e histórico de tarefas que não é autoridade. Portanto eles ficam ignorados por padrão.

O AntDelirium não altera automaticamente:

- `.gitignore` da raiz;
- `.git/info/exclude`;
- `core.excludesFile` ou configuração Git global/local;
- index com `git add`, `git rm --cached`, `assume-unchanged` ou `skip-worktree`.

Quando o projeto está em Git e nenhuma regra existente ignora as projeções, o writer pode criar somente:

```gitignore
/to-do_list.json
/governance_error.json
/.gitignore
```

em `guard/.gitignore`. Os padrões são relativos à própria pasta e ignoram exatamente os dois JSONs e o arquivo gerado. Não se usa `*`, `guard/**` nem outro wildcard que esconda arquivos inesperados.

### Comportamento diante do estado existente

| Estado encontrado | Ação |
|---|---|
| regra existente já ignora ambos os JSONs | preservar; não criar nem editar outra regra |
| `guard/.gitignore` inexistente e projeções não ignoradas | criar o arquivo mínimo acima atomicamente |
| `guard/.gitignore` existente | nunca sobrescrever; validar e respeitar a decisão do projeto |
| projeção já rastreada pelo Git | nunca desindexar automaticamente; bloquear rollout protegido até operador corrigir |
| audit-only com projeção rastreada | atualizar por compatibilidade e emitir aviso único ao operador |
| arquivo inesperado em `guard/` | preservar, não ignorar e registrar desvio; nunca apagar automaticamente |
| projeto sem Git | não criar `.gitignore`; gerar somente os dois JSONs |

O operador pode versionar intencionalmente o `guard/.gitignore` mínimo. O AntDelirium nunca versiona as projeções nem assume consentimento a partir de um repositório limpo.

### Conteúdo permitido

Em operação normal, `guard/` contém:

- `to-do_list.json`;
- `governance_error.json`;
- opcionalmente o `.gitignore` mínimo.

Nenhum binário, plano, histórico, login, TOML, token, socket, script ou arquivo de estado é permitido. A presença de outro item é desvio de configuração, não autorização para removê-lo.

## Classes de state root

| Classe | Conteúdo | Retenção padrão |
|---|---|---|
| provisório vazio | bootstrap sem plano, submissão ou revisão | elegível para quarentena após 30 dias inativo |
| enrollment ativo | projeto governado | indefinida enquanto ativo |
| migration staging | cópia ainda não aprovada | mínimo 30 dias após abort/expiração |
| retired/órfão governado | estado com plano, evento ou evidência | mínimo 180 dias e nunca purge automático |
| rolled back | estado novo preservado depois do retorno ao legado | mínimo 180 dias e nunca purge automático |
| archive de origem legada | snapshot original pós-cutover | mínimo 30 dias online e depois arquivo recuperável sem purge automático |

## Detecção de inatividade e órfãos

Ausência momentânea de path não significa órfão: volume pode estar desmontado, projeto movido ou worktree temporariamente ausente.

Um root só vira candidato quando:

- não há capability, sessão, lease, migration ou writer ativo;
- não houve observação válida no período;
- locators falharam em observações separadas por pelo menos 30 dias;
- não há rebind pendente;
- o registro não está sob hold de operador, incidente ou auditoria.

O candidato é marcado, nunca removido. Reaparecimento válido cancela a candidatura sem criar outro projeto.

## Quarentena e purge

### Estado provisório vazio

Pode ser movido logicamente para quarentena após 30 dias e ficar mais 30 dias recuperável. Purge automático é permitido somente se uma transação confirmar simultaneamente:

- zero planos, tarefas, submissões, reviews, attestations e eventos além do bootstrap;
- nenhum enrollment ou capability promovida;
- nenhuma referência por projeto/workspace ativo;
- política do operador permitindo purge de provisórios.

Sem todos esses critérios, o root é preservado.

### Estado governado

Qualquer root que já recebeu plano, tarefa, submissão, review, attestation, migration ou evento autoritativo:

- nunca é apagado automaticamente;
- após `retire`, permanece online por no mínimo 180 dias;
- exige snapshot consistente, hash, backup verificado e evento de archive;
- purge exige capability administrativa, alvo explícito, motivo e confirmação separada;
- ausência de política explícita significa retenção indefinida.

Mover para archive ou lixeira protegida é preferido a delete. O sistema nunca executa remoção recursiva baseada em glob, path recebido do agente ou variável não resolvida.

## Autoridades da migração

| Ação | Autoridade necessária |
|---|---|
| inventário e stage | operador com `state.admin` |
| relatório de paridade | verificador independente ou CI protegido |
| aprovação de cutover | operador mais attestation do verificador |
| execução do cutover | serviço sob operação administrativa autenticada |
| rollback por gate objetivo durante a janela | serviço com autorização pré-aprovada e evento imediato |
| rollback manual | operador com `state.admin` |
| archive do legado | operador após fim da janela e nova verificação |
| purge | operador, confirmação separada e backup comprovado |

Agente, wrapper, revisor de tarefa e JSON do projeto não aprovam migração, rollback, archive ou purge.

## Estados da migração

```text
inventoried
    -> staged
    -> validated
    -> approved
    -> cutover
    -> verified
    -> archived

staged/validated -> aborted
cutover/verified -> rolled_back
```

Transições são monotônicas e auditadas. `rollback` não apaga nem reescreve o evento de cutover.

## Fluxo de migração

1. operador identifica projeto e origem legada fora do protocolo do agente;
2. inventário read-only registra schemas, permissões, contagens e hashes;
3. origem recebe snapshot consistente ou janela de freeze;
4. cópia vai para staging protegido, sem remover ou editar a origem;
5. todos os JSONs são validados antes da importação;
6. paths são normalizados para IDs opacos e referências relativas;
7. planos, tarefas, problemas, transições, submissões e histórico são contados e comparados;
8. SQLite staged executa integrity check, migrations e projeção dry-run;
9. verificador independente produz attestation sobre hashes, contagens e testes;
10. operador aprova exatamente `project_id`, `migration_id`, hash de origem, hash staged e rollback point;
11. serviço faz cutover transacional do ponteiro canônico;
12. compilação, projeções, submissão, revisão e avanço são testados;
13. origem permanece congelada e recuperável durante a janela;
14. archive só ocorre depois do gate e nunca equivale a purge.

Nenhum passo altera permissões da origem antes de existir snapshot e rollback testado. Dados, formato, integração Cargo e remoção do legado não são mudados no mesmo commit operacional.

## Manifesto de migração

`migration-manifest-v1` registra IDs, hashes, contagens, versões, estado, approvals, rollback deadline e evidência. Ele não contém paths físicos; a resolução fica em tabela administrativa separada.

O manifesto é protegido e append-audited. A cópia apresentada ao agente, se alguma for necessária, contém apenas resultado genérico e nunca a origem/destino.

## Cutover e janela de rollback

Janela mínima: 30 dias depois do cutover e pelo menos um ciclo completo de validação do projeto. O operador pode aumentar, nunca reduzir abaixo disso para a primeira migração do AutoBrowser.

Durante a janela:

- o backend novo é o único writer canônico;
- o legado fica congelado, não existe dual-write;
- cada evento pós-cutover permanece no banco novo;
- o snapshot legado, manifest, hashes e integração anterior ficam recuperáveis;
- remoção do binário/guard legado espera o gate posterior definido no checklist.

### Gatilhos objetivos de rollback

- hash/contagem diverge da aprovação;
- plano, tarefa, histórico ou transição desaparece;
- projeção revela path/estado proibido;
- autorização permite mutação indevida;
- compilação ou API essencial perde paridade;
- storage novo falha em integrity/recovery;
- operador declara incidente.

### Execução do rollback

1. bloquear novas mutações e drenar requests ativos;
2. tirar snapshot consistente do banco novo;
3. preservar todos os eventos posteriores ao cutover;
4. verificar o rollback point legado;
5. trocar integração/canonical pointer para o modelo anterior;
6. validar Cargo, tarefa e API legada necessária;
7. marcar migration como `rolled_back` e registrar motivo;
8. manter o estado novo recuperável por pelo menos 180 dias;
9. reconciliar ou arquivar explicitamente eventos pós-cutover; nunca descartá-los silenciosamente.

Rollback não tenta fazer merge automático de dois writers. Se o replay compatível não for comprovado, o delta permanece em quarentena para decisão do operador.

## Política específica da primeira migração do AutoBrowser

- a raiz legada de planos continua canônica até `validated` e `approved`;
- o agente perde acesso à raiz legada antes do rollout protegido, mas o operador mantém acesso;
- staging usa cópia, nunca move a origem;
- SQLite só vira canônico no cutover aprovado;
- original permanece congelado durante a janela mínima de 30 dias;
- depois da janela, pode ser arquivado de forma recuperável, nunca apagado automaticamente;
- nenhuma projeção, erro ou binário distribuído contém o path físico da origem ou do archive.

## Spike Git executado

Num repositório temporário, o `.gitignore` autocontido com os três padrões exatos:

- manteve `.gitignore`, `to-do_list.json` e `governance_error.json` ignorados;
- não alterou configuração nem `.gitignore` raiz;
- deixou `guard/unexpected.txt` visível como untracked.

O teste rejeitou a alternativa `*`, pois ela esconderia qualquer conteúdo inesperado colocado na pasta.

## Consequências para implementação

- T5.1 implementa detecção de regras existentes e criação atômica do ignore mínimo.
- T5.6 trata item inesperado como desvio sem apagá-lo.
- T7 implementa quarantine/retire/archive como estados, não remoção direta.
- T8 usa `migration-manifest-v1`, approvals e rollback point.
- T8.9 preserva a origem por no mínimo 30 dias.
- T10 só remove integração legada depois dos gates G8/G9 e sem purgar dados.

