# Validação hostil de cliente e serviço — T9.4

## Objetivo

A T9.4 fecha conjuntamente as provas que antes estavam separadas entre cliente,
wrapper e serviço instalado. O gate reproduzível testa `open`, `stat`, `access`,
`readlink`, enumeração, `/proc`, `strace`/`ptrace` e herança de descritores sem
ler nem alterar projeto externo.

## Comando único

```bash
make hostile-isolation-matrix
```

O comando deve ser executado pelo operador comum, **sem `sudo`**. Ele requer o
serviço instalado ativo, Bubblewrap, `strace`, Cargo e acesso ao barramento
systemd para consultas somente-leitura.

O gate encadeia quatro provas:

1. o cliente descartável da T4.8, com UID sem privilégio e namespaces próprios,
   não consegue executar `stat`, `access`, `readlink`, enumeração ou abertura
   nem mesmo ao receber previamente os roots sentinela;
2. o `strace -f` da T6.8 acompanha Cargo, wrapper, filhos e compilador e rejeita
   toda syscall de arquivo que mencione estado protegido conhecido;
3. um subprocesso remove deliberadamente `FD_CLOEXEC`, aplica o hardening do
   serviço e comprova após um novo `exec` que o descritor não foi herdado;
4. o operador comum ataca o serviço instalado e precisa receber negação ao ler
   `environ`, `maps`, `mem`, enumerar ou resolver descritores, resolver `cwd`,
   `root` e executável, anexar `strace`/`ptrace`, atravessar, enumerar ou abrir o
   armazenamento canônico.

O serviço precisa continuar ativo com o mesmo PID durante o ensaio e executar
sob UID distinto. O gate também verifica no unit instalado `NoNewPrivileges`,
core dump zero, capability sets vazios, `ProtectProc`, `ProcSubset`, filesystem
read-only, home protegido e namespaces privados de tmp, devices e rede.

## Metadado visível não é conteúdo acessível

No host, um operador que já conhece o nome `/var/lib/antdelirium` pode conseguir
executar `stat` sobre a própria raiz `0700`; da mesma forma, o kernel pode deixar
uma parte não sensível de `/proc/<pid>/status` visível. Isso não permite
travessia, enumeração, leitura do banco, resolução de descritores, ambiente,
mapas, memória ou attach.

O contrato registra honestamente esses dois resultados como
`metadata-visible`/`limited-visible` ou `hidden`. Para o domínio do agente, o
namespace da T4.8 oculta a raiz e o PID do host por inteiro, inclusive contra
`stat` e `access(F_OK)`. Portanto a segurança não depende de fingir que o nome
arquitetural do serviço é secreto.

## Evidência

Cada execução grava somente logs locais em
`target/t94-hostile-isolation/<run-id>/`. O arquivo
`hostile-isolation-evidence-v1.json` segue o contrato fechado
[`hostile-isolation-evidence-v1`](./contracts/v1/hostile-isolation-evidence-v1.schema.json),
sem PID, UID, caminho de projeto externo, payload canônico ou localização de
plano. Ausência de ferramenta, serviço inativo, attach permitido, troca de PID
ou resultado inconclusivo falham o gate; não são convertidos em sucesso.

## Resultado observado

Em 28 de agosto de 2026, a execução
`20260828T201508Z-53158` terminou com exit `0`. O contrato registrou:

- isolamento do namespace do cliente e não divulgação por syscalls: `passed`;
- separação da conta e hardening do unit instalado: `passed`;
- leituras de `environ`, `maps`, `mem` e descritores em `/proc`: `denied`;
- `readlink` de descritor, cwd, root e executável: `denied`;
- leitura, travessia, enumeração e abertura do estado: `denied`;
- attach por `strace` e `ptrace(PTRACE_SEIZE)`: `denied`;
- descritor deliberadamente herdável depois do hardening e novo `exec`:
  `closed`;
- metadados já conhecidos no host: `limited-visible` para processo e
  `metadata-visible` para a raiz, sem acesso ao conteúdo.

O serviço permaneceu ativo sob o mesmo processo durante todo o ensaio. Nenhuma
fonte foi modificada pelo gate e nenhum projeto externo foi lido.
