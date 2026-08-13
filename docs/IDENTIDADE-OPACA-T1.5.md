# Identidade opaca de projeto, workspace, plano e tarefa

Tarefa: T1.5  
Versão da decisão: `identity-v1`  
Estado: decisão arquitetural da Onda 1

## Resultado

Identidades canônicas não serão derivadas de nome, path, branch, remote, commit, conteúdo ou `DefaultHasher`. O serviço protegido emitirá identificadores aleatórios de 128 bits usando CSPRNG do sistema, codificados em Crockford Base32 minúsculo sem padding.

| Entidade | Formato canônico | Estabilidade |
|---|---|---|
| projeto governado | `prj_<26 caracteres>` | dura por toda a vida do registro, inclusive renome local |
| checkout/worktree | `wsp_<26 caracteres>` | dura enquanto a mesma instância de checkout existir |
| plano | `pln_<26 caracteres>` | não muda com título, filename ou revisão |
| tarefa | `tsk_<26 caracteres>` | não muda com conteúdo, status, revisão ou movimento interno |

Alfabeto permitido: `0123456789abcdefghjkmnpqrstvwxyz`. Prefixos tornam erros de tipo detectáveis sem revelar localização ou semântica.

Conhecer um ID não concede acesso. Toda leitura e mutação continua exigindo ator, capability, escopo, freshness e política definidos nas T1.1 e T1.2.

## `project-id` e `project-key`

`project_id` é a chave canônica do banco. `project-key` é somente o nome interno e congelado do diretório protegido:

```text
<slug-congelado>--<token-base32-completo>
```

Exemplo ilustrativo:

```text
testproject--6g4m2k8r9c1v7x3b5n0q2s4t6w
```

Regras do slug:

1. derivado uma vez do nome do workspace Cargo ou package raiz;
2. ASCII minúsculo, `a-z`, `0-9` e hífen;
3. separadores repetidos são colapsados;
4. tamanho entre 1 e 48; fallback `project`;
5. renomear o package não renomeia automaticamente o state root.

O sufixo usa os mesmos 128 bits de `project_id`, sem o prefixo `prj_`. O key permanece privado e nunca aparece em `guard/`, stdout, stderr ou protocolo do agente. O banco impõe unicidade tanto no ID quanto no key; colisão provoca nova geração dentro da transação, nunca acréscimo previsível baseado em path.

## Unidade governada

Um projeto representa um workspace Cargo ou, na ausência de `[workspace]`, o package raiz selecionado. Um repositório Git pode conter vários projetos governados.

O registro privado associa o projeto a:

- identidade da instância Git, quando houver;
- âncora do workspace/package relativa ao top-level Git;
- identidade de filesystem da âncora observada;
- aliases históricos de localização, somente administrativos.

Dois workspaces Cargo no mesmo monorepo recebem `project_id` diferentes. O mesmo workspace visto por worktrees do mesmo repositório compartilha `project_id`.

## Pistas de reencontro não autoritativas

O ID aleatório é autoridade; os dados abaixo servem apenas para reencontrar o registro privado:

### Projeto Git

1. o resolver de enrollment confiável obtém top-level, `git-dir` e `git-common-dir` sem executar Cargo recursivamente;
2. o serviço compara a observação atestada `(st_dev, st_ino)` do common-dir com o registro protegido;
3. combinar a instância Git encontrada com a âncora relativa do workspace Cargo;
4. confirmar que a raiz e o manifest observados pertencem à mesma unidade registrada.

Remote URL, branch, `HEAD`, primeiro commit e hash da árvore são apenas sinais de diagnóstico. Eles mudam ou podem ser iguais em clones e nunca escolhem sozinhos um registro.

### Projeto sem Git

O serviço registra `(st_dev, st_ino)` da raiz Cargo, manifest e fingerprint estrutural. O fingerprint ajuda na confirmação, mas não substitui a identidade aleatória.

### Hint vindo do projeto

`project_ref` ou qualquer JSON em `guard/` é hint não confiável. O serviço só o aceita depois de confirmar a associação privada e a capability. Como o daemon normal não monta o projeto, essa confirmação vem do enrollment feito pelo supervisor/operador confiável; o bootstrap automático do wrapper permanece provisório. Alterar ou copiar a projeção não move nem clona autoridade.

## `workspace-id` e `workspace-key`

`workspace_id` identifica uma instância de checkout, não um Cargo workspace conceitual. Seu diretório privado é:

```text
project_state/workspaces/wsp--<token-base32-completo>/
```

Regras:

- branch, `HEAD`, cwd e nome da pasta não participam do ID;
- worktree principal e cada worktree ligada recebem IDs distintos;
- worktrees ligadas compartilham o projeto porque têm o mesmo common-dir;
- `git-dir` específico, inode da raiz do checkout e registro privado distinguem worktrees;
- remover e recriar uma worktree cria outro `workspace_id`, mesmo na mesma branch;
- renome no mesmo filesystem preserva o ID após revalidação do inode;
- `workspace-key` nunca é projetado ao agente.

