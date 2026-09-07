# Plano da API oficial de geração de tarefas e migração — Onda 11

## 1. Objetivo

Criar um fluxo oficial e genérico para transformar uma origem revisada em
tarefas canônicas e, quando necessário, associar um candidato migrado à
identidade real observada pelo Cargo.

O fluxo não pode depender do nome AutoBrowser, do primeiro projeto no banco, do
primeiro workspace, de paths fornecidos pelo agente ou de edição direta das
projeções públicas.

## 2. Fronteiras de autoridade

### Wrapper e `cargo check`

Continuam responsáveis somente por:

- descobrir fingerprints do projeto e workspace atuais;
- registrar ou reutilizar a identidade exata;
- observar a compilação e produzir o backup privado;
- atualizar as duas projeções públicas.

Compilar nunca gera, importa, aprova, associa ou ativa tarefas administrativas.

### API limitada do agente

Permanece com as quatro ações existentes: tarefa atual, regra relacionada,
próxima ação e submissão de evidência. Nenhuma operação da Onda 11 aparece na
ajuda ou no protocolo limitado.

### API administrativa

Geração, inspeção, attestation, approval, instalação e associação usam o canal
administrativo, capabilities escopadas e descritores somente leitura. O serviço
recebe IDs opacos e bytes; nunca recebe `--project-root`, path de plano ou
localização da origem.

## 3. Dois fluxos oficiais

### 3.1 Importação somente de tarefas

Este é o caminho preferido quando se quer trazer objetivos e critérios, mas não
o histórico canônico antigo:

1. `cargo check` registra a identidade real do projeto;
2. o operador fornece uma origem por descritor ao gerador;
3. o gerador produz uma proposta imutável e atribui IDs opacos;
4. um verificador independente atesta o resumo e o digest;
5. o operador aprova a proposta exata;
6. `governance.task-package.install` instala atomicamente no projeto/workspace
   explicitamente selecionados;
7. outro `cargo check` apenas atualiza as projeções.

Checkboxes, grades ou `implemented=true` da origem são evidência histórica e
nunca viram conclusão canônica automaticamente.

### 3.2 Migração com histórico

Quando é necessário preservar múltiplos planos, histórico e bindings:

1. reutilizar stage, normalize, dry-run, conversion, attestation, approval e
   cutover existentes;
2. resolver todas as diferenças antes do approval;
3. associar a identidade observada somente por uma nova operação administrativa
   explícita;
4. preservar ou quarentenar o projeto provisório conforme política aprovada;
5. nunca modificar a seleção de tarefa produzida pela conversão.

## 4. Contratos versionados

### `task-generation-source-v1`

Entrada genérica e limitada contendo:

- `request_id` e formato de origem declarado;
- objetivos, critérios, ordem e paths relativos permitidos;
- referências lógicas à origem por digest, nunca path físico;
- nenhum estado canônico, capability, ID de banco ou decisão de conclusão.

### `task-package-proposal-v1`

Artefato protegido gerado pelo serviço contendo:

- `proposal_id`, digest da origem e digest do payload;
- planos e tarefas em ordem explícita, nunca `rowid`;
- IDs opacos atribuídos uma vez;
- seleção proposta de plano/tarefa inicial explicitamente representada;
- contagens, limites e avisos de conversão;
- estado `prepared`, sem autoridade para instalação.

### `task-package-attestation-v1` e `task-package-approval-v1`

Vinculam proposta, origem, projeto, workspace, revisão esperada e identidade do
verificador/operador. Reutilizar os padrões de digest, expiração, replay e
separação de papéis já aplicados à migração protegida.

### `governance-task-package-v2`

Evolução compatível do pacote atual para suportar, de forma limitada:

- um ou mais planos ordenados;
- tarefas com ordinal explícito;
- um único plano ativo e uma única tarefa corrente declarados;
- instalação somente sobre projeto vazio na revisão esperada;
- vínculo obrigatório à proposta atestada e aprovada.

A versão v1 permanece aceita somente em sua operação atual. Não converter v1
implicitamente nem enfraquecer sua validação.

### `migration-identity-association-v1`

Pedido administrativo que contém exatamente:

- migration, cutover e approval IDs;
- projeto/workspace provisórios observados;
- projeto/workspace candidatos de destino;
- fingerprints esperados já presentes no registry;
- digests do candidato e do binding de cutover;
- política explícita para o estado provisório.

O pedido não contém prefixo, slug, path ou seleção por posição.

### Recibos

Cada operação produz recibo path-free, serializável e auditável com request ID,
resultado, IDs opacos, revisões anterior/posterior e digests. Retry idêntico
devolve o mesmo recibo; payload conflitante falha.

## 5. Operações administrativas

Preservar `operator task-package install` e acrescentar uma gramática fechada:

- `operator task-package prepare`;
- `operator task-package inspect`;
- `operator task-package attest`;
- `operator task-package approve`;
- `operator task-package install` para v1/v2;
- `operator migration identity-associate`.

