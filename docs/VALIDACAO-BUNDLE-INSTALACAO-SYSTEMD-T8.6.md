# Validação do bundle de instalação systemd — T8.6 parcial

Data: 2026-08-24

## Objetivo

Transformar os templates já existentes em um bundle reproduzível de implantação
sem usar privilégios, escrever fora do repositório, criar contas, copiar arquivos
para `/usr` ou ativar unidades no host.

O resultado foi gerado por:

```bash
make system-installation-bundle
```

O alvo usa exclusivamente `target/t86-system-installation-bundle/` para o bundle
e `target/t86-systemd-unit-validation/` para as cópias examinadas pelo systemd.

## Artefatos staged

O contrato `system-installation-bundle-v1` contém exatamente cinco artefatos:

1. o mesmo ELF release `antdelirium`, destinado ao diretório libexec root-owned;
2. a unidade de serviço de sistema sob a identidade `antdelirium`;
3. o socket limitado, grupo `antdelirium-client`, modo `0660`;
4. o socket administrativo separado, grupo `antdelirium-admin`, modo `0660`;
5. a declaração `sysusers.d` para a conta de serviço e os dois grupos.

O SHA-256 do ELF staged foi comparado ao release e coincidiu. Não há segundo
programa `guard`, daemon auxiliar ou implementação administrativa separada.

O manifesto registra destinos, modos, ownership esperado e hashes, e declara
explicitamente `activation_performed=false` e
`global_installation_performed=false`. Paths de projeto, plugin, plano e storage
protegido não entram nele.

## Validação local

- host de validação: systemd 255;
- `bash -n packaging/systemd/render-installation-bundle.sh`: aprovado;
- schema JSON `system-installation-bundle-v1`: JSON válido;
- placeholders de template residuais: zero;
- `systemd-analyze verify` nas três unidades renderizadas: aprovado;
- `systemd-sysusers --dry-run` na declaração de identidades: exit `0`;
- manifesto: cinco hashes SHA-256 minúsculos e flags de instalação falsas;
- release e cópia libexec staged: bytes/hash idênticos.

As cópias usadas por `systemd-analyze` trocam apenas `ExecStart` pelo caminho do
ELF staged para que o verificador possa abrir o executável sem uma instalação
real. Os arquivos do bundle mantêm o destino final root-owned.

## Correção de lifecycle

`RuntimeDirectory=antdelirium` foi removido da unidade de serviço. O diretório do
socket limitado pertence ao lifecycle da unidade `.socket`, que já o cria com
`DirectoryMode=0755`; fazê-lo pertencer também ao serviço permitiria que uma
parada do daemon removesse o diretório enquanto o socket continuasse ativo. O
state root continua sob `StateDirectory=antdelirium` com modo `0700`.

## Limites e próximo gate

O bundle não escolhe quais usuários humanos ou sandboxes entram nos grupos. Essa
associação é uma decisão de implantação: agentes autorizados usam somente
`antdelirium-client`; operadores autorizados usam somente `antdelirium-admin`.
Também não executa `systemd-sysusers`, `systemctl daemon-reload`, `enable` ou
`start` fora de dry-run.

A T8.6 continua aberta até que um operador revise o bundle, implante as
identidades/unidades como root, adicione as identidades corretas aos grupos,
instale a integração Cargo real e valide a mudança operacional separadamente.