## `active_plan_id`

`active_plan_id` será foreign key nullable para um `plan_id` aleatório. Ele não será filename, path, título, sequência como `foundation:0.0` nem conteúdo de `active_plan` legado.

Na importação:

1. cada plano físico recebe `pln_...` dentro de uma transação;
2. a relação entre filename legado e `plan_id` fica somente no registro protegido;
3. o orquestrador passa a apontar para a foreign key;
4. paths não são copiados para projeções;
5. um plano removido é tombstonado; seu ID não é reutilizado.

O agente normalmente não precisa receber `active_plan_id`. Ele recebe apenas a tarefa atual. Interfaces autorizadas de operador ou revisor podem receber o ID opaco sem resolução física.

## `task-id`

Cada tarefa lógica recebe `tsk_...` na criação ou importação. O ID pertence à linha canônica, não ao texto ou ao arquivo afetado.

- editar prompt, critérios, status ou arquivos incrementa `task_revision`, mas preserva `task_id`;
- mover a tarefa dentro do mesmo plano preserva o ID;
- mover entre planos exige operação administrativa explícita, mas pode preservar o ID e registrar evento;
- apagar e recriar produz outro ID;
- IDs tombstonados nunca são reutilizados;
- `step_id`, path e hash de conteúdo legados são guardados somente como chaves de migração privadas;
- submissão sempre referencia `task_id` e `task_revision`, impedindo evidência para uma revisão obsoleta.

O gerador atual baseado em hash do path será apenas adaptador de importação. Ele não será promovido a identidade canônica.

## Colisão, geração e falha segura

1. o serviço solicita 128 bits ao CSPRNG do sistema;
2. codifica e tenta inserir com constraint `UNIQUE`;
3. colisão gera novo valor, no máximo oito tentativas;
4. indisponibilidade do CSPRNG ou oito colisões causam falha interna e evento administrativo;
5. é proibido fazer fallback para timestamp, PID, path, `DefaultHasher` ou contador público.

A geração ocorre no serviço. Wrapper e agente não escolhem IDs canônicos.

## Projetos movidos, copiados e clonados

| Situação | Resultado `identity-v1` |
|---|---|
| rename no mesmo filesystem | preserva projeto/workspace após inode e associação privada coincidirem |
| linked worktree | compartilha projeto e recebe workspace próprio |
| clone Git | projeto novo, mesmo que remote e commits sejam iguais |
| cópia de diretório | projeto novo; projeção copiada é ignorada como autoridade |
| movimento entre filesystems | não religa automaticamente |
| raiz Cargo movida dentro de monorepo | exige rebind quando a âncora registrada mudar |

O rebind entre filesystems é administrativo e deve:

1. nomear explicitamente registro antigo e nova raiz;
2. comprovar que a origem não está ativa ou declarar migração controlada;
3. comparar identidade Git, manifest e fingerprint esperado;
4. exigir capability administrativa e confirmação;
5. gravar evento de auditoria e alias tombstonado;
6. rejeitar ambiguidade em vez de escolher pelo conteúdo mais parecido.

## Descoberta do Cargo

A T2.2 implementará a descoberta, mas deve obedecer esta precedência:

1. manifest explicitamente selecionado pelo fluxo Cargo autorizado;
2. raiz do workspace que contém esse manifest;
3. package root quando não houver workspace;
4. erro limitado se houver múltiplas unidades igualmente válidas.

O algoritmo deve tratar package único, workspace virtual, workspace com package raiz e `--manifest-path` fora do cwd. A descoberta não pode iniciar outro `cargo` por dentro do wrapper nem usar cwd como identidade.

## Spike de validação

Um repositório temporário, uma linked worktree e um clone confirmaram:

- repositório e linked worktree resolvem o mesmo `git-common-dir` e o mesmo `(st_dev, st_ino)`;
- seus `git-dir` são distintos;
- o clone tem outro common-dir e outro inode;
- renomear o clone no mesmo filesystem preserva `(st_dev, st_ino)` e o Git continua resolvendo a nova localização.

Isso valida o uso dos inodes como pista forte local e também demonstra por que path, remote e conteúdo não podem ser a identidade.

## Consequências para implementação

- T2.1 guarda todos os locators físicos somente no `ProjectStateRoot` protegido.
- T2.2 implementa descoberta e lookup sem Cargo recursivo.
- T2.3 gera IDs dentro da transação SQLite e aplica unicidade/tombstones.
- T2.4 usa novos tipos para impedir troca entre IDs.
- T4 e T5 projetam somente os IDs opacos indispensáveis.
- T7 implementa rebind/migração com staging, hashes e auditoria.