Entradas grandes chegam por `ANTDELIRIUM_*_FD` herdado, limitado, regular,
somente leitura e `CLOEXEC`. Argumentos de linha de comando nunca carregam
paths, JSON, tokens ou fingerprints.

## 6. Semântica da associação

A associação deve executar em uma única transação do registry:

1. autenticar capability `StateAdmin` escopada;
2. localizar todos os recursos por IDs exatos;
3. confirmar que o destino possui binding de cutover válido;
4. confirmar que a origem provisória corresponde aos fingerprints esperados;
5. recusar origem com tarefa/histórico não vazio até haver política explícita de
   reconciliação;
6. inserir um binding novo sem sobrescrever colisão;
7. registrar evento e recibo na mesma transação;
8. tornar o vínculo antigo inativo ou quarentenado sem apagá-lo;
9. preservar a tarefa corrente já validada no candidato;
10. permitir replay idêntico e rejeitar qualquer associação concorrente.

Não usar `ON CONFLICT DO UPDATE` para identidade. Não listar candidatos para
escolher o primeiro. Não usar `project_key` como autoridade.

## 7. Geração de tarefas

O gerador é determinístico para a mesma requisição e não é um agente autônomo.
Ele valida e normaliza uma proposta fornecida pelo operador. Se futuramente um
modelo produzir propostas, sua saída entra pelo mesmo contrato sem capability
de aprovação ou instalação.

Regras mínimas:

- preservar texto e ordem aprovados;
- gerar IDs opacos sem incorporar nomes ou paths;
- rejeitar duplicatas, ciclos, tarefas vazias e paths proibidos;
- limitar quantidade, bytes e profundidade antes de alocar;
- não inferir conclusão, revisão, grade ou plano ativo de checkboxes antigos;
- registrar diferenças e itens não convertidos em relatório protegido;
- expor ao operador apenas resumo sanitizado e digests.

## 8. Recuperação e compatibilidade

- detectar registros contaminados pelo daemon de 7 de setembro antes de migrar;
- nunca associar um projeto provisório não vazio silenciosamente;
- manter backup exato do registry e bancos afetados antes da operação;
- preservar candidatos preparados e rejeitados;
- suportar rollback da associação sem apagar eventos posteriores;
- manter storage-v13 legível e migrá-lo aditivamente para storage-v14, necessário
  para os registros protegidos e separados de proposta, attestation e approval;
- manter o wrapper e a API limitada binariamente compatíveis.

## 9. Matriz obrigatória de testes

### Unidade e protocolo

- contrato válido, truncado, grande demais, desconhecido e com campo extra;
- IDs, ordinais, digests e paths inválidos;
- capabilities erradas, expiradas, revogadas ou de outro papel;
- replay idêntico e request ID conflitante;
- nenhuma operação nova disponível no canal do agente.

### Identidade

- zero, um e vários candidatos;
- zero, um e vários workspaces;
- dois projetos homônimos;
- linked worktrees compartilhando projeto e separando workspace;
- fingerprint já vinculado a outro projeto;
- associação concorrente;
- ausência de seleção por ordem SQL, prefixo ou slug.

### Estado canônico

- instalação v1 permanece compatível;
- v2 instala múltiplos planos em ordem explícita;
- somente uma tarefa fica corrente;
- projeto não vazio falha sem mutação;
- checkbox legado não conclui tarefa;
- falha entre escrita e evento faz rollback integral;
- projeções só mudam depois do commit canônico.

### Instalação

- serviço, wrapper, bundle e release com hash idêntico;
- nenhuma compilação como root;
- backup e rollback reais;
- ELF e logs sem paths protegidos;
- fixture Rust interna primeiro;
- projeto externo somente no aceite operacional autorizado posterior.

## 10. Ordem de implementação

1. concluir a reversão e recuperar o daemon instalado;
2. congelar os contratos e invariantes da Onda 11;
3. implementar o gerador puro e seus testes sem tocar no serviço;
4. persistir proposta, attestation e approval em storage protegido;
5. estender o protocolo administrativo por descritores;
6. implementar instalação v2 transacional;
7. implementar associação explícita e quarentena reentrante;
8. criar launchers operacionais sem paths nos argumentos;
9. executar matrizes hostis e `make ci`;
10. provar o ciclo instalado com fixture sintética;
11. somente então preparar o aceite do AutoBrowser real.

## 11. Critério de conclusão

A API está pronta quando um projeto Rust vazio recebe tarefas por pacote oficial
e um candidato migrado pode ser associado à identidade exata somente após
attestation e approval, enquanto qualquer tentativa automática, ambígua ou
originada do perfil agente falha sem alterar registry, tarefa ou projeções.

O aceite do AutoBrowser é uma operação separada: ele comprova o primeiro uso da
API, mas não autoriza código específico do AutoBrowser dentro do daemon.
