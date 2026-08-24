# Provisionamento inicial e teste privilegiado — T8.6 parcial

## Resultado implementado sem `sudo`

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

- diretório absoluto, sem symlink, privado e pertencente a root ou à conta do
  serviço;
- arquivo regular aberto com `O_NOFOLLOW|O_CLOEXEC`, limite de 64 KiB e JSON sem
  campos desconhecidos;
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
rollback sem grant parcial, recusa de registry já preenchido, expiração e limite
temporal. Nenhuma conta, unidade ou configuração global foi alterada por esses
testes.

## Rotina privilegiada preparada, ainda não executada

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

O comando que exige confirmação humana é:

```bash
sudo bash packaging/systemd/validate-installed-service.sh --apply
```

Para somente recolher novos diagnósticos de uma instalação já existente, sem
alterar unidade, grupo, credential ou state root:

```bash
sudo bash packaging/systemd/validate-installed-service.sh --inspect
```

Esta etapa continua parcial até o script `--apply` ser realmente executado e os
logs serem revisados. A associação a grupos também só aparece em processos já
abertos depois de novo login; a própria rotina usa `runuser` para testar uma
sessão nova.
