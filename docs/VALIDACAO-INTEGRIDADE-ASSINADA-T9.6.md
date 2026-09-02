# Validação da integridade assinada — T9.6

## Objetivo

Provar que os journals e snapshots necessários possuem uma âncora monotônica
assinada por uma autoridade que o agente e o serviço não controlam. Restaurar o
estado SQLite não pode restaurar junto o epoch assinado, reescrever o prefixo do
journal, introduzir uma lacuna ou substituir silenciosamente um snapshot.

## Contrato implementado

O contrato [`signed-integrity-checkpoint-v1`](./contracts/v1/signed-integrity-checkpoint-v1.schema.json)
publica atomicamente um envelope canônico contendo:

- epoch monotônico e digest do checkpoint anterior;
- fingerprint da chave pública Ed25519;
- inventário identificado somente por IDs lógicos;
- hash semântico de cada banco protegido;
- revisão canônica e última sequência de cada projeto;
- cadeia SHA-256 de cada journal de projeto;
- cadeia única do journal de autoridade, formada pela intercalação ordenada de
  eventos de segurança e gestão de capabilities;
- revisão e SHA-256 do payload de cada snapshot de governança.

A autoridade em
[`checkpoint-authority.py`](../packaging/integrity/checkpoint-authority.py) não
possui path físico embutido. O bundle
[`integrity-authority-bundle-v1`](./contracts/v1/integrity-authority-bundle-v1.schema.json)
contém somente essa rotina e seu hash revisado, nunca a chave. A instalação
controlada torna a cópia `root:root`; chave privada, chave pública e envelope
ficam numa raiz externa `0700` pertencente a root, separada do state root que o
serviço pode gravar. O usuário do serviço e o operador não-root precisam falhar
ao ler a chave ou gravar a âncora.

Isso preserva um único ELF de produto chamado `antdelirium`. A autoridade é uma
rotina operacional externa, não um segundo binário, comando público do agente
ou caminho para planos.

## Gate sintético

O comando reproduzível é:

```bash
make signed-integrity-checkpoint
```

Ele usa exclusivamente dois bancos sintéticos sob `target/` e uma chave efêmera
fora do state root da fixture. A execução `20260901T172130Z-15` passou com epoch
2 e comprovou:

1. primeiro selo e extensão legítima do journal/snapshot;
2. aceitação da intercalação válida entre eventos de segurança e gestão;
3. rejeição de lacuna em journal de projeto;
4. rejeição de lacuna no journal intercalado da autoridade;
5. rejeição de reescrita do prefixo já assinado;
6. rejeição de rollback para a revisão e sequência anteriores;
7. rejeição de payload de snapshot incompatível com seu fingerprint canônico;
8. rejeição de envelope com assinatura Ed25519 alterada.

O fingerprint de snapshot foi conferido contra o algoritmo Rust existente:
versão do contrato, projeto e workspace em UTF-8 com comprimento little-endian,
revisão `i64`, comprimento do payload, payload e SHA-256.

## Primeira execução instalada e correção

A execução instalada `20260901T171923Z` falhou fechada antes de criar qualquer
checkpoint. O validador tratava `service_security_events` e
`capability_management_events` como sequências independentes, embora ambas
consumam atomicamente o mesmo `service_security_state.last_sequence`.
Consequentemente, a alternância válida parecia uma lacuna em cada tabela.

O commit `8948a3a` passou a ordenar e encadear a união tipada das duas tabelas
como um único journal de autoridade. O gate sintético ganhou uma regressão com
sequências 1 e 2 distribuídas entre as duas tabelas. Nenhum dado protegido foi
editado para contornar a falha.

## Prova no serviço instalado

O operador executou:

```bash
sudo bash packaging/systemd/checkpoint-installed-integrity.sh --apply
```

Em 1 de setembro de 2026, a execução `20260901T181130Z` terminou com:

```text
contract=installed-integrity-checkpoint-result-v1
exit_code=0
checkpoint_present=yes
service_recovery=not-needed

contract=integrity-checkpoint-v1
epoch=1
database_count=16
validation=pass
```

A rotina validou a cópia root-owned contra o manifesto, recusou leitura da
chave e escrita da âncora pelas identidades não-root, parou serviço e dois
sockets para uma leitura consistente, selou e verificou os 16 bancos e reiniciou
as três units. As verificações `systemctl is-active` passaram antes do retorno.

Os logs persistidos em
`target/t96-installed-integrity-logs/20260901T181130Z/` contêm somente contrato,
epoch, contagens, resultado e o locator local dos próprios logs. Não contêm
state root, nome de banco, projeto registrado, ID protegido, segredo, chave ou
path de plano.

## Limite e próxima tarefa

A T9.6 fornece assinatura, continuidade e rollback detection sob autoridade
root externa ao agente e ao serviço. Ela não ativa bloqueio de builds por conta
própria. Tornar a verificação obrigatória num runner protegido e numa política
da transição canônica local, com conjunto explícito de violações fail-closed,
pertence à T9.7. Publicar a decisão em uma branch protegida é opcional.
