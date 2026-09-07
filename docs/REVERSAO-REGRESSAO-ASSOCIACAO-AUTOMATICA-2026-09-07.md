# Reversão da associação automática e recuperação do serviço

Data da auditoria: 7 de setembro de 2026.

## Resultado

A tentativa de associar automaticamente um checkout Rust ao candidato legado
do AutoBrowser não implementa uma migração válida. Ela introduz seleção global
por prefixo, permite reatribuir identidades persistidas, altera tarefa canônica
sem autorização e substitui o instalador transacional por uma cópia direta como
root.

O candidato preparado deve ser preservado para análise. Nenhum passo desta
reversão autoriza apagar o candidato, os planos de origem, o backup Git privado,
o registry ou qualquer arquivo do AutoBrowser.

## Estado comprovado antes da regressão

A aceitação instalada de 6 de setembro de 2026 registrou:

- release validado: `685adabac5767228685f88c57901063cccc903a868f22a85b0e70ada0c0613c7`;
- wrapper, serviço e bundle com o mesmo hash;
- `doctor=ready`;
- `cargo check`, `cargo build` e `cargo test` aprovados;
- exatamente dois arquivos públicos em `guard/`;
- ausência de localizadores físicos.

Essa evidência está em
`target/t8g-generic-installation-logs/20260906T134149Z-30966/acceptance.env`.

## Estado encontrado em 7 de setembro

- binário instalado: `55dcb1d1b7347988ca0d70900f09e48880c30ce4353a4ebfb2fe78d0c08d0a95`;
- release presente em `target/`: `14a7016756166afaee49345a3b18f5477fd8d895fcae5d76c47e397ac9ff14b2`;
- o arquivo-fonte foi alterado depois desse release, portanto nem esse release
  representa a árvore atual;
- o ELF instalado contém o novo `DO UPDATE SET project_id = excluded.project_id`,
  as consultas por primeiro `rowid` e uma versão intermediária da tarefa falsa
  `Governance Baseline` que já não aparece na fonte atual;
- não existe recibo do instalador transacional T8.6 para a substituição de hoje.

## O que a preparação do AutoBrowser realmente concluiu

O run `20260907T155137Z` concluiu somente staging, normalização, dry-run e
conversão de um candidato protegido:

- `migration_id=mig_6qfejwtpv2vwdsfmnvx3eeans9`;
- `project_id=prj_1c70qd00vhn8yy0s9tm54rzmzx`;
- `workspace_id=wsp_7zb4nnnfgrrvykeewn3v3tdtj5`;
- `differences=6`;
- `readiness=blocked`;
- `approval_created=false`;
- `cutover=false`;
- estado final `candidate_ready_for_independent_attestation`.

Logo, as tarefas não foram associadas ao checkout ativo. O projeto provisório
que continuou sendo projetado pelo `cargo check` não foi escolhido por ordem
alfabética: ele era a identidade já vinculada ao fingerprint do checkout.

## Reversão obrigatória na fonte

### `src/Service-Storage_unit.rs`

Remover somente as alterações de associação implícita introduzidas em 7 de
setembro:

1. remover a procura global por `project_key.starts_with("legacy-candidate--")`
   em `bootstrap_provisional`;
2. restaurar `registration` como resultado imutável da consulta por fingerprint;
3. remover o registro fallback do primeiro workspace;
4. remover as escritas ignoradas com `let _ =` no registry e banco do candidato;
5. remover as consultas `ORDER BY rowid LIMIT 1` usadas para escolher plano e
   tarefa;
6. remover os `UPDATE` diretos de `tasks` e `projects`;
7. remover a segunda implementação da mesma associação em
   `create_provisional_registration`;
8. restaurar a regra: identidade já conhecida reutiliza somente seu projeto;
   identidade nova cria um projeto novo, nunca escolhe candidato global;
9. remover imports SQLite adicionados exclusivamente para as mutações acima.

Não reverter as correções anteriores de auditoria de fontes, backup baseline,
projeções ou storage-v13. A reversão deve ser feita por trechos e arquivos
exatos; `git checkout`, `git reset` e restauração ampla são proibidos porque a
árvore contém trabalho válido ainda não consolidado.

### `src/structs/Transactional_Storage_structcore.rs`

1. restaurar `provisional_project_identities` para `ON CONFLICT ... DO NOTHING`;
2. restaurar a leitura posterior do vínculo e rejeitar projeto diferente com
   erro de integridade;
3. remover o `DO UPDATE` de `provisional_project_registrations`;
4. manter colisão como falha, sem reatribuição silenciosa;
5. remover `registered_workspaces_for_project` enquanto ele servir apenas à
   seleção pelo primeiro workspace. Uma API futura deve consultar um workspace
   exato por ID;
6. adicionar teste que prove que um fingerprint não pode mudar de projeto nem
   de workspace.

### `update-antdelirium-service.sh`

Restaurar o launcher seguro que:

1. constrói `system-installation-bundle` como usuário não root;
2. delega a instalação a `packaging/systemd/upgrade-installed-service.sh`;
3. não copia diretamente um binário de `target/` como root;
4. preserva backup, migração de schema, ownership, hashes, wrapper Cargo e logs;
5. interrompe diante de falha ao parar qualquer unidade.

