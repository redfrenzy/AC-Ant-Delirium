# Validação de não divulgação por syscalls — T6.8

## Objetivo

Esta validação prova, no caminho executado sob a autoridade do agente, que
`cargo`, o wrapper `antdelirium`, seus filhos audit-only e o `rustc` real não
abrem, testam nem enumeram raízes conhecidas do estado protegido.

Ela complementa a inspeção estática do release. Remover paths de mensagens ou
do ELF não seria suficiente se um processo cliente ainda entregasse o path ao
kernel em `openat`, `statx`, `readlink` ou outra syscall de arquivo.

## Gate reproduzível

O script
[`validate-wrapper-syscalls.sh`](../packaging/sandbox/validate-wrapper-syscalls.sh)
copia o workspace Cargo bloqueado da T6.6 para uma área isolada e executa:

```text
strace -f -qq -s 4096 -e trace=%file,connect \
  env RUSTC_WORKSPACE_WRAPPER=<release-antdelirium> \
      ANTDELIRIUM_LIMITED_SOCKET=/run/antdelirium/agent.sock \
      CARGO_TARGET_DIR=<target-isolado> \
      cargo check --manifest-path <fixture>/Cargo.toml \
        --quiet --locked --offline --workspace --all-targets \
        --target x86_64-unknown-linux-gnu -j 4
```

O `--manifest-path` é obrigatório. Sem ele, Cargo usaria o diretório atual do
repositório principal e a prova observaria o projeto errado.

Depois do comando, o gate exige evidência de execução do wrapper e de tentativa
de conexão ao endpoint limitado. Em seguida rejeita qualquer ocorrência de:

- raiz absoluta e traversal relativo da fixture protegida;
- layout ou payload sentinela `t48-protected-container/canonical` e
  `secret-state.db`;
- raiz de serviço `/var/lib/antdelirium`;
- raiz local `$CARGO_HOME/antdelirium`.

O alvo público é:

```bash
make syscall-client-nondisclosure
```

O comando não usa `sudo`. Ele precisa de `strace` e de permissão de `ptrace` no
ambiente que o executa; quando isso não estiver disponível, termina com `77` em
vez de declarar um falso sucesso. Por essa razão ele é separado do `make ci`
portável, embora a sintaxe do script seja verificada pelo CI.

## Resultado observado

Em 20 de agosto de 2026, o gate passou com:

- 26.921 registros de syscall;
- 117 PIDs/TIDs acompanhados por `strace -f`;
- 17 execuções do wrapper;
- 12 tentativas de conexão ao canal limitado, indisponível no ensaio;
- zero ocorrência de cada raiz, layout ou payload protegido proibido.

O serviço ausente exercita deliberadamente o fallback audit-only: as tentativas
de conexão falham sem bloquear ou mudar o resultado de Cargo e sem levar o
cliente a procurar o estado por filesystem.

## Limite da prova

Este gate prova a não divulgação do lado cliente para os sentinelas e roots
conhecidos. Ele não prova, sozinho, que um serviço real instalado sob outra
autoridade resiste a attach, leitura de `/proc` ou bypass de sandbox. Essas
provas continuam na matriz instalada das ondas posteriores.
