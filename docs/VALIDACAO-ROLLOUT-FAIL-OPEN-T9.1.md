# Rollout fail-open e falsos positivos — T9.1

Data da prova: 28 de agosto de 2026.

## Escopo

Esta prova usa somente o binário AntDelirium, o serviço limitado instalado e
cópias descartáveis de `tests/fixtures/t76-standard-cargo` sob `target/`. Ela
não lê, migra nem modifica AutoBrowser ou qualquer outro projeto externo.

O gate reproduzível é:

```bash
make fail-open-rollout
```

Ele deve ser executado pelo operador sem `sudo`. O socket limitado instalado
precisa estar ativo. O gate constrói o release revisado, mantém o enforcement
desativado e grava evidência fechada conforme
`fail-open-rollout-evidence-v1`.

## Política implementada

O worker de auditoria continua separado do compilador: o processo principal
faz `exec` do `rustc`, enquanto falhas do worker nunca alteram stdout, stderr,
status ou sinais do compilador. As falhas internas agora têm classes fechadas:

- configuração do endpoint;
- serviço indisponível;
- falha temporária;
- protocolo;
- erro interno;
- materialização das projeções públicas.

A classificação permanece interna e silenciosa no rollout. Ela não revela ao
agente endpoints, caminhos protegidos ou detalhes do serviço. Uma falha real
do compilador continua tendo precedência sobre qualquer degradação da
auditoria.

## Casos exercitados

O gate comprovou que a compilação permanece inalterada quando:

1. a integração não fornece endpoint ao wrapper;
2. o endpoint aponta para um serviço indisponível;
3. um peer Unix sintético conclui o handshake com envelope malformado;
4. `guard/` é substituído, na fixture descartável, por um symlink não
   confiável que impede a materialização.

No quarto caso, o worker não substituiu o symlink e não escreveu através dele.
O teste de protocolo usa framing real `ADL1`, timeout limitado e um endpoint
sob `target/`, sem depender de processo ou projeto externo.

## Medição observada

A execução `20260828T165617Z-34286` terminou com exit zero. O release medido
teve SHA-256
`5c3c510dfa4e294e90728876efb3da35cbb9acb6e260df08a613ab8f8dcb1bbb`.

Foram feitas três observações independentes e válidas, cada uma com workspace
e target novos:

- `cargo check`;
- `cargo build`;
- `cargo test`.

Todas acionaram o wrapper da compilação primária, receberam as duas projeções
válidas do serviço instalado e produziram zero violações. Portanto, a amostra
do gate teve `valid_observation_count=3` e `false_positive_count=0`.

Uma fonte Rust deliberadamente inválida também foi compilada diretamente e
através do wrapper com o serviço indisponível. Ambos retornaram o mesmo status
não zero e preservaram o diagnóstico Rust, sem mensagem `AD-*` no fluxo do
compilador.

## Limite e decisão

Três fixtures sintéticas válidas não representam uma taxa estatística de falso
positivo em produção. Elas formam um gate determinístico de regressão, não uma
afirmação sobre toda base Rust possível. Por isso esta tarefa não ativa
fail-closed: `enforcement_activated=false` permanece parte obrigatória do
contrato. Bypass e cenários hostis são tratados separadamente em T9.2 a T9.6;
somente T9.7 pode ativar bloqueio para violações explicitamente definidas.

## Resultado

T9.1 está concluída: a política fail-open é reproduzível, as falhas internas
estão classificadas sem divulgação ao agente, o compilador mantém precedência
e o gate registrou zero falsos positivos nas três observações mantidas.
