# Provisionamento inicial e teste privilegiado — T8.6 parcial

## Resultado implementado

O serviço agora resolve o impasse da primeira autoridade sem confiar no primeiro
cliente que alcançar um socket. Antes de servir conexões, ele procura a
credencial lógica fixa `antdelirium-bootstrap-v1` no diretório privado entregue
pelo `systemd` em `CREDENTIALS_DIRECTORY`. O valor dessa variável não é segredo;
o conteúdo permanece em uma cópia read-only, privada para a conta da unidade e,
quando possível, em memória não paginável.

O contrato fechado
[`initial-capability-provisioning-v1`](./contracts/v1/initial-capability-provisioning-v1.schema.json)
tem janela máxima de 15 minutos e de 2 a 16 grants. O bootstrap exige, no
mínimo, dois grants globais distintos para um operador não-root:
`state.admin` e `installation.admin`, ambos no canal administrativo. Grants
iniciais adicionais podem pertencer somente a operador, CI ou revisor e ainda
passam pela matriz normal de ator, canal, operação e escopo.

[`Service-Provisioning_unit`](../src/Service-Provisioning_unit.rs) aplica as
seguintes garantias:

- diretório absoluto aberto com `O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC` e arquivo
  relativo aberto por `openat` com `O_NOFOLLOW|O_CLOEXEC`;
- validação da forma real de systemd credentials: ownership direto pela conta do
  serviço ou ACL POSIX exata com uma única entrada nomeada para seu UID, sem
  escrita, outros usuários, grupos nomeados ou acesso de `other`;
- arquivo regular com limite de 64 KiB e JSON sem campos desconhecidos;
- envelope válido somente no horário do serviço, grants por no máximo 366 dias
  e tokens base64url canônicos de 32 bytes;
- limpeza em memória dos bytes do documento e das strings de token depois do
  parse;
- persistência somente do SHA-256 de cada token;
- recusa de UID root, IDs ou tokens repetidos e estado canônico previamente
  populado;
- transação única para epochs, grants e recibo de uso único.

O storage passou a `storage-v8` com a tabela singleton
`initial_capability_provisioning`. Com recibo existente e sem nova credencial, o
serviço reinicia normalmente. Com recibo e credencial reapresentada, ele falha
fechado. Sem recibo e sem credencial, o primeiro boot também falha fechado.

Os testes nativos focados comprovaram aplicação única, armazenamento hash-only,
rollback sem grant parcial, recusa de registry já preenchido, expiração, limite
temporal, parsing da ACL Linux, aceitação da ACL exata de diretório/arquivo e
recusa de entrada adicional ou escrita. Cada erro interno possui um código
`AD-PROVISION-*` sem token, UID, path ou conteúdo; a saída pública permanece
`AD-CONFIG-0001`.

## Primeira execução privilegiada e falha preservada

A primeira execução real de `--apply`, em 24 de agosto de 2026, criou as três
identidades dedicadas, instalou o ELF/unidades e ativou os dois sockets. O
serviço terminou com `78/CONFIG`. O trap removeu o bootstrap transitório e os
segredos em claro, mas preservou os descritores cifrados. O registry resultante
ficou em schema 8 com zero recibos e zero grants; portanto, não houve
provisionamento parcial.

Os logs estão em
`target/t86-root-validation-logs/20260824T205727Z/`. Eles também provaram uma
corrida no validador: o teste imediato alegou que o banco não existia, mas a
coleta posterior encontrou o schema já criado. A causa compatível com o exit do
serviço era a antiga checagem `mode & 077 == 0`: o systemd representa a máscara
da ACL nomeada nos bits de grupo de `stat`, ainda que somente o UID do serviço
tenha acesso. Essa checagem foi substituída pela validação semântica da ACL.

## Rotina privilegiada corrigida e retomável

O script auditável
[`validate-installed-service.sh`](../packaging/systemd/validate-installed-service.sh)
instala somente o bundle já compilado e revisado; ele não compila como root. A
rotina recusa sobrescrever qualquer artefato ou state root existente, cria as
identidades via `sysusers`, associa o usuário que chamou `sudo` aos dois grupos,
gera duas capabilities com OpenSSL, sela os descritores com
`systemd-creds --with-key=host` e entrega o manifesto ao primeiro boot por um
drop-in transitório em `/run`.

Depois do primeiro start, o script exige schema 8, um recibo e dois grants,
procura os tokens brutos no SQLite, remove o canal de bootstrap e prova um
segundo start sem credencial. Os descritores do operador permanecem somente em
forma cifrada e root-controlled; tokens em claro existem apenas sob um diretório
privado de `/run` durante a execução e são removidos pelo trap inclusive em
falha. Nenhum segredo é enviado aos logs.

O readiness agora espera por até dez segundos pelo recibo transacional e encerra
antes se o serviço entrar em `failed`; um segundo gate exige que o restart sem
credencial permaneça ativo. O modo `--resume` não aceita uma instalação
arbitrária: exige todos os artefatos regulares, contratos de unidade idênticos ao
bundle revisado, descritores com ownership/mode exatos, operador único nos dois
grupos e registry estritamente virgem (`0 0 0 8`). Só então instala o novo ELF,
decifra temporariamente os dois descritores já existentes, recria um envelope
curto com os mesmos IDs/tokens e repete o primeiro boot. Assim, a autoridade
preservada não é substituída.

Todos os diagnósticos ficam em:

```text
target/t86-root-validation-logs/<UTC>/
```

Eles incluem exit code, versão do systemd, `status`, propriedades e definição
das unidades, journal do boot, identidades, modos de arquivos/sockets e resumo
read-only do registry. Os arquivos recebem permissão de leitura ao final para
que possam ser analisados sem novo `sudo`.

Antes da execução privilegiada, gerar e validar o bundle como usuário comum:

```bash
make system-installation-bundle
```

Em uma máquina sem instalação anterior, o comando que exige confirmação humana
é:

```bash
sudo bash packaging/systemd/validate-installed-service.sh --apply
```

Na instalação parcial preservada pela primeira tentativa, usar exclusivamente:

```bash
sudo bash packaging/systemd/validate-installed-service.sh --resume
```

Para somente recolher novos diagnósticos de uma instalação já existente, sem
alterar unidade, grupo, credential ou state root:

```bash
sudo bash packaging/systemd/validate-installed-service.sh --inspect
```

Esta etapa continua parcial até `--resume` terminar com exit zero e os novos
logs serem revisados. A associação a grupos também só aparece em processos
abertos depois de novo login; a própria rotina usa `runuser` para testar uma
sessão nova.
