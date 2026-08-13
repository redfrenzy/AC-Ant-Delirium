# Contrato de atores e capabilities

Versão do contrato: `actors-v1`  
Tarefa de origem: T1.1  
Estado: decisão arquitetural da Onda 1

## Objetivo

Este contrato define quem pode observar, submeter e alterar cada classe de dado do AntDelirium. Ele separa autorização lógica de implementação: usuário do serviço, socket, credencial, sandbox e política de `ptrace` serão escolhidos nas tarefas seguintes, mas nenhuma implementação poderá conceder mais autoridade do que a matriz abaixo.

O nome do executável, cwd, argumento de linha de comando ou conhecimento de um ID não constitui autorização.

## Atores

| Ator | Papel | Autoridade de execução esperada |
|---|---|---|
| wrapper Cargo | delegar ao `rustc`, observar compilações primárias e solicitar atualização de projeções | mesma autoridade limitada do agente |
| agente de implementação | alterar o código da missão atual e enviar evidências | sandbox limitado ao projeto atribuído |
| revisor | avaliar uma submissão atribuída e emitir uma decisão limitada | capability externa de revisão, sem administração global |
| operador/supervisor | instalar, migrar, diagnosticar e administrar políticas | autoridade administrativa fora do alcance do agente |
| serviço protegido | resolver IDs opacos, manter estado canônico e aplicar transições válidas | domínio de segurança diferente do agente |
| CI autoritativo | recompilar em ambiente limpo e emitir attestations verificáveis | ambiente protegido que o agente não controla |

## Classes de recurso

1. **Projeto:** fontes, manifests, testes e arquivos de build pertencentes ao projeto.
2. **Projeção do agente:** somente `guard/to-do_list.json` e `guard/governance_error.json`.
3. **Submissão:** evidência proposta por agente, revisor ou CI; nunca equivale a conclusão autoritativa.
4. **Estado canônico:** planos, orquestrador, flags de implementação/revisão, histórico e fila administrativa.
5. **Configuração bruta:** TOMLs, schemas internos, registro de projetos e mapeamentos físicos.
6. **Segredos e identidade:** chaves, capabilities, credenciais, descritores e metadados privados do serviço.
7. **Instalação:** binário global, configuração Cargo, migração, rollback e atualização.

## Matriz de permissões

Legenda: `R` leitura; `W` escrita direta; `S` submissão sem mutação canônica; `D` decisão limitada aplicada pelo serviço; `A` administração; `—` negado.

| Recurso/operação | wrapper | agente | revisor | operador | serviço | CI |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| ler fontes do projeto necessárias à operação | R | R | R limitada à revisão | R | R para validação | R |
| alterar fontes do projeto | — | W na missão atribuída | — | W excepcional | — | — |
| executar/delegar toolchain | W operacional | via Cargo | R dos resultados | A | — | W operacional |
| ler as duas projeções em `guard/` | R | R | R | R | R/W | R |
| alterar ou apagar projeção como estado autoritativo | — | — | — | A | W/regenerar | — |
| criar submissão de implementação | S automática de compilação | S | — | A | registrar | S de attestation |
| ler submissão própria | — | R | R quando atribuída | R | R | R da própria execução |
| listar submissões de outros projetos/agentes | — | — | somente as atribuídas | A | R | — |
| aceitar/rejeitar submissão atribuída | — | — | D | A | aplicar decisão válida | — |
| marcar `implemented`, `reviewed`, `task_reviewed` ou `grade` | — | — | — | A excepcional | W após política | — |
| alterar `active_plan`, modo ou avanço do orquestrador | — | — | — | A | W após política | — |
| ler planos físicos, orquestrador e histórico completo | — | — | — | A | R | — |
| resolver ID opaco para caminho físico | — | — | — | A | R | — |
| ler/alterar TOMLs e registro privado | — | — | — | A | R/W | — |
| instalar, atualizar, migrar ou remover integração Cargo | — | — | — | A | — | A apenas no ambiente CI |
| ler chaves, tokens ou credenciais do serviço | — | — | — | A conforme política | R interno | — |
| assinar evento canônico | — | — | S de decisão autenticada | A | W/assinar | S de attestation |
| consultar diagnóstico detalhado com caminhos | — | — | — | A | R interno | A do próprio ambiente |

## Capabilities mínimas

As capabilities são estreitas, vinculadas a ator, projeto, workspace, tarefa e validade temporal quando aplicável.

