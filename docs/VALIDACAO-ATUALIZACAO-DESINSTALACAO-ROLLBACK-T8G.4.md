# Atualização, desinstalação e rollback globais — T8G.4

Data da prova real: 28 de agosto de 2026.

## Escopo

Esta prova mede somente a instalação global do AntDelirium e seu estado
protegido. Nenhum projeto externo ou exemplo de migração foi lido ou alterado.
O estado exercitado foi produzido pelas fixtures Rust sintéticas mantidas neste
repositório.

Foram validados separadamente:

1. atualização recuperável do serviço a partir do bundle revisado;
2. desinstalação e reinstalação reais da integração Cargo;
3. ativação temporária de um backup anterior do serviço;
4. restauração exata do binário e do estado mais novos.

## Atualização instalada

[`upgrade-installed-service.sh`](../packaging/systemd/upgrade-installed-service.sh)
parou o serviço e os dois sockets, reteve binário e state root anteriores e
instalou sem recompilar como root o ELF já revisado no bundle. A execução
`20260828T115850Z` terminou com `exit_code=0`,
`upgrade_committed=true`, sete bancos em schema 10, `quick_check` válido,
foreign keys válidas e operação administrativa de transição disponível.

O bundle e o serviço receberam o release SHA-256
`295e4406aed195495f3b4e46a587db4e0cda85548927936ffde0d5f857aa21e6`.
O backup recuperável dessa atualização foi mantido para a prova de rollback.

## Desinstalação e reinstalação Cargo

[`validate-installed-uninstall-rollback.sh`](../packaging/cargo/validate-installed-uninstall-rollback.sh)
executa primeiro uma prova descartável com configuração sentinela não
pertencente ao AntDelirium. Depois confere o recibo privado de ownership da
instalação real, remove somente o wrapper e a configuração registrados, testa
os proxies `cargo` e `rustc` sem o wrapper e reinstala o release revisado. Um
trap tenta concluir a desinstalação retomável e reinstalar o release se uma
etapa intermediária falhar.

A execução `20260828T121509Z-12148` terminou com exit zero e comprovou:

- bytes e modo da configuração sentinela restaurados exatamente;
- configuração real anterior restaurada conforme o recibo — neste host ela era
  originalmente ausente;
- symlinks e hashes dos proxies rustup `cargo` e `rustc` preservados;
- wrapper global reinstalado com modo `0755` e o mesmo hash do release;
- integração Cargo novamente reconhecida como `active-owned`;
- `doctor result=ready` com Cargo/rustc 1.97.1 e MSRV 1.88 atendido;
- nenhuma operação solicitada ao estado do serviço.

## Rollback temporário do serviço

[`validate-installed-service-rollback.sh`](../packaging/systemd/validate-installed-service-rollback.sh)
exige um backup explícito e `sudo`. A rotina para as três unidades, move o
state root atual no mesmo filesystem, ativa uma cópia do backup, valida e inicia
o binário anterior e, por fim, restaura por `rename` o estado atual e reinstala
seu binário. Ela não usa remoção destrutiva: a cópia realmente exercitada fica
retida na raiz protegida de validações de rollback para inspeção do operador,
sem publicar seu localizador na evidência.

A primeira execução revelou um erro no próprio gate: o fingerprint era
calculado antes de abrir SQLite para `quick_check`. Em modo WAL, a leitura pode
atualizar arquivos auxiliares `-shm`, fazendo a comparação posterior falhar
mesmo com uma cópia correta. O serviço não chegou a iniciar com o backup. A
rotina de recuperação
[`recover-installed-service-rollback.sh`](../packaging/systemd/recover-installed-service-rollback.sh)
recolocou o snapshot atual, comparou-o ao registro lógico anterior e reativou
as três unidades. A execução de recuperação `20260828T124932Z` confirmou oito
bancos, 14 linhas de projeto, 57 eventos, schema 10, `quick_check=ok`, foreign
keys válidas e o binário atual exato.

A correção calcula o fingerprint somente depois de toda inspeção SQLite e
registra os dois valores comparados. A repetição
`20260828T125533Z` terminou com exit zero e comprovou:

- binário anterior SHA-256
  `ada96ddb8e370909e0ae7c146fb01fdd85fcd8307cb663f1640e24168a28453e`
  iniciado com sucesso;
- sete bancos, 12 linhas de projeto e 49 eventos do backup com schema 10,
  `quick_check=ok` e foreign keys válidas;
- fingerprint selecionado e copiado idêntico:
  `ab77e14d4b286bc0af2426b0cb5969bd4d970bdd6e2dfc3fe4e439acd2e56a07`;
- estado mais novo restaurado com oito bancos, 14 linhas de projeto e 57
  eventos, sem diferença lógica antes/depois;
- fingerprint esperado e restaurado idêntico:
  `dcacb37820390a77e83f047b529d79792b6d4421685befe4b16ee338d25be9c4`;
- binário atual restaurado no mesmo SHA-256 do release e do wrapper Cargo;
- contratos systemd, sysusers e tmpfiles inalterados;
- serviço, socket limitado e socket administrativo ativos ao final.

## Resultado

T8G.4 está concluída. Atualização, desinstalação e rollback foram executados no
host real com recuperação testada, sem apagar estado de projetos, sem alterar
configuração Cargo não pertencente ao AntDelirium e sem usar qualquer projeto
externo como requisito do produto.
