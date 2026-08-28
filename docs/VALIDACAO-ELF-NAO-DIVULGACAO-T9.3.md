# Auditoria ELF e não divulgação — T9.3

Data da prova: 28 de agosto de 2026.

## Escopo

Esta prova examina somente o release nativo `antdelirium` produzido por este
repositório. Ela não lê projetos externos, planos concretos ou estado protegido.

O gate reproduzível é:

```bash
make release-nondisclosure
```

O alvo recompila o binário único com paths físicos remapeados, remove metadados
ELF não necessários em runtime e executa
[`validate-elf-nondisclosure.sh`](../packaging/release/validate-elf-nondisclosure.sh).
A evidência pública segue o contrato fechado `release-elf-audit-v1`.

## Problemas encontrados e correções

Antes da T9.3, `file` classificava o release como `not stripped`. O ELF mantinha
`.symtab` e `.strtab`, com milhares de símbolos de funções e objetos. O gate
anterior verificava apenas algumas expressões em `strings`, sem validar seções,
RPATH, stack, PIE ou RELRO.

O perfil `[profile.release]` agora usa `strip = "symbols"`. Isso removeu as
tabelas estáticas de símbolos e reduziu o ELF em aproximadamente 0,97 MB sem
alterar seu código executável ou APIs.

O strip ainda preservava `.comment`, que continha o path físico
`/checkout/src/llvm-project` proveniente do build do linker. Embora esse path
não localizasse nossos planos, era metadado de build evitável. `make binary`
agora remove `.comment` com `objcopy` antes de publicar o artefato revisado.

Os remapeamentos do compilador continuam substituindo:

- a raiz física deste checkout por um prefixo lógico;
- o sysroot da toolchain por um prefixo lógico;
- o Cargo home por um prefixo lógico.

Esses prefixes podem aparecer em mensagens internas de panic, mas não revelam
usuário, home, checkout ou root protegido reais.

## Contratos ELF comprovados

A execução `20260828T185015Z-24` terminou com exit zero e registrou:

```text
elf_class=ELF64
elf_machine=Advanced Micro Devices X86-64
elf_type=DYN
stripped=true
static_symbol_table=false
debug_sections=false
comment_section=false
rpath=false
runpath=false
pie=true
relro=full
executable_stack=false
physical_locator_match_count=0
predictable_log_match_count=0
```

O release final tem 5.262.704 bytes e SHA-256
`070a570b9507469dcda9904a7993b44b394a2879a2cac443fd6bc3e3cd43ec00`.
O ELF mantém somente símbolos dinâmicos exigidos pelo loader, usa PIE,
`GNU_RELRO`, `BIND_NOW`, stack `RW` sem execução e não contém `RPATH` ou
`RUNPATH`.

A busca em todos os bytes do ELF rejeita:

- homes Linux, macOS e Windows;
- paths de root, checkout, build, workspace e deste repositório;
- state root e sockets protegidos do serviço;
- logs globais previsíveis antigos.

## Limite honesto da inspeção por `strings`

Um binário local não é secreto e engenharia reversa continua possível. A prova
encontrou dois nomes administrativos genéricos sem diretório:
`default_hierarchy.toml` e `task_orchestrator.json`. Eles permanecem porque as
rotas de compatibilidade/conversão administrativa ainda estão compiladas no
binário único. O relatório registra `administrative_marker_count=2` e os
classifica como nomes sem localizador físico, não como segredo protegido.

Ocultar esses nomes por montagem dinâmica ou ofuscação não criaria uma
fronteira de segurança. Sua remoção correta depende da retirada das rotas
legadas em T10.2 e da revisão de mensagens/documentação em T10.3. Até lá, o
serviço, sandbox e CI devem garantir que conhecer um nome não conceda localização
nem acesso.

## Resultado

T9.3 está concluída: símbolos e metadados físicos evitáveis foram removidos, o
release possui hardening ELF verificável e nenhum path de mantenedor, build,
storage, socket ou log protegido aparece em `strings`. Os dois nomes genéricos
restantes estão inventariados, sem falsa promessa de sigilo, para remoção junto
com o legado.
