# Validação adversarial de isolamento do cliente

Tarefa: T4.8

## Objetivo

Provar com mecanismos reais do kernel que um processo no domínio do cliente não
consegue resolver, testar, abrir ou enumerar o armazenamento canônico mesmo
quando recebe antecipadamente o caminho exato. A validação não depende de nome
secreto, `chmod` aplicado a arquivos do mesmo UID ou resposta simulada da API.

## Fronteira exercitada

O alvo `make adversarial-client-isolation` inicia um processo descartável com
Bubblewrap e aplica conjuntamente:

- namespaces novos de usuário, PID, mount, rede, IPC e UTS;
- UID/GID `65534` internos e bounding set de capabilities vazio;
- bloqueio de novos user namespaces dentro do cliente;
- `/proc` novo contendo somente processos do namespace;
- visão mínima com `/usr`, `/etc` e o checkout atual somente-leitura;
- `/var`, `/run` e `/tmp` novos, sem conteúdo do host;
- sobreposição vazia no pai de uma fixture que existe e é legível no host.

A fixture controlada evita um falso positivo causado por testar somente um path
que já não existia. O processo externo confirma primeiro que o conteúdo existe;
o cliente recebe o path exato via ambiente e ainda assim precisa obter falha em:

1. `stat`;
2. `access` nas formas de existência, leitura e travessia;
3. `readlink`;
4. `realpath -e`;
5. enumeração com `ls`;
6. abertura/leitura com `dd`.

O mesmo conjunto é repetido contra o layout conhecido do serviço. O PID do
launcher no host também precisa estar ausente do `/proc` do cliente, enquanto
`Cargo.toml` permanece legível para provar que o projeto continua disponível.

## Execução

```sh
make adversarial-client-isolation
```

Sucesso produz somente:

```text
AD-T48: isolated client cannot resolve or access protected state.
```

O probe não instala conta, unidade, socket ou política global e não acessa planos
reais. Ele comprova a política de isolamento reproduzível usada para fechar G4;
a instalação final da mesma fronteira e os testes contra serviço ativo continuam
na Onda 9.

## Evidência executada

Em 2026-08-13, o alvo foi executado no host Linux 7.0 com Bubblewrap 0.9.0. O
processo interno confirmou UID/GID `65534`, capabilities efetivas zeradas,
proibição de criar outro user namespace, projeto legível e PID do launcher
ausente de `/proc`. Todas as operações adversariais falharam e o comando terminou
com exit `0` e a única linha pública prevista.

A execução precisou sair do sandbox do ambiente de desenvolvimento porque esse
sandbox já bloqueia a criação de um namespace de rede aninhado. A autorização
foi concedida somente ao alvo `make adversarial-client-isolation`; o próprio alvo
então criou sua fronteira descartável sem privilégios de root.
