# Auditoria do commit automático legado

## Resultado

O AntDelirium antigo possui um checkpoint Git automático dentro do state root
do plugin, mas essa funcionalidade **não está correta nem ativa no fluxo global
atual**. Ela permanece compilada somente junto do runtime de rollback.

O sistema novo já substituiu a persistência de tarefa, submissão, revisão e
avanço por transações SQLite e journals append-only, e T9.6 acrescentou um
checkpoint Ed25519 do estado protegido. Ainda falta preservar a outra metade
da funcionalidade antiga: criar automaticamente, a cada avanço aceito, uma
referência recuperável que vincule a revisão aceita, a revisão da tarefa e o
estado do código que foi avaliado.

Nenhum repositório remoto é necessário para esse mecanismo. GitHub, GitLab,
push e branch protection são integrações opcionais fora do gate do produto.

## O que o runtime antigo realmente faz

[`commit_governance_state`](../src/Git_unitcore.rs) tenta descobrir ou criar um
repositório Git no próprio state root legado, executa `git add .` e tenta criar
um commit com a mensagem `Governance Checkpoint: Completed Task [...]`.

Antes do commit, [`capture_project_snapshot`](../src/Git_unitcore.rs) executa
`rsync -a --delete` para copiar o workspace para `project_state/`, excluindo
algumas pastas. Chamadas em [`Agent-Runtime_unitcore.rs`](../src/Agent-Runtime_unitcore.rs)
ocorrem depois de mutações legadas como `implemented=true`, revisão e nota.

Os hooks gerados por `config hooks init` são outra funcionalidade: eles apenas
bloqueiam certos commits, merges e pushes quando faltam metadados. O próprio
README gerado declara que a ativação é manual e que o motor não executa Git ou
shell. Portanto esses hooks não implementam o commit automático.

## Defeitos comprovados

1. Os status de `git add`, `git commit` e `rsync` são ignorados.
2. `commit_governance_state` devolve sucesso mesmo quando nenhum commit foi
   criado.
3. Todos os chamadores descartam o erro com `unwrap_or(())`, impedindo o
   operador de saber que o checkpoint falhou.
4. Não há preparação ou validação explícita de autor Git, árvore vazia,
   primeiro commit, lock concorrente ou repositório corrompido.
5. `git add .` inclui o state root inteiro, sem uma allowlist de artefatos e sem
   separar planos, segredos, logs ou arquivos temporários.
6. A cópia usa `rsync --delete`; uma resolução incorreta de raiz pode apagar o
   snapshot interno e a falha ainda assim ser ocultada.
7. A API legada oferece `git reset --hard` sobre o state root e retorna saída
   bruta do Git. Ela não deve voltar à superfície do agente.
8. Não existem testes específicos para sucesso, falha, concorrência,
   recuperação ou vínculo entre tarefa e commit.

## Estado no binário global

O roteador público nunca seleciona `LegacyAgentCliRollback` ou
`LegacyGuardRollback`. A CLI do agente aceita somente leitura das duas
projeções, `next-action` e submissão de evidência; `hierarchy.git`, checkbox,
revisão, grade e override são recusados.

Já estão implementados e validados:

- estado canônico transacional fora do projeto;
- evento append-only para instalação, submissão, revisão e avanço;
- revisão aceita obrigatória antes de avançar uma tarefa;
- capability de avanço separada, que o agente não possui;
- detecção de lacunas e rollback por checkpoint Ed25519 root-owned;
- reconstrução das projeções públicas sem confiar no JSON local.

Ainda não estão implementados:

- checkpoint automático imediatamente após cada avanço aceito;
- vínculo obrigatório entre checkpoint, `project_id`, `workspace_id`,
  `task_id`, revisão canônica e fingerprint do código avaliado;
- prova de que falha ao criar esse checkpoint impede o avanço ou o torna
  explicitamente incompleto e recuperável;
- recuperação equivalente ao snapshot histórico antigo sem copiar código para
  `plugins/` e sem revelar a raiz protegida ao agente.

## Contrato exigido para a substituição

A substituição deve ser local e controlada pela autoridade protegida:

1. o agente apenas envia evidência;
2. o revisor aceita ou rejeita a submissão;
3. a autoridade de transição confere auditoria atual e revisão aceita;
4. o avanço e seu evento canônico são persistidos atomicamente;
5. um checkpoint automático vincula a transição ao fingerprint do código
   avaliado e recebe confirmação assinada;
6. falha de checkpoint é observável e não pode ser convertida em sucesso;
7. recuperação exige capability administrativa e nunca é oferecida na API do
   agente;
8. nenhum `.git`, cópia de fonte, plano ou log administrativo é criado dentro
   do projeto governado ou de `plugins/`.

O formato interno poderá usar Git local protegido ou armazenamento
content-addressed, mas a implementação deve provar paridade de recuperação com
o checkpoint antigo antes que `Git_unit`, o runtime legado e a dependência
`gix` possam ser removidos.
