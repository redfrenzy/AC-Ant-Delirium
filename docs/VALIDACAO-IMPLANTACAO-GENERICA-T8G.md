# Validação da implantação genérica — T8G

Data da prova real: 26 de agosto de 2026.

## Escopo

Esta validação mede somente o produto AntDelirium. Nenhum projeto externo é
lido, copiado, migrado ou modificado. O projeto usado no teste é uma cópia
descartável da fixture Rust mantida em
`tests/fixtures/t76-standard-cargo`, criada dentro de `target/`.

O gate reproduzível é:

```bash
make installed-generic-acceptance
```

Ele é executado pelo operador sem `sudo`. O serviço systemd e a integração
global precisam estar previamente instalados e ativos.

## Contratos comprovados

O script
[`validate-installed-generic-project.sh`](../packaging/systemd/validate-installed-generic-project.sh)
falha se qualquer um dos contratos abaixo não for verdadeiro:

1. release revisado, wrapper global do Cargo, binário do serviço e binário do
   bundle têm o mesmo SHA-256;
2. o manifesto do bundle atesta esse mesmo SHA-256;
3. `antdelirium doctor` retorna `result=ready`;
4. serviço, socket limitado e socket administrativo estão ativos;
5. `cargo check`, `cargo build` e `cargo test` passam com a sintaxe normal, cada
   um em target novo;
6. a integração materializa exatamente `guard/to-do_list.json` e
   `guard/governance_error.json`, ambos regulares, sem symlink e com modo `0600`;
7. os dois documentos obedecem a `agent-projection-v1`, usam IDs opacos
   consistentes e não contêm localizadores físicos;
8. nenhum estado local obsoleto (`login.json`, `AC_Ant-Delirium_props` ou
   `plugins/`) é criado.

## Resultado observado

A execução real terminou com exit zero e registrou:

```text
wrapper_matches_release=yes
service_matches_release=yes
bundle_matches_release=yes
doctor=ready
cargo_check=pass
cargo_build=pass
cargo_test=pass
public_projection_count=2
public_projection_contract=pass
physical_locator_disclosure=none
```

O teste Rust da fixture também foi realmente executado: um teste aprovado,
zero falhas. A evidência operacional detalhada fica sob
`target/t8g-generic-installation-logs/`; o resumo público registra somente
hashes e estados fechados.

## Limite desta prova

Esta prova conclui a uniformidade da instalação e o acionamento automático por
comandos Cargo normais. O ciclo com tarefa sintética, submissão, revisão,
avanço e histórico pertence à T8G.3. Atualização, desinstalação e rollback
genéricos pertencem à T8G.4.
