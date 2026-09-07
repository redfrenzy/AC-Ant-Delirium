# Remoção do build hook de plugin — evidência da T10.3

Data da prova: 1 de setembro de 2026.

## Problema confirmado

O `build.rs` do próprio AntDelirium ainda executava uma integração anterior à
arquitetura global. A cada build ele podia:

- aceitar `ANT_DELIRIUM_BIN`, `ANT_DELIRIUM_CONFIG` e
  `ANT_DELIRIUM_PROJECT_ROOT`;
- procurar `../plugins/ac_ant_delirium`;
- executar esse binário com `--project-root` e `--config`;
- imprimir no Cargo o path procurado e instruções da API antiga.

Esse comportamento estava ativo, não era apenas rollback compilado. Ele
produzia o aviso `governance binary not found` observado nas compilações e
reintroduzia a topologia de plugin que a integração global substituiu.

## Correção

O hook de execução foi removido integralmente. O `build.rs` agora possui apenas
duas responsabilidades:

1. compilar a taxonomia estática usada pela crate;
2. incorporar hash Git e timestamp reproduzível usados pela versão pública.

As cinco variáveis do hook, a procura em `plugins/`, os seletores físicos e as
mensagens de instalação antiga deixaram de participar do build. `make check`
também passou a executar o `cargo check` comum, em vez de chamar uma query
legada do binário.

## Gate reproduzível

[`validate-obsolete-build-hook-removal.sh`](../packaging/ci/validate-obsolete-build-hook-removal.sh)
faz duas verificações:

- recusa por fonte qualquer retorno dos nomes, paths, seletores e função do
  hook antigo;
- executa `cargo check` com todos os seletores obsoletos preenchidos por paths
  hostis e exige que nenhum deles apareça ou afete a compilação.

A execução `20260901T225443Z-16` registrou:

```text
legacy_plugin_execution=absent
legacy_path_selectors=ignored-by-absence
taxonomy_compiler=retained
version_metadata=retained
ordinary_cargo_check=passed
external_project_access=forbidden
validation=pass
```

## Fechamento

Esta prova foi a primeira parte da T10.3. A T10.2 removeu estruturalmente o
runtime, as mensagens e os templates da instalação antiga. A validação final
confirmou a API pública fechada, o ELF sem localizadores protegidos e a pasta
`guard/` contendo somente seus dois JSONs públicos.
