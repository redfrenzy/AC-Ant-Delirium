# Inventário de remoção do rollback antigo — T10.2

## Objetivo

Este documento fixa a fronteira da T10.2 antes de qualquer exclusão. O nome
`legacy` também é usado por recursos atuais de migração; portanto a remoção não
pode ser feita por busca textual, prefixo de arquivo ou diretório inteiro.

O ponto de recuperação revisado antes da retirada é o commit
`de355fe78ca958988b8c00ebf14d442e182df1f2`. A exclusão estrutural só pode ser
executada depois de G9/T9.7 estar comprovado por um branch protegido real. Até
lá, este inventário não autoriza apagar nem desativar o rollback compilado.

## Remoção autorizada depois de G9

### Entradas compiladas do runtime antigo

- `src/Agent-Runtime_unit.rs` e `src/Agent-Runtime_unitcore.rs`;
- `src/Main-Logic_unit.rs` e `src/Main-Logic_unitcore.rs`;
- `src/guard_main.rs`;
- `get_toml_for_new/ant_delirium_build_guard.rs`;
- os includes e módulos `agent_runtime`, `legacy_guard_mode` e
  `run_legacy_cli_mode` em `src/main.rs`;
- as variantes internas `LegacyAgentCliRollback` e `LegacyGuardRollback`, seus
  match arms e seus testes de compilação em `src/Binary-Mode_unit.rs`.

Esses componentes não possuem rota pública hoje. Eles existem somente para
recuperar o comportamento anterior dentro do ELF único.

### Contrato Super Block ligado ao guard local

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
capabilities, checkpoint assinado e gate de branch são contratos separados.

## Componentes que devem permanecer

### Projeções públicas dentro do projeto

A pasta `guard/` não é o binário antigo. Ela é a superfície pública e
encontrável do projeto e deve continuar contendo:

- `guard/to-do_list.json`;
- `guard/governance_error.json`;
- `guard/.gitignore`.

Nenhum passo da T10.2 pode renomear, excluir ou ocultar esses arquivos.

### Migração, armazenamento e rollback atuais

Devem permanecer os módulos `Legacy-Staging`, `Legacy-Normalization`,
`Legacy-Dry-Run`, `Legacy-Conversion`, `Legacy-Attestation`,
`Legacy-Approval` e `Legacy-Cutover`. Eles implementam a importação
transacional e auditada de uma origem antiga; não são pass-through da CLI.

Também permanecem:

- rollback do instalador e da configuração Cargo;
- backup e rollback do serviço e do storage;
- detecção de rollback pelo checkpoint assinado;
- tipos de compatibilidade de estado ainda consumidos por migração;
- `src/engine_identity.rs` enquanto houver consumidores atuais fora do runtime
  antigo.

## Ordem sem conflito

1. comprovar T9.7 em remoto real e fechar G9;
2. preservar o commit acima como tag/release recuperável;
3. remover as duas rotas internas e seus arquivos de runtime;
4. remover o Super Block e os templates do guard local no mesmo commit;
5. atualizar apenas snapshots e testes que cobriam esses componentes;
6. provar que `cargo metadata` expõe somente `antdelirium` e que o ELF não
   contém os entrypoints ou instruções antigas;
7. executar a matriz T10.4 em fixtures Rust internas;
8. revisar mensagens e documentação restantes para concluir T10.3.

## Critérios de falha

A retirada deve ser interrompida se qualquer validação:

- remover ou deixar de atualizar `guard/to-do_list.json` ou
  `guard/governance_error.json`;
- eliminar uma operação atual de migração, instalação ou rollback;
- reintroduzir caminho de `plugins/`, TOML externo ou instrução para editar
  `build.rs` na superfície do agente;
- criar um segundo executável;
- depender de AutoBrowser ou de outro projeto externo.
