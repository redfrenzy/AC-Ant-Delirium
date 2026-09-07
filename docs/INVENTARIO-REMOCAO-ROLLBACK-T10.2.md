# Inventário de remoção do rollback antigo — T10.2

## Objetivo

Este documento registra a fronteira aplicada pela T10.2. O nome
`legacy` também é usado por recursos atuais de migração; portanto a remoção não
pode ser feita por busca textual, prefixo de arquivo ou diretório inteiro.

O ponto de recuperação revisado antes da retirada é o commit
`de355fe78ca958988b8c00ebf14d442e182df1f2`. A exclusão estrutural foi executada
depois de G9 comprovar o enforcement local da T9.7 e a substituição do commit
automático na T9.8. Repositório remoto e branch protection não são
pré-requisitos do produto.

## Remoção aplicada depois de G9

### Entradas removidas do runtime antigo

- `src/Agent-Runtime_unit.rs` e `src/Agent-Runtime_unitcore.rs`;
- `src/Main-Logic_unit.rs` e `src/Main-Logic_unitcore.rs`;
- `src/guard_main.rs`;
- `get_toml_for_new/ant_delirium_build_guard.rs`;
- os includes e módulos `agent_runtime`, `legacy_guard_mode` e
  `run_legacy_cli_mode` em `src/main.rs`;
- as variantes internas `LegacyAgentCliRollback` e `LegacyGuardRollback`, seus
  match arms e seus testes de compilação em `src/Binary-Mode_unit.rs`.

Depois que T9.8 provar paridade de backup, contexto e recuperação, saem apenas
as funções e superfícies legadas dessas rotas:

- chamadas legadas a `capture_project_snapshot`, `commit_governance_state`,
  `hierarchy.git.*` e `hierarchy.context.*` junto do runtime antigo;
- os shell-outs e handlers inseguros em `src/Git_unitcore.rs` depois que seus
  consumidores tiverem migrado para o módulo protegido.

O módulo de backup Git privado e `gix` devem permanecer. `Git_unit` pode ser
refatorado ou renomeado, mas a funcionalidade não sai; somente os entrypoints,
shell-outs e handlers inseguros são retirados.
Esses componentes não permanecem no produto nem no ELF único.

### Contrato Super Block removido do guard local

O antigo `Super Block` ainda descreve e verifica um `build.rs` que chama um
guard copiado para o projeto, binários dentro de `plugins/` e TOMLs externos.
Isso contradiz a integração global já implantada. Depois de G9 devem sair em um
mesmo commit:

- `src/rules/axiom_sanitization/Super_Block_Axiom_unit.rs` e
  `Super_Block_Axiom_unitcore.rs`;
- a chamada direta de `run_preflight` em `src/engine/Swarm_Engine_unit.rs`;
- o registro de `SuperBlockAxiom` e o módulo correspondente;
- o verbete e as mensagens do Super Block em `src/rules/Rule_Docs_unit.rs` e
  os classificadores exclusivos desse axioma em `src/Logs/Logs_unitcore.rs`;
- a seção `hierarchy.super_block` de `get_toml_for_new/default_hierarchy.toml`
  e o seu catálogo em `get_toml_for_new/default_rules.toml`;
- testes e snapshots que existem exclusivamente para o build guard antigo.

O `build.rs` do próprio AntDelirium já não executa esse guard. A retirada do
axioma não elimina o enforcement novo: wrapper global, serviço dedicado,
capabilities, backup Git privado, checkpoint assinado e transição local são
contratos separados.

## Componentes que devem permanecer

### Projeções públicas dentro do projeto

A pasta `guard/` não é o binário antigo. Ela é a superfície pública e
encontrável do projeto e deve continuar contendo:

- `guard/to-do_list.json`;
- `guard/governance_error.json`.

Nenhum passo da T10.2 pode renomear, excluir ou ocultar esses dois arquivos. A
pasta não recebe uma terceira entrada gerada pelo AntDelirium.

### Migração, armazenamento e rollback atuais

Devem permanecer os módulos `Legacy-Staging`, `Legacy-Normalization`,
`Legacy-Dry-Run`, `Legacy-Conversion`, `Legacy-Attestation`,
`Legacy-Approval` e `Legacy-Cutover`. Eles implementam a importação
transacional e auditada de uma origem antiga; não são pass-through da CLI.

Também permanecem:

- o repositório Git privado por projeto, a cópia versionada de fonte e o
  histórico usado para descrições/contextos de tarefas;
- `gix` enquanto implementar esse repositório interno;
- rollback do instalador e da configuração Cargo;
- backup e rollback do serviço e do storage;
- detecção de rollback pelo checkpoint assinado;
- tipos de compatibilidade de estado ainda consumidos por migração;
- `src/engine_identity.rs` enquanto houver consumidores atuais fora do runtime
  antigo.

## Ordem sem conflito

1. portar e provar o backup Git automático na T9.8, inclusive em projeto sem
   `.git`;
2. usar o commit/fingerprint desse backup no enforcement local T9.7;
3. preservar o commit acima como tag/release recuperável;
4. remover as duas rotas internas e somente as funções Git legadas já
   substituídas;
5. remover o Super Block e os templates do guard local no mesmo commit;
6. atualizar apenas snapshots e testes que cobriam esses componentes;
7. provar que `cargo metadata` expõe somente `antdelirium` e que o ELF não
   contém os entrypoints ou instruções antigas;
8. executar a matriz T10.4 em fixtures Rust internas;
9. revisar mensagens e documentação restantes para concluir T10.3.

## Critérios de falha

A retirada deve ser interrompida se qualquer validação:

- remover ou deixar de atualizar `guard/to-do_list.json` ou
  `guard/governance_error.json`;
- eliminar uma operação atual de migração, instalação ou rollback;
- eliminar o backup Git privado, a cópia histórica de fonte ou o contexto de
  tarefa derivado de commits;
- reintroduzir caminho de `plugins/`, TOML externo ou instrução para editar
  `build.rs` na superfície do agente;
- criar um segundo executável;
- depender de AutoBrowser ou de outro projeto externo.