### `packaging/systemd/cutover-installed-autobrowser-candidate.sh`

O script não rastreado não pode ser usado nem promovido porque:

- fixa IDs e hashes pertencentes a outra execução;
- mistura esses valores com o run escolhido no argv;
- executa `source` como root sobre um arquivo no repositório gravável;
- não valida que attestation e approval pertencem ao candidato atual;
- afirma que 73 tarefas estão ativas sem consultar o recibo canônico final.

Ele deve ser retirado da rotina operacional. A futura ferramenta oficial deve
parsear dados como dados, nunca executar um `.env`, e obter todos os bindings de
um recibo autenticado da mesma migração.

### Evidência de inventário sobrescrita

`docs/evidence/autobrowser-legacy-source-inventory-v1.json` foi alterado no
lugar em vez de preservar a evidência anterior. A captura, o digest e quatro
registros de ownership mudaram de `65534:65534` para `0:0`.

A correção deve:

1. restaurar o inventário versionado anterior como evidência imutável;
2. se a captura de 7 de setembro for necessária, gravá-la com outro ID/nome e
   explicar por que foi executada sob ownership diferente;
3. nunca tratar mudança causada por execução como root como equivalência da
   origem;
4. vincular qualquer novo inventário ao migration ID correspondente.

## Estado dos gates locais

- `cargo fmt --all -- --check` e `cargo check --locked --all-targets --target
  x86_64-unknown-linux-gnu` passam;
- os 23 testes de service storage passam;
- um teste novo cria um candidato não relacionado e prova que o bootstrap cria
  outro projeto em vez de capturá-lo;
- outro teste novo prova que um fingerprint persistido não pode ser reatribuído
  a outro projeto/workspace;
- o gate completo passou com 184 testes da biblioteca, 191 testes aprovados do
  binário, oito testes externos ignorados, nove testes de integração e auditoria
  ELF sem localizadores protegidos.

## Diagnóstico e correção do estado protegido

A reversão de fonte não basta porque o daemon contaminado foi instalado. Antes
de reinstalar, uma rotina somente leitura deve registrar, sem paths físicos:

1. hash do serviço, wrapper, bundle e release revisado;
2. versão do schema e `quick_check` de todos os bancos;
3. vínculos de fingerprints que mudaram desde o último checkpoint assinado;
4. registros provisórios apontando para `legacy-candidate--`;
5. projetos/workspaces com binding ausente ou contraditório;
6. tarefas/plano/revisão alterados sem evento canônico correspondente;
7. tarefas sintéticas `Governance Baseline` inseridas pelo binário intermediário;
8. capabilities ainda ativas emitidas durante a tentativa;
9. integridade dos backups Git e checkpoints T9.6/T9.8.

Se houver mutação, a correção deve ser uma operação administrativa auditada e
reentrante. Ela deve restaurar os vínculos e o estado exatos do último
checkpoint válido, preservar eventos posteriores para investigação e nunca
executar SQL manual destrutivo.

## Recuperação do daemon instalado

Depois da reversão e dos testes locais:

1. executar `make ci`;
2. executar `make system-installation-bundle` sem root;
3. auditar o ELF e guardar seu SHA-256;
4. usar somente `packaging/systemd/upgrade-installed-service.sh` para instalar;
5. comparar serviço, wrapper, bundle e release;
6. executar `doctor` e a aceitação genérica instalada;
7. repetir o diagnóstico de integridade do registry;
8. só então retomar qualquer migração.

## Critérios de conclusão da reversão

- nenhuma ocorrência de seleção automática por `legacy-candidate--` no
  bootstrap;
- nenhum `DO UPDATE` pode reatribuir fingerprint;
- nenhuma escolha de plano/tarefa por `rowid` fora do store canônico;
- nenhum erro de mutação crítica descartado com `let _ =`;
- `cargo fmt --check`, `cargo check --all-targets`, testes completos e auditoria
  ELF aprovados;
- serviço, wrapper, bundle e release com o mesmo hash revisado;
- candidato do AutoBrowser preservado e ainda não associado sem aprovação;
- zero acesso de escrita ao checkout do AutoBrowser.

## Resultado final

Em 7 de setembro de 2026, a recuperação instalada foi concluída:

- serviço, wrapper Cargo, bundle e release ficaram no mesmo digest
  `685adabac5767228685f88c57901063cccc903a868f22a85b0e70ada0c0613c7`;
- a aceitação genérica comprovou `doctor=ready`, `cargo check`, `cargo build`,
  `cargo test` e as duas projeções públicas;
- os 33 bancos permaneceram em storage-v13, com `quick_check` aprovado e zero
  violações de foreign key;
- houve zero associação provisória ao candidato, zero contradição de identidade
  ou escopo, zero tarefa `Governance Baseline` e zero ponteiro canônico inválido;
- o novo checkpoint externo assinado foi criado e verificado;
- o modo explícito de recuperação preservou separadamente os dois binários
  divergentes anteriores antes de substituí-los, mantendo rollback exato.

Evidências instaladas: `target/t86-capability-upgrade-logs/20260907T220037Z`,
`target/t8g-generic-installation-logs/20260907T220048Z-190611`,
`target/t96-installed-integrity-logs/20260907T220300Z` e
`target/t110-installed-regression-diagnostics/20260907T220302Z`.
