# Baseline adversarial da Onda 0

Esta captura registra a exposição observável antes da migração para o binário único, o state root protegido e a superfície limitada do agente. Ela é evidência da T0.7 e não afirma que os problemas abaixo já foram corrigidos.

## Artefatos analisados

Build nativo de desenvolvimento, Rust 1.93.1, target `x86_64-unknown-linux-gnu`:

| Binário | Tamanho | SHA-256 |
|---|---:|---|
| `delirium_cli` | 98.577.104 bytes | `a36cd591b0251ea96a6b4a2218c282ff0bb438d0a5b6baa1752f010519d18781` |
| `guard` | 5.229.648 bytes | `b56d04f2a7b759e677066b6b3e2f7ac02972b6cf732ae9ca72b842f00cc962d1` |

Ambos são ELF64 PIE para x86-64, dinamicamente ligados, com `GNU_RELRO`, `BIND_NOW`, pilha não executável e símbolos de debug não removidos.

Build IDs:

- `delirium_cli`: `f4e8c479e4f79809e3029983daf95bb6a5677ec3`;
- `guard`: `fcb9612a355400aa90180ab095929ac2ebfb68da`.

## Inspeção com `strings`

Contagens de ocorrências no `delirium_cli` debug:

| Marcador | Ocorrências |
|---|---:|
| prefixo absoluto do usuário `/home/dracker` | 10.243 |
| nomes previsíveis `delirium_debug.log` ou `delirium_error.log` sob `/tmp` | 2 |
| topologias explícitas `plugins/AutoBrowser`, placeholder de projeto ou exemplos `plugins/<...>` | 33 |
| `default_hierarchy.toml` ou `default_rules.toml` | 26 |
| nomes dos principais JSONs administrativos | 39 |

O `guard` debug contém 438 ocorrências do prefixo absoluto do usuário e ao menos uma referência explícita à topologia/configuração do plugin. Parte substancial dos caminhos absolutos vem de DWARF e fontes de dependências, mas isso não elimina o vazamento: um agente com acesso ao binário consegue recuperar tanto caminhos de compilação quanto literais operacionais do AntDelirium.

## Superfície impressa por `--help`

A ajuda atual:

- imprime o caminho absoluto do próprio executável em 73 linhas;
- contém 39 linhas que revelam ou descrevem comandos administrativos de manager tasks, submits, root checks, poda de agentes e hooks;
- documenta diretamente a topologia `plugins/<project>`, o TOML de hierarquia e o diretório de login administrativo.

Isso confirma que não basta mover os planos: a superfície textual atual orienta o agente até os componentes internos e precisa ser dividida por capability.

## Rastreamento de arquivos

Foram executados dois rastreamentos `strace -f -e trace=%file` sobre uma fixture criada em `tmp/`:

| Operação | Eventos de arquivo | Caminhos absolutos únicos | Opens de escrita |
|---|---:|---:|---:|
| `--help` | 228 | 50 | 0 |
| `config init` com `--project-root` apontando para a fixture | 374 | 86 | 23 |

O `--help` sobe a árvore de diretórios procurando configuração de hierarquia, mesmo sendo uma operação informativa. O `config init` não respeitou a fixture aninhada como raiz efetiva: descobriu a raiz Git real, leu os TOMLs do projeto e tentou criar o pacote em `plugins/AC-Ant-Delirium`, além do build guard em `AC_Ant-Delirium_props`.

O bootstrap realizou 34 chamadas `mkdir` e abriu 23 arquivos para escrita, incluindo dois binários, dois TOMLs, JSONs administrativos, plano de exemplo, transition task, scripts e contexto. Os artefatos comprovadamente criados por este ensaio foram imediatamente movidos para `tmp/t0.7-generated/`; nenhum arquivo preexistente foi removido.

Esse resultado deve virar um teste de segurança posterior: um caminho explícito não pode ser silenciosamente substituído por outra raiz para uma operação mutável.

## Logs globais previsíveis

O código atual ainda grava:

- `/tmp/delirium_debug.log` com o caminho do output root;
- `/tmp/delirium_error.log` quando a escrita de artefatos falha.

Na captura existia um `delirium_debug.log` global de 105 bytes contendo um caminho absoluto para os artefatos de outro projeto. O conteúdo exato não é reproduzido neste documento para não transformar a evidência em mais uma fonte de descoberta.

Os dois rastreamentos acima não acionaram essas gravações, mas a presença do arquivo e os pontos de escrita no código confirmam a exposição. A remoção desses logs previsíveis permanece tarefa da fase de hardening.

## Conclusão da baseline

A versão atual falha nos objetivos futuros de discrição por quatro canais independentes:

1. símbolos/debug strings do executável;
2. ajuda pública excessivamente administrativa;
3. descoberta e bootstrap que materializam caminhos internos;
4. logs globais previsíveis contendo caminhos absolutos.

A T0.8 deve produzir o release-base e registrar quanto da exposição permanece em um binário de release. As fases seguintes devem então reduzir esses números sem perder o comportamento congelado pelos snapshots da T0.4.
