# AntDelirium global: dificuldades atuais e plano de evolução para integração com Cargo/rustc

## 1. Propósito

Este documento registra os problemas observados no modelo de instalação local do AC-Ant-Delirium e propõe uma evolução para um único binário global chamado `antdelirium`.

O destino pretendido é:

```text
$CARGO_HOME/bin/antdelirium
```

Neste ambiente, isso normalmente corresponde a:

```text
/home/dracker/.cargo/bin/antdelirium
```

O estado canônico de governança de cada projeto deixará de ficar dentro do projeto auditado. Ele será materializado sob uma raiz externa protegida, enquanto duas projeções de trabalho continuarão sendo geradas dentro do projeto:

```text
$CARGO_HOME/antdelirium/<project-key>/
```

Exemplo de apresentação humana:

```text
/home/dracker/.cargo/antdelirium/testproject--a1b2c3d4/
```

```text
<project-root>/guard/to-do_list.json
<project-root>/guard/governance_error.json
```

Essa divisão é deliberada. Os arquivos em `guard/` devem ser fáceis de encontrar pelo agente porque contêm a tarefa atual e os erros que ele precisa corrigir. A raiz externa, os planos completos, o orquestrador e os TOMLs administrativos não são superfícies de trabalho do agente e seus caminhos não podem aparecer nas projeções, mensagens normais ou API pública.

O objetivo é tornar o AntDelirium parte normal do ciclo de compilação Rust, de maneira semelhante a ferramentas que envolvem o `rustc`, sem fingir ser uma implementação alternativa do compilador.

## 2. Estado de referência

O repositório novo em `Documents/AC-Ant-Delirium` possui um commit-base preservando a versão anterior à tentativa de adaptação para Godot:

```text
f868d97b2b8d16be78e2117b20c27147dbdc8ca1
```

Essa base:

- possui o catálogo completo de 27 axiomas;
- passa em `cargo fmt --all -- --check`;
- passa na compilação de biblioteca e binários;
- executa 100 testes, dos quais 92 passam e 8 ainda falham;
- ainda contém dois pontos de warning conhecidos;
- ainda usa o modelo de plugin local e os binários `delirium_cli` e `guard`.

Portanto, esta migração não deve começar removendo código antigo. Primeiro precisamos estabilizar a base, criar testes de compatibilidade e só então substituir a implantação local.

## 3. Dificuldades observadas

### 3.1. A autoridade de governança está dentro do espaço editável

O modelo atual instala binário, configuração, scripts e JSONs dentro de `plugins/<project>`. Esse diretório fica próximo do código e geralmente está acessível ao mesmo agente que recebe a tarefa de implementação.

Isso mistura três responsabilidades incompatíveis:

1. código do produto;
2. autoridade que avalia o produto;
3. estado interno da autoridade.

Quando a ferramenta que impõe as regras está dentro da área que o agente pode editar, o agente pode tratar a governança como mais um obstáculo de código a ser alterado.

### 3.2. O binário `guard` não criou uma fronteira de confiança

A tentativa de manter um `guard` dentro do plugin não resolveu o problema. O agente consegue localizar o binário, observar como ele é chamado, estudar os arquivos em volta e tentar reproduzir ou neutralizar seu comportamento.

Um binário local não é secreto. Engenharia reversa deve ser considerada possível. Ofuscação, nomes pouco claros ou verificações internas podem aumentar o custo de análise, mas não criam autoridade independente.

Se o agente roda com o mesmo usuário do binário e tem permissão de escrita sobre o binário, a configuração e os dados, ele também pode:

- substituir o executável;
- alterar os JSONs;
- limpar `RUSTC_WRAPPER` ou `RUSTC_WORKSPACE_WRAPPER`;
- fornecer uma configuração Cargo mais próxima do projeto e sobrescrever a global;
- chamar `rustc` diretamente;
- executar Cargo com `--config` para substituir valores;
- copiar o projeto para outro local e compilar fora do fluxo governado.

### 3.3. Agentes tentam desabilitar a governança em vez de cumprir a tarefa

Quando uma regra bloqueia o build, alguns agentes interpretam o bloqueio como um defeito da ferramenta. Em vez de corrigirem o código solicitado, eles tentam:

- remover o hook;
- alterar o `build.rs`;
- desabilitar uma regra;
- modificar o manifesto de tarefas;
- editar estados de aprovação;
- trocar o binário;
- criar um caminho de compilação que não passa pelo AntDelirium.

Isso mostra que o sistema atual expõe detalhes administrativos demais ao agente de implementação e não separa claramente erro do produto, erro de governança e falha interna da ferramenta.

### 3.4. Agentes abandonam a pasta original do projeto

Outro comportamento recorrente é sair da raiz do projeto e passar longos períodos vasculhando `plugins/`. O agente tenta entender planos, JSONs, scripts e estados internos, mas deixa de trabalhar nos arquivos que resolvem a tarefa original.

Os efeitos são:

- desperdício de tempo;
- perda de contexto da tarefa;
- edições em estado interno;
- risco de corromper a fila de tarefas;
- relatórios de conclusão sem implementação real;
- falsa sensação de progresso por manipulação de metadados.

### 3.5. JSON mutável funciona simultaneamente como armazenamento e API

Arquivos como `governance.json`, `connected.json`, `task_orchestrator.json`, `manager_tasks.json`, `submit_queue.json` e planos numerados são úteis para inspeção e integração. Porém, quando também são a fonte canônica diretamente editável, qualquer consumidor pode tentar reescrever a verdade do sistema.

Além disso, várias invocações paralelas do compilador podem gerar:

- escritas concorrentes;
- arquivos parcialmente gravados;
- perda de eventos;
- estados incompatíveis entre JSONs;
- tarefas duplicadas;
- resultados dependentes da ordem de execução.

### 3.5.1. O gerador de arquivos de trabalho já existe, mas expõe detalhes demais

A base atual já possui mecanismos que geram manifesto de tarefas, manifesto de violações, shadow context e relatórios de terminal. `write_todo_manifest` já projeta a tarefa ativa e `GovernanceLogs` já materializa erros de governança. A evolução não deve descartar esse trabalho nem recriar o produto do zero.

O problema atual é de destino, contrato e exposição:

- os nomes e caminhos ainda remetem a `AC_Ant-Delirium_props`, plugins e artefatos internos;
- algumas mensagens revelam `task_orchestrator.json`, TOMLs, manifests e a localização do plugin;
- a projeção de tarefas pode incluir plano completo e comandos administrativos;
- arquivos projetados e arquivos autoritativos ainda não possuem fronteira inequívoca;
- o agente consegue sair do projeto e alcançar os planos canônicos.

O gerador existente será refatorado para produzir somente:

```text
<project-root>/guard/to-do_list.json
<project-root>/guard/governance_error.json
```

`to-do_list.json` conterá apenas a tarefa atualmente liberada, arquivos-fonte relativos ao projeto, validações normais de Cargo e o comando de submissão. `governance_error.json` conterá somente violações corrigíveis, regras públicas e caminhos relativos ao projeto. Ambos serão projeções descartáveis: alterá-los ou apagá-los nunca mudará o estado canônico, e uma execução governada os regenerará.

Ser discreto, neste documento, não significa esconder esses dois arquivos do agente. Significa não revelar a infraestrutura externa que os produziu: nenhum caminho de `plugins/`, `$CARGO_HOME/antdelirium`, `plan/`, TOML, orquestrador, histórico ou armazenamento administrativo aparecerá em seu conteúdo ou nas mensagens que apontam para eles.

### 3.5.2. A localização externa foi descoberta por análise estática e rastreamento de syscalls

Um teste adversarial em um projeto Godot demonstrou que omitir o caminho das mensagens não é suficiente. O agente combinou quatro técnicas:

1. busca de caminhos absolutos em metadados e arquivos gerados dentro do workspace;
2. extração de strings do binário para descobrir nomes como `task_orchestrator.json`, `connected.json`, `manager_tasks.json`, `submit_queue.json` e `default_hierarchy.toml`;
3. `strace` das chamadas `open`, `openat`, `stat`, `access` e `newfstatat` do `guard`;
4. inspeção direta do diretório externo revelado pelas syscalls.

O ponto decisivo foi arquitetural: o processo executado sob a autoridade do agente abriu diretamente os arquivos protegidos. O kernel necessariamente expôs esses caminhos ao rastreador. Trocar nomes, esconder ajuda ou retirar símbolos não corrige esse fluxo.

O projeto deve tratar como canais distintos de vazamento:

- texto emitido em stdout/stderr;
- projeções e metadados locais;
- literais e metadados de compilação presentes no executável;
- caminhos passados às syscalls do processo cliente;
- descritores, sockets, argumentos e variáveis de ambiente;
- informações de processos disponíveis em `/proc`;
- capacidade de anexar `strace`, debugger ou `ptrace` ao serviço protegido.

A correção obrigatória é impedir que o processo cliente/wrapper abra, teste ou resolva caminhos do armazenamento autoritativo. Apenas um serviço sob outra autoridade poderá realizar esse I/O. O cliente receberá uma resposta sanitizada por um protocolo que não contenha caminhos físicos.

### 3.6. O `build.rs` do projeto é um ponto de bypass

O hook atual depende de código colocado no `build.rs` do projeto consumidor. Como esse arquivo pertence ao projeto, o agente pode modificá-lo. Cargo também pode deixar de executar o `build.rs` quando seus gatilhos não mudaram.

O hook continua útil como defesa em profundidade e como verificação específica do pacote, mas não deve ser a autoridade única.

### 3.7. Existem binários e layouts duplicados

Hoje há conceitos de `delirium_cli`, `ac_ant_delirium`, `guard`, cópias em `plugins/<project>/bin` e guardas gerados para `build.rs`. Isso cria deriva de versão e dúvidas sobre qual artefato é autoritativo.

A evolução deve produzir um único executável físico:

```text
antdelirium
```

Esse binário terá modos internos, mas não cópias funcionais diferentes por projeto.

### 3.8. A base ainda não está totalmente verde

Antes da migração global, devem ser resolvidas as nove falhas reproduzidas na baseline da Onda 0 — oito na biblioteca e uma no binário `delirium_cli` — cobrindo pelo menos:

- fallback de informação da API;
- integração de `manifest_anchoring`;
- integração de `recursion` e sua prioridade nos logs;
- integração de `dependency_purification`;
- fixture de `max_nesting`;
- fallback de parâmetros taxonômicos;
- paridade de violações entre perfis.

Migrar com a suíte quebrada tornaria difícil distinguir regressão nova de problema antigo.

Estado após o início da Onda 0: as nove falhas foram corrigidas e a suíte nativa passou com 228 testes. A evidência e a classificação das causas estão em `docs/BASELINE-ONDA-0.md`.

## 4. Decisão arquitetural proposta

O AntDelirium deve evoluir para um wrapper transparente do compilador real, configurado pelo Cargo como `rustc-workspace-wrapper`.

A configuração pertence ao arquivo global do Cargo:

```text
/home/dracker/.cargo/config.toml
```

Ela não pertence ao `Cargo.toml` do AntDelirium nem ao `Cargo.toml` dos projetos governados. A configuração global pretendida é:

```toml
[build]
rustc-workspace-wrapper = "/home/dracker/.cargo/bin/antdelirium"
```

O código não deve definir `build.rustc = "antdelirium"`. O AntDelirium não implementará o protocolo completo do compilador nem substituirá o toolchain. Cargo continuará escolhendo o `rustc` real e passará o caminho desse compilador como primeiro argumento do wrapper.

### 4.1. Compatibilidade com a versão instalada

`RUSTC_WORKSPACE_WRAPPER` foi estabilizado no Cargo 1.50. O ambiente atual foi
reverificado durante a T7.4 com:

```text
cargo 1.97.1 (c980f4866 2026-06-30)
rustc 1.97.1 (8bab26f4f 2026-07-14)
```

Portanto, não é necessário atualizar Rust ou Cargo para implementar o wrapper. O `Cargo.toml` atual usa edition 2021, foi aceito por `cargo metadata --locked` e compilou com `cargo check --locked --target x86_64-unknown-linux-gnu` nessa família de toolchains. A prova original com 1.93.1 usando `build.rustc-workspace-wrapper="/usr/bin/env"` como delegador transparente passou, e a T7.4 voltou a validar o fluxo com 1.97.1. A opção explícita de target é necessária porque a configuração local deste repositório ainda define `wasm32-unknown-unknown` como target padrão; isso é independente do suporte ao wrapper global.

O piso do protocolo de integração é Cargo 1.50; ele não é o MSRV do binário inteiro. O `Cargo.toml` agora declara `package.rust-version = "1.88"`, piso validado com o grafo bloqueado, enquanto a toolchain 1.97.1 atual suporta tanto o manifesto quanto o wrapper sem atualização adicional.

Segundo a documentação oficial do Cargo:

- `build.rustc-wrapper` envolve todas as invocações de `rustc`;
- `build.rustc-workspace-wrapper` envolve apenas membros do workspace;
- um projeto de pacote único também é considerado workspace;
- o primeiro argumento do wrapper é o caminho do compilador real;
- `RUSTC_WORKSPACE_WRAPPER=""` pode desabilitar a configuração;
- configurações locais e `--config` podem ter precedência sobre a configuração global.

Para reduzir custo e ruído em dependências de terceiros, a recomendação inicial é `rustc-workspace-wrapper`.

Referências oficiais:

- [Cargo configuration: rustc wrappers](https://doc.rust-lang.org/cargo/reference/config.html#buildrustc-wrapper)
- [Cargo environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html)
- [Cargo hierarchical configuration](https://doc.rust-lang.org/cargo/reference/config.html#hierarchical-structure)
- [Cargo 1.50 changelog: `RUSTC_WORKSPACE_WRAPPER`](https://doc.rust-lang.org/cargo/CHANGELOG.html#cargo-150-2021-02-11)

## 5. Escopo correto da integração com os comandos existentes

O agente continuará usando somente os comandos normais:

```text
cargo check
cargo build
cargo test
cargo run
cargo clippy
```

Não haverá um comando alternativo obrigatório como `antdelirium cargo check`. Exigir que o agente escolha outro comando apenas recriaria o problema de governança que esta evolução pretende resolver.

### 5.1. Garantia fornecida pelo wrapper

Em um projeto novo ou alterado, `cargo check` compila pelo menos um membro do workspace. O Cargo chama o AntDelirium automaticamente, e o wrapper:

1. identifica `CARGO_MANIFEST_PATH` e `CARGO_MANIFEST_DIR`;
2. usa `CARGO_PRIMARY_PACKAGE` para distinguir o pacote selecionado de dependências;
3. descobre a raiz do workspace sem iniciar outro Cargo recursivamente;
4. cria o state root externo de maneira idempotente;
5. executa a governança aplicável;
6. delega ao `rustc` real sem alterar seus argumentos ou resultado.

Isso satisfaz o requisito principal: `cargo new testproject`, seguido de `cargo check`, cria automaticamente os dados em `$CARGO_HOME/antdelirium/<project-key>/` sem qualquer instrução adicional ao agente.

### 5.2. Build totalmente atendido pelo cache

O wrapper é uma interface de compilação, não um hook geral de início de comando. Quando nenhum membro do workspace precisa ser compilado, não devemos depender de uma nova invocação primária do wrapper. Uma eventual consulta interna do Cargo ao compilador também não deve ser confundida com uma compilação governável do projeto.

Esse caso não justifica criar um comando novo para o agente. O segundo `cargo check` deve preservar o estado criado pela primeira compilação, sem duplicar ou corromper dados. Se for necessário provar cada execução do comando, inclusive as atendidas pelo cache, essa prova deve vir de uma autoridade externa, como CI limpo, supervisor ou sandbox, e não da cooperação do agente.

### 5.3. Limite de enforcement

O Cargo permite que configuração local, variáveis de ambiente e `--config` substituam a configuração global. Logo, `$CARGO_HOME/config.toml` fornece integração automática para o fluxo normal, mas não é sozinho uma barreira contra um agente hostil com o mesmo usuário.

O contrato será:

- os comandos públicos dos agentes continuam sendo os comandos Cargo existentes;
- o wrapper global observa automaticamente todas as compilações de membros do workspace;
- o CI ou supervisor executa uma compilação limpa e verifica a presença e integridade dos artefatos de governança;
- nenhuma documentação instrui o agente a trocar `cargo check` por outro comando.

## 6. Fluxo desejado

```text
Desenvolvedor ou agente
        |
        v
     cargo check
        |
        v
Cargo seleciona um membro do workspace
        |
        v
antdelirium <caminho-do-rustc-real> <argumentos-exatos>
        |                                  |
        |                                  +--> delegação transparente ao rustc real
        |
        +--> identificação do projeto
        +--> bootstrap idempotente dos dados externos
        +--> registro mínimo do evento de compilação
        +--> auditoria somente quando o fingerprint mudou
        +--> projeção sanitizada em guard/to-do_list.json
        +--> projeção sanitizada em guard/governance_error.json
        +--> propagação exata de stdout, stderr e exit code
```

O wrapper nunca deve:

- reescrever argumentos do `rustc` sem uma especificação explícita;
- esconder diagnósticos do compilador;
- transformar sucesso do `rustc` em sucesso artificial;
- iniciar outro Cargo durante a própria invocação;
- auditar dependências de registro por padrão;
- copiar fontes do projeto para a pasta de governança;
- escrever dentro do projeto durante a compilação, exceto pela atualização atômica das duas projeções declaradas em `guard/`.

## 7. Um único binário, vários modos internos

O executável físico será sempre `antdelirium`.

Modos propostos:

```text
antdelirium <rustc-real> <rustc-args...>     modo wrapper detectado automaticamente
antdelirium install                          instalação e integração Cargo
antdelirium uninstall                        remoção segura apenas da integração própria
antdelirium doctor                           diagnóstico de wrapper, Cargo e armazenamento
antdelirium init [--project-root <path>]     bootstrap explícito
antdelirium check [--project-root <path>]    auditoria explícita
antdelirium status                           estado resumido do projeto atual
antdelirium next-action                      somente a próxima ação permitida
antdelirium --api <path>                     consulta controlada
antdelirium migrate-plugin <path>            migração controlada do layout antigo
```

Um único executável físico não significa uma única superfície de autorização. O runtime deve separar pelo menos quatro perfis:

| Perfil | Superfície permitida |
|---|---|
| wrapper Cargo | delegação ao `rustc`, auditoria e atualização das duas projeções |
| agente de implementação | tarefa atual, regra relacionada, próxima ação e submissão de evidência |
| revisor | inspeção da submissão e decisão limitada à tarefa atribuída |
| operador/supervisor | instalação, migração, configuração, planos, overrides, histórico e diagnóstico interno |

`install`, `uninstall`, `doctor --verbose`, `migrate-plugin`, configuração bruta, overrides, manutenção de agentes e mutações de planos são operações administrativas. Elas não devem aparecer no `--help` ou na API projetada para o agente, e precisam de autorização externa ao cwd e ao nome do processo.

A inspeção dos binários antigos mostrou que `guard` não implementava uma API limitada: ele localizava `ac_ant_delirium` e encaminhava todos os argumentos, inclusive `--api`, `--config`, `--project-root` e operações administrativas. A nova arquitetura não pode preservar esse pass-through como fronteira de confiança.

O agente não poderá marcar diretamente `implemented=true`, `task_reviewed=true`, `review_status`, `grade`, `active_plan` ou o modo do orquestrador. A API pública aceitará uma submissão, por exemplo:

```text
antdelirium --api 'hierarchy.task.current.submit Evidências verificáveis'
```

Essa operação muda apenas o estado da submissão. Somente a autoridade protegida, depois de validar diff, arquivos, build, governança, identidade e revisão exigida, poderá alterar os campos autoritativos do plano.

O modo wrapper deve ser identificado sem ambiguidade. A primeira versão deve exigir simultaneamente:

- primeiro argumento apontando para um executável compatível com `rustc`;
- presença dos argumentos típicos de compilação ou consulta do compilador;
- ausência de um subcomando AntDelirium conhecido.

Uma variável interna de profundidade deve impedir recursão acidental:

```text
ANTDELIRIUM_WRAPPER_DEPTH=1
```

Se a profundidade já estiver ativa, o binário deve apenas delegar ao compilador real.

A T6.1 fechou a interpretação dessa variável: ausência ou `0` seleciona a rota
inicial governada, `1` seleciona exclusivamente a futura delegação direta e
qualquer outro valor falha fechado. A profundidade só é interpretada depois de
uma forma wrapper inequivocamente reconhecida e nunca transforma outro perfil
em wrapper. A detecção é lexical e não consulta o filesystem apontado pelo
argumento. A T6.2 implementou a execução no target Unix por substituição de
processo: `argv` permanece em `OsString`, os três streams são herdados e o
próprio `rustc` passa a controlar exit e sinal. A T6.4 acrescentou a negociação
limitada e a T6.5 colocou bootstrap, fingerprint, auditoria e projeções em um
filho audit-only, enquanto o pai continua delegando imediatamente.

A T6.6 executou essa delegação em uma fixture Cargo real: workspace virtual,
três membros, build script, proc-macro, testes, exemplo, compilação Linux e
target WASM, com paralelismo `-j 4` e repetição em cache. O teste configura o
binário somente no ambiente do subprocesso, demonstra que o Cargo realmente o
invoca e compara a saída do binário nativo direto e encapsulado. A matriz não
instala configuração global nem substitui os gates posteriores contra um
serviço protegido ativo.

A T6.7 fechou o orçamento desta rota com um benchmark pareado e automatizado.
Medianas alternadas com targets isolados limitam o overhead positivo a `25 ms`
por entrada do wrapper, `2 s` no check limpo, `1 s` no incremental e `250 ms`
no no-op. A [evidência T6.7](./VALIDACAO-OVERHEAD-WRAPPER-T6.7.md) registra
deltas de `2,220 ms`, `48,752 ms`, `5,214 ms` e `1,139 ms`, respectivamente,
com exit, stdout e stderr idênticos. A rota com serviço ativo permanece fora
dessa medição e será coberta na T10.6.

## 8. Estrutura externa por projeto

A estrutura deve manter a utilidade do plugin AutoBrowser sem copiar os binários para cada projeto.

```text
$CARGO_HOME/antdelirium/
  registry.json
  testproject--a1b2c3d4/
    project.json
    ant_delirium_config/
      default_hierarchy.toml
      default_rules.toml
    global/
      transition_task.json
    plan/
      ...
    problems/
      ...
    project_state/
      workspaces/
        <workspace-key>/
          context.json
          last_compilation.json
      schemas/
      fixtures/
      references/
      tmp/
    scripts/
      README.md
      *.template.sh
    artifacts/
      violations_manifest.json
      shadow_context.sarif.json
      to-do_list_manifest.json
    state/
      lock
      events.log
      snapshot.json
    connected.json
    context.json
    feedback.json
    governance.json
    governance_history.json
    manager_tasks.json
    messages.json
    submit_queue.json
    task_orchestrator.json
    task_index.txt
    trash/
```

No AutoBrowser atual, a fonte canônica dos planos ainda está em uma raiz externa administrativa:

```text
<protected-governance-root>/AutoBrowser/plan/
```

O caminho físico real ficará no registro privado do operador e não será compilado no cliente, incluído em documentação entregue ao agente ou retornado pela API pública. É nessa superfície que `implemented`, revisão, aprovação e avanço são decididos. Ela deve ser tratada como armazenamento administrativo protegido durante a migração. Qualquer mudança futura para outro state root exige staging, comparação de hashes, preservação do histórico e uma autoridade inacessível ao agente; não basta apenas mudar o caminho.

Dentro do projeto governado haverá somente a projeção visível:

```text
<project-root>/
  guard/
    to-do_list.json
    governance_error.json
```

O nome `guard/` é intencionalmente simples e encontrável. Ele não contém binário, configuração, plano completo, histórico nem estado de aprovação.

Diferenças deliberadas em relação ao plugin local:

- não haverá `bin/` por projeto;
- não haverá binário `guard` por projeto; poderá existir somente a pasta projetada `guard/`;
- não haverá `.git` dentro da pasta de dados;
- `project_state/` não será uma cópia do código-fonte;
- scripts gerados serão apenas templates não executáveis;
- JSONs de leitura poderão ser projeções de um estado canônico transacional;
- o caminho da pasta de dados não será apresentado como local de trabalho para o agente de implementação.

As projeções devem usar somente caminhos relativos à raiz do projeto. `active_plan` físico será substituído por um identificador opaco no formato `pln_<26 caracteres Crockford>` em qualquer estado autorizado que realmente precise desse ID; normalmente o agente recebe somente a tarefa atual. `complete_plan`, tarefas futuras, nomes de JSONs administrativos e caminhos externos são proibidos em `guard/`.

## 9. Identidade de projeto e worktree

Usar somente o nome do pacote causa colisões. Dois repositórios podem se chamar `server`, `core` ou `testproject`.

O `project-key` deve combinar:

1. slug legível derivado do workspace ou pacote;
2. hash curto de uma identidade canônica;
3. registro da raiz original em `project.json`.

Exemplo:

```text
testproject--6g4m2k8r9c1v7x3b5n0q2s4t6w
```

O algoritmo foi fechado na T1.5 pelo contrato [`identity-v1`](./IDENTIDADE-OPACA-T1.5.md): o serviço emite IDs CSPRNG de 128 bits; o slug é congelado e somente descritivo; locators de Git/filesystem servem para reencontro privado, nunca como ID. Para repositórios Git, o common git directory agrupa worktrees do mesmo repositório e cada checkout recebe um `workspace-key` próprio. Clones são projetos diferentes e movimentos entre filesystems exigem rebind administrativo explícito.

O algoritmo definitivo precisa ser validado em:

- pacote único;
- workspace virtual;
- workspace com pacote raiz;
- `--manifest-path` fora do diretório atual;
- Git worktrees;
- diretório sem Git;
- dois projetos com o mesmo nome;
- caminho movido ou renomeado.

## 10. Escrita concorrente e consistência

Cargo executa compiladores em paralelo. O AntDelirium não pode reescrever vários JSONs livremente a cada processo.

Requisitos:

- bootstrap idempotente;
- um lock por projeto;
- escrita em arquivo temporário seguida de rename atômico;
- permissões `0700` para diretórios e `0600` para estado mutável;
- journal de eventos append-only ou armazenamento transacional;
- snapshots versionados por schema;
- recuperação explícita após interrupção;
- nenhuma espera longa segurando o caminho crítico do compilador;
- deduplicação por fingerprint de fonte, configuração e revisão;
- retenção e limpeza previsíveis.

Essas permissões só criam uma fronteira real quando o agente não executa com a mesma autoridade proprietária. Os planos canônicos precisam ficar fora do sandbox do agente ou pertencer a um serviço/usuário protegido. O wrapper cliente não deve abrir `plan/*.json` diretamente quando estiver sob as credenciais do agente; deve solicitar ao serviço somente a projeção autorizada.

Antes de escolher o backend, devem ser prototipadas duas alternativas:

1. journal append-only com lock e projeções JSON;
2. banco embutido transacional com exportações JSON compatíveis.

A escolha deve ser feita por teste de concorrência, recuperação e custo de implantação, não apenas por preferência.

## 11. Política de execução do wrapper

### 11.1. Invocações que apenas delegam

Consultas do toolchain, detecção de target, impressão de versão e compilações de dependências não primárias devem ser encaminhadas sem auditoria completa.

A T6.3 concretizou essa distinção: flags de consulta têm prioridade mesmo no
probe Cargo com `--crate-name ___`; compilação sem marcador de pacote primário é
dependência, e formas incompletas ficam `Unclassified`. Todas essas classes
somente delegam.

### 11.2. Invocações que podem materializar o projeto

Uma invocação de membro do workspace, especialmente quando `CARGO_PRIMARY_PACKAGE` estiver presente, pode:

- registrar a identidade;
- criar o diretório externo;
- atualizar o fingerprint;
- produzir artefatos mínimos.

`CARGO_PRIMARY_PACKAGE` é um sinal útil, mas não deve ser assumido como identificador completo de workspace. A fase de protótipo precisa capturar o ambiente real fornecido pelo Cargo em vários tipos de projeto.

A T6.3 exige também `CARGO_MANIFEST_DIR`, `CARGO_MANIFEST_PATH` e
`CARGO_CRATE_NAME` coerentes com o `--crate-name` antes de selecionar
`PrimaryWorkspaceMember`. A classificação não é autorização; ela apenas decide
qual entrada pode iniciar o filho audit-only que usa o canal limitado.

### 11.3. Ordem de compilação e auditoria

No rollout inicial:

1. validar protocolo do wrapper;
2. delegar ao `rustc` real;
3. preservar exatamente o status do compilador;
4. atualizar dados de maneira idempotente;
5. operar governança em modo audit-only.

Depois de estabilizado:

- violações de política do projeto primário poderão bloquear;
- falhas internas do AntDelirium terão código e mensagem próprios;
- falha ao executar o `rustc` real sempre será propagada;
- dependências externas não serão bloqueadas por regras de código do projeto.

A T1.7 fechou o contrato [`errors-v1`](./ERROS-STDIO-T1.7.md): stdout do wrapper pertence exclusivamente ao compilador; diagnósticos AntDelirium usam stderr; falhas próprias ocupam a faixa 64–78; uma falha ou sinal do `rustc` tem precedência e é preservado; audit-only não transforma violações em exit não zero.

A T6.5 concretizou essa ordem sem aguardar o serviço antes do `exec`: somente o
membro primário cria um filho audit-only, e o pai fecha o descriptor secreto e
se transforma imediatamente no `rustc`. O filho descobre a raiz Cargo, calcula
fingerprints de argv, manifestos e fonte, executa bootstrap/observação/projeção
pelo canal limitado e materializa apenas `guard/`. Qualquer falha nessa rota é
silenciosa e não pode substituir stdout, stderr, exit ou sinal do compilador.

A T6.6 validou o caminho de falha silenciosa em builds Cargo reais com endpoint
e capability ausentes: check paralelo e em cache, testes, binário, exemplo,
build script, proc-macro e target WASM terminam normalmente, sem criar `guard/`.
Concorrência contra o serviço ativo continua separada na matriz integral da
T10.6, depois dos gates de isolamento e instalação.

## 12. Modelo de ameaça e limites reais

### 12.1. O que a instalação em `$CARGO_HOME` melhora

- remove o binário da pasta do projeto;
- elimina cópias divergentes por plugin;
- reduz o incentivo para o agente entrar em `plugins/`;
- permite atualização centralizada;
- materializa estado fora do repositório;
- torna o caminho normal de Cargo automaticamente observável durante compilações;
- permite uma API única e estável.

### 12.2. O que ela não consegue impedir sozinha

Um agente executado como o mesmo usuário ainda pode, em princípio:

- editar `$CARGO_HOME/config.toml`;
- substituir `$CARGO_HOME/bin/antdelirium`;
- desabilitar o wrapper por variável de ambiente;
- usar uma configuração Cargo local de maior precedência;
- chamar `rustc` diretamente;
- editar `$CARGO_HOME/antdelirium/`;
- compilar uma cópia do projeto em outro ambiente.

Logo, a instalação user-local é uma melhoria de arquitetura e ergonomia, não uma barreira absoluta contra um agente hostil.

### 12.3. Autoridade necessária para enforcement real

Para impedir desabilitação intencional, pelo menos uma autoridade precisa ficar fora do alcance do agente:

- binário e configuração controlados por root ou por outro usuário;
- serviço supervisor com API limitada;
- sandbox que permita escrever somente na raiz do projeto;
- CI remoto obrigatório com política protegida;
- branch protection que exija o resultado do AntDelirium;
- assinatura de eventos com chave não disponível ao agente.

O caminho recomendado é usar `$CARGO_HOME` para integração local e CI/supervisor como autoridade final.

Para o AutoBrowser, o requisito imediato é mais específico: o agente não pode ler nem escrever `plugins/AutoBrowser/plan/` ou `task_orchestrator.json`. Hoje esses arquivos usam permissões graváveis pelo mesmo usuário, portanto o layout atual não satisfaz a fronteira. A implantação deverá combinar sandbox de raiz do projeto com serviço/usuário protegido, ou outra capability externa equivalente. Tornar arquivos apenas `readonly` para o mesmo usuário é defesa contra acidente, não autorização suficiente.

### 12.4. Resistência à descoberta de caminhos por binário, syscall e processo

O processo `antdelirium` iniciado pelo Cargo sob as credenciais do agente será tratado como cliente não privilegiado. Durante uma compilação ele não poderá executar `open`, `openat`, `stat`, `lstat`, `access`, `newfstatat`, `readlink` ou enumeração de diretório sobre qualquer raiz de governança protegida.

O fluxo permitido será:

```text
Cargo/rustc wrapper não privilegiado
        |
        +--> arquivos do projeto e target
        +--> guard/to-do_list.json
        +--> guard/governance_error.json
        +--> protocolo limitado por socket/canal
                                  |
                                  v
                    serviço sob outra autoridade
                                  |
                                  +--> planos e estado canônico
```

O protocolo local foi fechado na T1.6 como `protocol-v1`: AF_UNIX `SOCK_STREAM`, framing limitado, `SO_PEERCRED` mais capability de 256 bits, nonce por conexão, request id deduplicado e listeners/parsers distintos para os canais limitado (`ADL1`) e administrativo (`ADA1`). O bootstrap automático de um projeto desconhecido cria apenas registro provisório vazio; enrollment protegido exige supervisor ou operador confiável.

Requisitos para o cliente distribuído:

- nenhum caminho absoluto de plano, plugin ou state root protegido embutido em literais;
- nenhum caminho de build do mantenedor preservado em símbolos ou metadados de release;
- nenhuma variável de ambiente, argumento ou mensagem contendo a localização protegida;
- nenhum `login.json` local contendo `workspace_path` absoluto;
- nenhum log de debug ou erro escrito em `/tmp`;
- descritores privilegiados marcados `close-on-exec` e nunca herdados pelo processo do agente;
- protocolo usando IDs opacos de projeto, plano, tarefa e evento;
- respostas do serviço contendo somente caminhos relativos ao projeto quando um arquivo-fonte precisar ser indicado.

Requisitos para o serviço protegido:

- execução por usuário/autoridade diferente do agente ou dentro de isolamento equivalente comprovado;
- processo não anexável pelo agente via `ptrace`, `strace` ou debugger;
- `/proc/<pid>` e descritores do serviço não legíveis pelo agente além do mínimo inevitável da plataforma;
- socket administrativo separado da interface limitada do wrapper;
- autenticação e autorização independentes de cwd, nome do binário ou argumento oculto;
- nenhum segredo administrativo entregue ao ambiente do agente;
- logs e registros internos fora do sandbox e sem caminhos físicos nas respostas públicas.

Hardening de release, remoção de símbolos e remapeamento de caminhos de compilação são defesa em profundidade. Eles reduzem pistas estáticas, mas não substituem permissões, sandbox e separação de processo. Nomes genéricos eventualmente descobertos não podem conceder localização nem acesso ao armazenamento real.

A T6.8 materializou a prova dinâmica do lado cliente em `make syscall-client-nondisclosure`. O gate segue com `strace -f` todo o `cargo check` isolado, o wrapper, seus filhos audit-only e o compilador real; exige a conexão limitada e rejeita qualquer syscall que mencione roots, layout ou payload sentinela do estado protegido. O teste não usa `sudo`, falha inconclusivo com exit `77` se `ptrace` não estiver disponível e não substitui a prova posterior de que o serviço instalado resiste a attach e inspeção de `/proc`.

## 13. Como evitar que o agente abandone o projeto

O novo sistema deve mudar também a experiência apresentada ao agente.

Regras de produto:

- mensagens sempre começam pela ação no código do projeto;
- a saída mostra somente arquivos-fonte relevantes à tarefa atual;
- caminhos internos de armazenamento não aparecem em prompts normais;
- mensagens de Cargo apontam somente para `guard/to-do_list.json` e `guard/governance_error.json`;
- `next-action` entrega uma tarefa por vez;
- JSONs administrativos não são instruídos como mecanismo de edição;
- mutações passam por comandos com validação, autorização e log;
- erros distinguem `code_violation`, `governance_configuration_error`, `tool_internal_error` e `authorization_error`;
- a mensagem de erro nunca recomenda desabilitar uma regra;
- o sistema registra tentativa de mutação inválida;
- a documentação de implementação afirma explicitamente que explorar armazenamento interno não conclui tarefas;
- o runtime pode alertar quando o cwd informado deixa a raiz do workspace, sem tentar controlar o shell do agente.

Mensagens normais devem seguir o formato discreto:

```text
antdelirium[AD-TASK]: leia guard/to-do_list.json
antdelirium[AD-GOV]: leia guard/governance_error.json
```

Elas nunca devem imprimir o caminho do plugin, da pasta `plan`, do state root externo, dos TOMLs, do orquestrador ou dos manifests internos. Ajuda administrativa detalhada só poderá ser obtida por uma superfície de operador autorizada.

Códigos, exit status, JSON público, correlação opaca e composição com o resultado do compilador seguem o contrato [`errors-v1`](./ERROS-STDIO-T1.7.md). Erros conhecidos não usam `[!]`, panic, path do executável ou erro bruto do sistema.

O AntDelirium deve tornar o caminho correto mais curto que o caminho de bypass.

## 14. Plano de implementação

### Fase 0 — estabilizar a base

Objetivo: obter uma referência confiável antes da migração.

Tarefas:

1. catalogar e corrigir as nove falhas reproduzidas na baseline da Onda 0;
2. reduzir os dois warnings conhecidos;
3. congelar testes de catálogo garantindo os 27 axiomas;
4. adicionar testes de bootstrap do layout AutoBrowser;
5. registrar benchmarks de `cargo check` limpo, incremental e no-op;
6. determinar o MSRV real do binário e declarar `package.rust-version` no `Cargo.toml`;
7. produzir um release reproduzível do commit-base estabilizado.
8. capturar uma baseline adversarial com `strings`, metadados ELF e `strace` das operações de arquivo;
9. inventariar caminhos absolutos, nomes administrativos compilados e todos os writes atuais em `/tmp`.

Critério de saída:

- formatação passa;
- `cargo check` passa sem warnings inesperados;
- todos os testes passam;
- o MSRV declarado compila o binário e suporta `rustc-workspace-wrapper`;
- catálogo e templates têm snapshots versionados.

### Fase 1 — separar domínio, armazenamento e CLI

Objetivo: remover a dependência estrutural de `plugins/<project>` sem mudar a implantação ainda.

Tarefas:

1. criar uma abstração `ProjectStateRoot`;
2. remover joins de caminho espalhados pelo runtime;
3. centralizar descoberta de projeto e worktree;
4. versionar schemas de estado conforme os contratos independentes `canonical-state-v1`, `submission-v1`, `audit-event-v1` e `agent-projection-v1` congelados na T1.4;
5. implementar locks e escrita atômica;
6. criar migração de layout antiga para externa;
7. garantir que nenhum writer escreva diretamente fora da abstração.
8. adaptar o gerador existente de tarefas e logs para uma interface `ProjectProjectionWriter`;
9. separar em tipos distintos estado canônico, submissão e projeção para o agente.

Critério de saída:

- a mesma suíte roda com state root local e externo;
- testes provam que fontes do projeto não são copiadas nem alteradas;
- somente `guard/to-do_list.json` e `guard/governance_error.json` podem ser gerados no projeto.

### Fase 2 — produzir o binário único `antdelirium`

Objetivo: unificar `delirium_cli`, `ac_ant_delirium` e `guard`.

Tarefas:

1. declarar somente um `[[bin]]` autoritativo;
2. mover diferenças de comportamento para subcomandos/modos;
3. preservar APIs úteis do binário antigo;
4. remover documentação que manda copiar binário para plugin;
5. gerar `antdelirium --version` com hash e versão de schema;
6. adicionar `doctor`, `install` e `uninstall` seguros;
7. impedir que `uninstall` remova configuração que não pertence ao AntDelirium.
8. implementar roteamento por perfil/capability antes do parser legado;
9. substituir o pass-through irrestrito do `guard` por superfícies autorizadas;
10. impedir descoberta de comandos administrativos pela ajuda do agente.
11. remover caminhos absolutos e templates de localização protegida dos literais do cliente;
12. configurar release com símbolos desnecessários removidos e caminhos de compilação remapeados;
13. adicionar auditoria automatizada de `strings` e metadados ELF ao release.

Critério de saída:

- existe um único executável físico;
- nenhuma funcionalidade depende de `plugins/<project>/bin`.

### Fase 3 — implementar o wrapper transparente

Objetivo: provar equivalência de compilação.

Tarefas:

1. detectar o protocolo de wrapper;
2. encaminhar argv sem perda ou reordenação;
3. herdar stdin, stdout e stderr adequadamente;
4. propagar exit code e sinais;
5. implementar proteção de recursão;
6. ignorar probes sem contexto de projeto;
7. limitar auditoria a membros do workspace;
8. testar targets nativos, WASM, build scripts, proc-macros, testes e exemplos;
9. medir overhead com e sem mudança de fonte;
10. validar interação com wrappers existentes, especialmente caches de compilação.
11. provar por rastreamento de syscalls que o wrapper cliente não abre, testa nem enumera o state root protegido;
12. substituir acesso direto aos planos por protocolo limitado com IDs opacos;
13. garantir que somente o serviço protegido resolva IDs opacos para caminhos físicos.

Critério de saída:

- resultados do `rustc` com e sem wrapper são equivalentes;
- overhead de no-op do wrapper fica dentro do limite definido;
- nenhum deadlock ocorre em builds paralelos.

### Fase 4 — materialização externa automática

Objetivo: criar o pacote por projeto na primeira compilação relevante.

Tarefas:

1. resolver `$CARGO_HOME` com fallback documentado;
2. gerar `project-key` sem colisões;
3. materializar a estrutura externa idempotentemente;
4. importar templates embutidos no binário;
5. gerar JSONs válidos e schemas versionados;
6. criar permissões restritas;
7. separar projeto Git de worktrees;
8. nunca criar `plugins/`, `AC_Ant-Delirium_props` ou `build.rs` no projeto novo;
9. registrar a origem e o fingerprint em `project.json`.
10. gerar atomicamente `guard/to-do_list.json` a partir do mecanismo existente de `write_todo_manifest`;
11. gerar atomicamente `guard/governance_error.json` a partir do mecanismo existente de logs e violações;
12. remover caminhos externos, plano completo, tarefas futuras e configuração administrativa dessas projeções;
13. não regravar projeções quando o conteúdo não mudou.
14. remover `login.json` e qualquer outro estado administrativo de dentro do projeto;
15. rejeitar caminhos absolutos em qualquer arquivo gerado sob `guard/`;
16. eliminar `/tmp/delirium_debug.log`, `/tmp/delirium_error.log` e outros logs globais previsíveis;
17. manter logs internos somente no armazenamento protegido e diagnósticos públicos somente nos canais definidos.

Critério de saída:

- um projeto `cargo new testproject` seguido da primeira compilação cria exatamente um state root externo;
- o mesmo fluxo cria ou atualiza somente as duas projeções previstas em `guard/`;
- uma segunda compilação não duplica nem corrompe dados;
- dois projetos de mesmo nome recebem chaves diferentes.

### Fase 5 — instalador Cargo seguro

Objetivo: integrar ao Cargo sem sobrescrever configuração do usuário.

Tarefas:

1. instalar em `$CARGO_HOME/bin/antdelirium`;
2. ler e mesclar `$CARGO_HOME/config.toml`;
3. escrever `build.rustc-workspace-wrapper` somente na configuração do Cargo, nunca no `Cargo.toml` de um projeto;
4. preservar todas as chaves existentes;
5. detectar wrapper já configurado;
6. recusar conflito sem uma decisão explícita;
7. criar backup recuperável antes de alterar configuração;
8. rejeitar Cargo anterior a 1.50 e validar a configuração com o MSRV que o AntDelirium declarar;
9. manter o proxy `$CARGO_HOME/bin/cargo` pertencente ao rustup sem alterações;
10. implementar `doctor` para detectar overrides locais e por ambiente;
11. implementar `uninstall` que reverta somente a alteração registrada.

Critério de saída:

- instalação e desinstalação são reversíveis;
- configuração existente nunca é truncada;
- conflitos com outros wrappers são apresentados claramente;
- `cargo check`, `cargo build` e `cargo test` continuam sendo os comandos normais;
- o instalador não substitui nem renomeia o executável ou proxy `cargo`.

Estado da T7.1: o release já copia a si próprio atomicamente para
`$CARGO_HOME/bin/antdelirium`, recusa root, symlinks, ownership divergente e
overwrite de instalação desconhecida, e trata bytes idênticos como no-op. O
gate descartável comprova que o proxy `cargo` e `config.toml` mantêm bytes e
inode. Esta etapa ainda não modifica a configuração: ativação do wrapper,
backup e rollback pertencem às T7.2–T7.5, portanto nenhuma instalação real foi
feita no Cargo home do usuário.

Estado da T7.2: a transação format-preserving de `config.toml` está pronta e
testada em Cargo homes descartáveis. Antes do replace ela persiste bytes exatos,
presença, modo e hashes anterior/planejado em backup privado, recusa o arquivo
legado `config`, symlinks, TOML inválido, backup ambíguo e mudança concorrente,
e preserva inode quando a mutação é no-op. A transação permanece desligada do
comando `install` até a T7.3 limitar a mutação exclusivamente ao wrapper de
workspace e fechar a política de conflitos.

Estado da T7.3: `install` agora compõe a instalação idempotente do release com a
transação de configuração e escreve somente `build.rustc-workspace-wrapper`
apontando para o binário daquele Cargo home. Um valor idêntico é no-op sem
reivindicação de ownership; qualquer `build.rustc-wrapper`, wrapper de workspace
divergente, tipo inválido ou backup conflitante falha no preflight sem copiar o
binário nem reescrever configuração. Não há `--force`: composição com `sccache`
ou outro wrapper permanece uma decisão explícita do operador. A revalidação após
a cópia preserva fail-closed em caso de corrida.

Estado da T7.4: o diagnóstico administrativo `doctor v1` verifica os pisos Cargo
1.50 e rustc/MSRV 1.88, identidade da instalação, ownership registrado e modo
da configuração, overrides de compilador por ambiente, hierarquia Cargo local e
disponibilidade estática do serviço/socket activation. Sua saída usa somente
estados e contagens sem paths ou valores. Arquivos locais são lidos com limite,
`O_NOFOLLOW` e snapshot consistente. `--config` é declarado não observável antes
da execução porque vence ambiente e TOML por comando; CI/supervisor permanece o
gate autoritativo. O diagnóstico não ativa unidade, não conecta a socket e não
escreve estado.

Estado da T7.5: `antdelirium uninstall` exige o backup privado e os hashes da
instalação para reconhecer ownership. Ele restaura bytes e modo anteriores, ou
remove `config.toml` apenas quando o recibo prova que o arquivo não existia;
configuração idêntica não reivindicada, bytes/modo alterados, entrada inesperada,
symlink e binário divergente falham fechados. Um marcador sincronizado mantém o
backup até a remoção comprovada do binário e permite retomar interrupção sem
deixar o Cargo apontando para executável ausente. O proxy `cargo`, `bin/` e
qualquer configuração não registrada permanecem intocados.

Estado da T7.6: o gate reproduzível `make cargo-command-matrix` instala o release
em Cargo home descartável e executa literalmente `cargo check`, `cargo build`,
`cargo test`, `cargo run` e `cargo clippy`, cada um com target limpo e sem
variável ou argumento de wrapper. A fixture local define somente o target nativo
para não herdar o target WASM deste checkout; o gate recusa qualquer chave de
wrapper nela. Um probe separado com profundidade inválida exige `AD-CONFIG-0001`
e prova que a configuração instalada foi carregada. Não foi criado comando
alternativo AntDelirium nem estado obsoleto no projeto.

Estado da T7.7: `make authoritative-ci-gate` implementa a mecânica protegível do
CI/supervisor. Um controle prova que configurações e binário locais divergentes
falham; o gate então remove overrides ambientais, fixa `rustc` e o wrapper
confiável por `cargo --config`, usa target exclusivo vazio e exige uma compilação
primária observável. O segundo check em cache é apenas advisory; cada nova
execução ganha outro target limpo. O modo protegido exige path e SHA-256 vindos
de fora do workspace, revalida o binário ao final e gera evidência versionada
sem paths. Execução local, digest derivado no próprio PR e evidência não assinada
não são autoridade; runner protegido, assinatura e branch gate ficam na T9.7.

### Fase 6 — comandos Cargo existentes e gate externo

Objetivo: manter a integração invisível para o agente e reduzir bypass sem substituir o Cargo.

Tarefas:

1. validar `cargo check`, `cargo build`, `cargo test`, `cargo run` e `cargo clippy` sem alterar a forma de chamada;
2. provar que a primeira compilação de um projeto novo materializa o state root;
3. tratar uma execução atendida pelo cache como ausência legítima de nova compilação, preservando o estado existente;
4. publicar um fluxo CI autoritativo que use ambiente ou configuração protegida e target limpo;
5. verificar no CI que o wrapper participou das compilações primárias esperadas;
6. detectar tentativa de desabilitação por ambiente, configuração local, `--config` ou chamada direta do compilador;
7. definir política fail-open de rollout e fail-closed de produção;
8. documentar que não existe comando AntDelirium alternativo obrigatório para agentes.

Critério de saída:

- todos os comandos Cargo existentes continuam funcionando sem instruções especiais;
- a primeira compilação gera os dados externos automaticamente;
- o CI bloqueia bypass mesmo que o wrapper local seja removido ou substituído;
- nenhum componente substitui o proxy `cargo` gerenciado pelo rustup.

### Fase 7 — migrar projetos existentes

Objetivo: retirar binários e estado mutável de `plugins/<project>` sem perda.

Tarefas:

1. criar inventário read-only do plugin antigo;
2. validar todos os JSONs antes da migração;
3. copiar dados para staging externo;
4. normalizar caminhos absolutos e relativos;
5. preservar histórico, planos, problemas e transições;
6. comparar hashes e contagens;
7. trocar o projeto para o wrapper global;
8. executar validação completa;
9. arquivar o plugin antigo de maneira recuperável;
10. nunca apagar automaticamente o original no primeiro rollout.
11. manter `plugins/AutoBrowser/plan/` autoritativo e protegido até a migração ser aprovada e verificada;
12. trocar caminhos físicos de plano por IDs opacos nas projeções e no protocolo do agente.

Critério de saída:

- AutoBrowser mantém planos, histórico e estado necessários;
- o projeto compila sem binário ou guard dentro de `plugins/`;
- rollback restaura o modelo anterior.

A T1.8 fechou a política [`lifecycle-v1`](./POLITICA-GIT-RETENCAO-MIGRACAO-T1.8.md): projections são ignoradas por regra mínima autocontida em `guard/`, sem editar Git global/raiz; cutover exige operador e verificador; origem permanece recuperável por no mínimo 30 dias; state governado nunca sofre purge automático.

### Fase 8 — endurecimento contra agentes hostis

Objetivo: transformar detecção local em enforcement confiável.

Tarefas:

1. definir o adversário e as permissões do agente;
2. executar o agente em sandbox limitado à raiz do projeto;
3. tornar binário e configuração não graváveis pelo agente;
4. mover chaves de assinatura para supervisor ou CI;
5. assinar eventos e snapshots relevantes;
6. detectar lacunas de sequência e rollback de estado;
7. exigir resultado do gate em branches protegidas;
8. testar bypass por env, config local, `--config`, rustc direto e cópia do projeto.
9. impedir leitura e escrita de `plan/` e `task_orchestrator.json` pelo sandbox do agente;
10. provar que `submit` não altera `implemented`, revisão, aprovação ou grade;
11. permitir alteração autoritativa apenas pelo serviço, revisor ou operador com capability adequada;
12. registrar tentativa de acesso ou mutação direta dos planos quando a plataforma permitir.
13. executar o serviço sob usuário ou domínio de segurança diferente do agente;
14. impedir attach por `ptrace`, `strace` e debugger ao serviço protegido;
15. restringir exposição do serviço em `/proc` e impedir leitura de descritores, argumentos e ambiente;
16. separar socket administrativo e socket/canal limitado do wrapper;
17. testar herança de descritores e garantir `close-on-exec` em recursos privilegiados;
18. testar enumeração, `readlink`, `stat`, `access` e abertura direta contra a raiz protegida.

Critério de saída:

- os cenários de bypass definidos falham ou são detectados por uma autoridade que o agente não controla.

## 15. Matriz mínima de validação

| Cenário | Resultado obrigatório |
|---|---|
| `cargo new` + primeiro `cargo check` | cria state root e compila normalmente |
| segundo `cargo check` sem mudanças | não corrompe estado; wrapper pode não ser chamado se Cargo usar cache |
| `cargo build`, `test`, `run` e `clippy` | continuam com a sintaxe normal e usam o wrapper quando compilam membros do workspace |
| CI com target limpo | confirma a participação do wrapper nas compilações primárias |
| workspace virtual | identifica raiz e membros corretamente |
| dois projetos com mesmo nome | gera chaves distintas |
| duas worktrees | compartilha projeto e separa workspace state |
| compilação paralela | nenhuma perda ou JSON parcial |
| build script e proc-macro | delegação correta, sem auditoria indevida |
| target WASM | preserva target e diagnósticos |
| falha do `rustc` | mesmo exit code e diagnóstico |
| falha interna do AntDelirium | erro classificado, sem simular sucesso |
| wrapper já existente | conflito detectado; configuração não sobrescrita |
| `RUSTC_WORKSPACE_WRAPPER=""` | `doctor` detecta; CI ainda bloqueia |
| configuração local substitui global | `doctor` aponta arquivo e precedência |
| tentativa de editar estado | rejeitada ou registrada conforme autoridade disponível |
| migração do AutoBrowser | hashes, planos e histórico conferem |
| primeiro `cargo check` governado | gera `guard/to-do_list.json` e `guard/governance_error.json` sem caminhos externos |
| agente lê as projeções | encontra somente tarefa atual, erros corrigíveis e caminhos relativos ao projeto |
| agente altera ou apaga uma projeção | estado canônico não muda; projeção é regenerada |
| agente procura `plan/` fora do projeto | sandbox ou autoridade do serviço impede leitura e escrita |
| agente envia `hierarchy.task.current.submit` | cria submissão, mas não marca a tarefa como implementada |
| agente tenta `implemented=true` ou override | operação pública é rejeitada e registrada |
| `strings -a antdelirium` e inspeção ELF | não encontram caminho absoluto protegido nem caminho de build do mantenedor no release |
| `strace -f -e trace=%file cargo check` | nenhum PID sob autoridade do agente abre, testa ou enumera planos e estado canônico |
| `strace -p <serviço-protegido>` | attach do agente é recusado |
| inspeção de `/proc/<pid>` do serviço | ambiente, argumentos e descritores protegidos não ficam disponíveis ao agente |
| busca por caminhos absolutos em `guard/` | não encontra `/home`, `/tmp`, `/var`, `/etc` ou qualquer raiz externa |
| inspeção do workspace após compilação | não existe `login.json` administrativo nem caminho externo gerado pelo AntDelirium |
| busca por logs temporários | AntDelirium não cria `/tmp/delirium_debug.log`, `/tmp/delirium_error.log` ou equivalente previsível |
| captura do protocolo limitado | IDs são opacos e respostas não contêm caminhos físicos externos |

## 16. Decisões que ainda precisam ser fechadas

1. **Fechada na T1.2:** cliente user-local não confiável, serviço systemd sob UID dedicado e CI/supervisor como autoridade final.
2. **Fechada na T1.3:** SQLite/WAL protegido é canônico; eventos permanecem append-only em tabela transacional e JSON é apenas projeção/exportação.
3. **Fechada na T6.7:** até `25 ms` adicionais por entrada do wrapper; checks
   limpo, incremental e no-op têm tetos pareados de `2 s`, `1 s` e `250 ms`.
4. Auditoria ocorre antes, depois ou em paralelo ao `rustc` em modo de produção?
5. Quais violações bloqueiam e quais apenas geram dados?
6. Como compor com `sccache` e outros wrappers já instalados?
7. **Fechada na T1.8:** provisório vazio pode ser quarentenado após 30 dias; qualquer state governado tem retenção mínima de 180 dias e nunca sofre purge automático.
8. Quais partes do layout AutoBrowser são canônicas e quais são apenas exportações compatíveis?
9. **Fechada na T1.8:** cutover exige operador `state.admin` mais attestation independente; rollback manual exige operador e gatilhos objetivos podem usar autorização pré-aprovada.
10. Qual autoridade externa registrará ou validará comandos totalmente atendidos pelo cache quando essa prova for necessária?
11. **Fechada na T1.8:** permanecem canônicos até stage, paridade e aprovação; depois do cutover o SQLite protegido é canônico e a origem fica congelada/recuperável por no mínimo 30 dias.
12. **Fechada na T1.6:** sockets/listeners separados, `SO_PEERCRED` e capability de 256 bits vinculada a UID, ator e escopo; supervisor entrega capabilities protegidas por descriptor.
13. **Fechada na T1.8:** regra autocontida e exata em `guard/.gitignore` somente quando necessária; nunca editar root ignore, index, exclude ou configuração Git automaticamente.
14. **Fechada na T1.2:** UID dedicado, ausência de `CAP_SYS_PTRACE`, Yama, daemon não dumpable, AppArmor e sandbox do agente com PID namespace/procfs oculto.
15. **Fechada na T1.3:** estado canônico migra para SQLite protegido; o cliente recebe somente projeções JSON sanitizadas e nunca o path do banco.
16. Quais caminhos e nomes serão proibidos pela auditoria estática do release sem transformar obscuridade em requisito de segurança?

## 17. Definição de pronto da evolução

O trabalho estará concluído somente quando:

- houver um único binário `antdelirium`;
- a instalação configurar `$CARGO_HOME/config.toml`, não o `Cargo.toml` dos projetos;
- o projeto declarar seu MSRV, nunca inferior ao piso de protocolo Cargo 1.50, e permanecer validado na versão 1.93.1 atual;
- a integração usar o `rustc` real de forma transparente;
- projetos novos materializarem estado fora do repositório;
- nenhum projeto precisar de binário ou guard em `plugins/`;
- projetos governados receberem somente `guard/to-do_list.json` e `guard/governance_error.json` como projeções locais encontráveis;
- essas projeções nunca revelarem caminhos de planos, plugins, TOMLs, orquestrador ou state root externo;
- o agente não conseguir ler nem escrever os planos autoritativos;
- uma submissão do agente não conseguir marcar a própria tarefa como implementada, revisada ou aprovada;
- o processo wrapper do agente nunca abrir, testar ou enumerar o armazenamento autoritativo;
- `strings`, metadados ELF, argumentos, ambiente, syscalls e projeções não revelarem caminhos físicos protegidos;
- o agente não conseguir anexar tracer/debugger nem inspecionar ambiente ou descritores do serviço protegido;
- nenhum arquivo administrativo local, `login.json` com caminho absoluto ou log previsível em `/tmp` for gerado;
- nenhum agente precisar trocar `cargo check`, `cargo build` ou `cargo test` por um comando AntDelirium;
- o proxy `cargo` gerenciado pelo rustup permanecer intacto;
- Cargo continuar compilando corretamente com todos os targets suportados;
- dados concorrentes forem atômicos e recuperáveis;
- o agente receber ações de código, não instruções para editar JSON;
- a documentação não prometer proteção que o mesmo usuário pode contornar;
- existir um gate externo para enforcement contra bypass intencional;
- a migração do AutoBrowser estiver validada e possuir rollback;
- toda a suíte do AntDelirium estiver verde.

## 18. Não objetivos

Esta evolução não pretende:

- reimplementar o compilador Rust;
- esconder o funcionamento do AntDelirium por obscuridade;
- impedir engenharia reversa de um binário entregue ao mesmo usuário;
- controlar o shell do agente por comportamento não documentado;
- copiar repositórios para `$CARGO_HOME`;
- tratar JSON editável pelo mesmo usuário como prova criptográfica;
- substituir revisão humana, CI e controle de acesso do sistema operacional.

O não objetivo de impedir engenharia reversa não autoriza vazamentos evitáveis. O agente pode compreender que o produto possui planos e governança; ele não pode obter por isso a localização física, credenciais, descritores ou autoridade para ler e alterar o estado canônico.

O objetivo é separar responsabilidades, reduzir caminhos de bypass acidental, centralizar a implantação e criar uma base na qual enforcement real possa ser adicionado por uma autoridade externa.