| Capability | Portador permitido | Efeito máximo |
|---|---|---|
| `project.bootstrap.observe` | wrapper em socket limitado | criar registro provisório vazio, sem importar ou acessar plano |
| `compile.observe` | wrapper, CI | enviar fingerprint e resultado de compilação sem acessar estado canônico |
| `projection.refresh` | wrapper, serviço | solicitar ou materializar somente as duas projeções sanitizadas |
| `task.current.read` | agente, revisor | obter tarefa atual, regra relacionada e caminhos relativos ao projeto |
| `submission.create` | agente | criar evidência imutável vinculada à tarefa atribuída |
| `submission.read.assigned` | agente, revisor | ler somente submissão própria ou explicitamente atribuída |
| `review.decide.assigned` | revisor | aceitar ou rejeitar a submissão atribuída; não editar plano |
| `attestation.create` | CI | registrar resultado de build/teste limpo e identidade do input |
| `state.transition` | serviço | aplicar transição canônica somente após política e evidências válidas |
| `state.admin` | operador | administrar planos, overrides, migração e histórico |
| `installation.admin` | operador, CI protegido | instalar ou remover integração no domínio autorizado |
| `diagnostic.private` | operador | consultar caminhos, mapeamentos e falhas internas sem projetá-los ao agente |

Nenhuma capability pública combina submissão com aprovação. `submission.create` nunca implica `state.transition`.

## Regras por ator

### Wrapper Cargo

- recebe o caminho do `rustc` real e seus argumentos, mas não recebe caminho de state root, plano ou TOML privado;
- não abre, testa, faz `stat`, resolve symlink nem enumera armazenamento canônico;
- pode ler apenas o contexto Cargo/projeto necessário e escrever streams/status por delegação;
- solicita ao serviço uma projeção sanitizada e grava somente os dois arquivos de `guard/` quando essa responsabilidade estiver no cliente;
- probes, dependências não primárias e consultas de toolchain não criam autoridade adicional.

### Agente de implementação

- trabalha no projeto e encontra deliberadamente os dois JSONs de `guard/`;
- recebe tarefa atual, erros corrigíveis, regra relacionada, próxima ação e caminhos relativos ao projeto;
- envia evidência, mas não marca checkbox, grade, revisão, plano ativo ou modo;
- não possui comando ou help que revele instalação, migração, paths físicos, planos, TOMLs ou manutenção de outros agentes;
- alterar/apagar a projeção não altera o estado canônico; o serviço a regenera.

### Revisor

- enxerga apenas tarefa, submissão, diff/evidência e attestations necessários à revisão atribuída;
- emite aceitar/rejeitar com justificativa autenticada;
- não escolhe outro plano, não altera a própria atribuição e não edita campos canônicos diretamente;
- a decisão só produz transição quando o serviço valida escopo, freshness, identidade e política.

### Operador/supervisor

- é o único ator humano com superfície administrativa completa;
- pode instalar, migrar, diagnosticar, executar rollback, gerir planos e conceder/revogar capabilities;
- ações destrutivas ou de migração exigem alvo explícito, staging, hashes e trilha de auditoria;
- sua ajuda/API não pode ser servida automaticamente a um cliente identificado apenas como agente.

### Serviço protegido

- é a única autoridade online que resolve IDs opacos para caminhos e escreve estado canônico;
- valida ator, capability, projeto, workspace, tarefa, revisão e attestation antes de transições;
- retorna somente dados mínimos e sanitizados para wrapper, agente e revisor;
- não entrega segredo, caminho físico, argumento administrativo ou descritor privilegiado ao processo do agente.

### CI autoritativo

- compila a entrada identificada em target limpo e verifica participação/integridade do wrapper;
- emite attestation, não marca tarefa como concluída por conta própria;
- não usa estado ou credencial fornecidos pelo agente como autoridade final;
- falha se o wrapper esperado for removido, substituído ou contornado na compilação primária.

## Invariantes verificáveis

1. Nenhuma requisição sob capability de agente consegue ler ou escrever plano físico, TOML privado ou orquestrador.
2. Nenhuma submissão consegue aprovar a si própria ou alterar `implemented`, `reviewed`, `grade` ou `active_plan`.
3. Somente o serviço e o operador resolvem IDs opacos; somente o operador recebe o caminho resultante.
4. O wrapper e o agente não possuem capability administrativa, mesmo que invoquem um subcomando conhecido.
5. O revisor não expande o próprio escopo e não revisa tarefa não atribuída.
6. O CI emite evidência; o serviço aplica política e transição.
7. Ajuda e erros são filtrados pelo perfil autenticado antes de renderizar comandos ou paths.
8. Negação de autorização não confirma a existência de projeto, plano, arquivo ou ID fora do escopo.
9. Toda decisão, transição, concessão e revogação produz evento auditável sob autoridade protegida.
10. Cwd, nome do processo, variável de ambiente e posse do binário cliente nunca elevam capability.

## Decisões delegadas às próximas tarefas

Este contrato fixa permissões, mas não antecipa escolhas que pertencem às próximas etapas:

- T1.2: domínio de segurança, usuário do serviço, sandbox, `ptrace` e `/proc`;
- T1.3: backend canônico e transações;
- T1.5: formato e colisão dos IDs opacos;
- T1.6: transporte, autenticação do peer e envelope de capability;
- T1.7: códigos de erro sem oráculo de descoberta;
- T1.8: retenção, Git, migração e rollback.
