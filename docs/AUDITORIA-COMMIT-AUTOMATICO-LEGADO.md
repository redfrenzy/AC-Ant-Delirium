# Auditoria do commit automático legado

## Resultado

O AntDelirium antigo possui um backup Git automático dentro do state root do
plugin. Essa arquitetura é deliberada: o projeto governado não precisa possuir
`.git`; o AntDelirium mantém sua própria cópia versionada do código junto do
estado privado. A funcionalidade permanece compilada somente no runtime de
rollback e ainda não foi portada corretamente para o fluxo global.

O sistema novo já substituiu a persistência de tarefa, submissão, revisão e
avanço por transações SQLite e journals append-only, e T9.6 acrescentou um
checkpoint Ed25519 do estado protegido. Ainda falta preservar a outra metade
da funcionalidade antiga: criar automaticamente uma referência recuperável no
baseline e em cada transição canônica relevante — submissão, revisão e avanço
— vinculando tarefa, revisão e estado do código avaliado. Esse histórico também
deve fornecer os diffs e resumos usados nas descrições e contextos das tarefas
JSON.

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

O histórico interno também é consultado por `hierarchy.context.*`: o motor
localiza o commit associado à tarefa e renderiza o diff de `project_state/`
sem exigir Git no projeto original. Essa é a fonte correta para contexto e
recuperação.

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
5. `gix::discover` pode subir para um repositório pai em vez de exigir o
   repositório interno exato; isso viola a separação quando o projeto possuir
   Git por decisão própria.
6. `git add .` inclui o state root inteiro, sem uma allowlist de artefatos e sem
   separar planos, segredos, logs ou arquivos temporários.
7. A cópia usa `rsync --delete`; uma resolução incorreta de raiz pode apagar o
   snapshot interno e a falha ainda assim ser ocultada.
8. O resumo gravado em `implementation_context` executa `git diff HEAD --stat`
   no workspace original, não no backup interno. Em um projeto sem `.git`, o
   Git retorna erro, mas o status é ignorado e o texto pode declarar falsamente
   que nenhum arquivo foi modificado.
9. A API legada oferece `git reset --hard` sobre o state root e retorna saída
   bruta do Git. Ela não deve voltar à superfície do agente.
10. Não existem testes específicos para sucesso, falha, concorrência,
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

- backup e commit Git automáticos no baseline e nas transições de submissão,
  revisão e avanço;
- vínculo obrigatório entre checkpoint, `project_id`, `workspace_id`,
  `task_id`, revisão canônica e fingerprint do código avaliado;
- prova de que falha ao criar esse checkpoint impede o avanço ou o torna
  explicitamente incompleto e recuperável;
- repositório Git privado e snapshot de código equivalentes ao backup histórico
  antigo, fora do projeto e sem revelar a raiz protegida ao agente;
- geração de `implementation_context` e contexto de revisão a partir do diff
  entre commits do backup interno, inclusive quando o projeto não possui Git.

## Contrato exigido para a substituição

A substituição deve ser local e controlada pela autoridade protegida:

1. o agente apenas envia evidência;
2. o revisor aceita ou rejeita a submissão;
3. a autoridade de transição confere auditoria atual e revisão aceita;
4. o avanço e seu evento canônico são persistidos atomicamente;
5. o workspace é copiado para a árvore de backup privada por uma operação
   limitada, validada e sem seguir symlinks inesperados;
6. commits automáticos no Git interno vinculam baseline, submissão, revisão e
   avanço ao fingerprint do código avaliado e recebem confirmação assinada;
7. descrições e contextos JSON são derivados do diff desse Git interno, nunca
   de `.git` no projeto governado;
8. o repositório interno é aberto por raiz exata e nunca descoberto subindo
   diretórios;
9. falha de cópia, indexação ou commit é observável e não pode ser convertida
   em sucesso;
10. recuperação exige capability administrativa e nunca é oferecida na API do
   agente;
11. nenhum `.git` é criado no projeto governado; o `.git`, a cópia da fonte e
   o histórico ficam somente no state root privado do AntDelirium;
12. capabilities, segredos, temporários e logs sensíveis ficam fora da árvore
   versionada por uma allowlist fechada.

O formato obrigatório é um Git local privado, equivalente ao backup original.
`gix` deve ser concluído ou substituído por outra implementação Git validada;
ele não deve ser removido apenas porque o runtime legado será retirado. A
implementação global precisa provar backup, diff, contexto e recuperação antes
que as funções antigas possam sair.
