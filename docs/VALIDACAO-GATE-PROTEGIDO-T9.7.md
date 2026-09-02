# Gate fail-closed protegido — T9.7 parcial

Data da prova mecânica: 1 de setembro de 2026.

## Estado honesto

A política e a decisão fail-closed assinada estão implementadas e passaram na
matriz sintética. A autoridade root-owned também foi instalada e validada no
host. A T9.7 permanece aberta porque a decisão ainda não é exigida pela
transição canônica local de tarefa. Repositório remoto, workflow hospedado e
proteção de branch não são requisitos do AntDelirium; publicar a mesma decisão
em um provedor é uma integração opcional.

## Onde o bloqueio ocorre

O wrapper usado pelos comandos Cargo permanece `audit-only-local` e continua
fazendo `exec` do `rustc`. Indisponibilidade do serviço não substitui o status,
o sinal ou o diagnóstico do compilador. A decisão fail-closed já pode ser
produzida por uma autoridade protegida, mas ainda falta fazer o serviço local
exigi-la antes de aceitar o avanço canônico. O nome histórico de contexto
`antdelirium/protected-gate` pode ser publicado por CI, sem tornar CI parte do
fluxo obrigatório.

O gate local definitivo deverá exigir simultaneamente:

1. compilação primária em target limpo pelo wrapper cujo digest veio da
   autoridade protegida;
2. evidência mecânica em modo `protected-digest`, nunca `local-mechanics`;
3. checkpoint de integridade Ed25519 válido e vinculado à chave externa;
4. política fechada e atestação final assinada pela autoridade local protegida;
5. vínculo com projeto, workspace, tarefa, revisão e fingerprint atual do
   código antes da transição.

Falha do compilador continua sendo falha do compilador. Ela impede o conjunto
de provas de ficar completo, mas não é convertida em uma falsa violação de
governança.

## Violações que podem bloquear

A política normativa
[`protected-branch-policy-v1`](./contracts/v1/protected-branch-policy-v1.json)
define exatamente sete códigos:

- `limited-protocol-invalid`;
- `primary-wrapper-bypass`;
- `projection-unavailable`;
- `protected-service-unavailable`;
- `signed-state-invalid`;
- `source-mutated-during-gate`;
- `trusted-wrapper-digest-mismatch`.

Código desconhecido não é aceito como nova regra implícita: ele invalida o
contrato. Cache sem recompilação primária, probes da toolchain, dependências,
workloads não primários e observações de regra continuam consultivos. Em
especial, `code-rule-observation` ainda não bloqueia: o serviço protegido atual
registra compilação e materializa projeções, mas ainda não executa
sincronamente todo o motor maduro de regras sobre a fonte canônica. Promover
essa observação agora seria alegar uma ligação que o código não possui.

## Evidência assinada

[`protected-branch-authority.py`](../packaging/ci/protected-branch-authority.py)
valida contratos fechados, verifica a assinatura do checkpoint de T9.6 e gera
uma atestação path-free que vincula os hashes da política, evidência mecânica e
checkpoint, além dos hashes da fonte e do wrapper. A chave privada precisa ter
permissão restrita e não aparece na atestação.

A verificação repete a assinatura Ed25519, o hash da política, os conjuntos de
códigos e a coerência `passed`/`blocked`. Uma atestação com pelo menos uma
violação definida retorna status 1; erro de contrato ou assinatura retorna 2.

## Matriz executada

O comando reproduzível, sem `sudo`, é:

```bash
make protected-branch-gate
```

A execução `20260901T185440Z-15`:

- aceitou e verificou uma decisão limpa assinada;
- bloqueou e verificou individualmente os sete códigos definidos;
- recusou código desconhecido;
- recusou evidência limitada a `local-mechanics`;
- recusou política divergente;
- recusou checkpoint assinado alterado;
- recusou conteúdo da atestação alterado após a assinatura.

Toda a prova foi criada sob `target/`, usou somente fixture sintética e não
consultou projeto externo nem estado instalado.

## Prova instalada

O bundle `make protected-branch-authority-bundle` prepara, sem chave, o
avaliador e a política para destinos root-owned. A rotina
[`install-protected-branch-authority.sh`](../packaging/systemd/install-protected-branch-authority.sh)
instala apenas esses dois arquivos revisados, cria uma chave Ed25519 exclusiva
sob `/var/lib/antdelirium-branch-gate`, comprova que operador e serviço não
conseguem lê-la ou gravar sua raiz e testa o par de chaves. Ela não compila como
root e publica os logs em `target/` através da identidade normal do operador,
sem escrever como root em um destino controlado pelo usuário.

A execução instalada `20260901T222425Z` passou com:

```text
exit_code=0
authority_installed=yes
policy_installed=yes
private_key_present=yes
provider_branch_protection=not-configured
```

O teste criptográfico retornou `Signature Verified Successfully`. O log
público não contém chave, path da chave, conteúdo protegido ou outro locator
administrativo. A chave exclusiva permanece na raiz root-owned e a rotina
comprovou que nem o operador normal nem a identidade do serviço conseguem
lê-la ou gravar sua âncora.

Esse campo apenas confirma que o instalador não altera serviços externos. Ele
não representa uma pendência do produto.

## O que falta para concluir T9.7

Falta uma prova local instalada:

1. persistir uma decisão vinculada à revisão corrente do código;
2. exigir essa decisão na operação `governance.task.advance.accepted`;
3. recusar auditoria ausente, obsoleta, adulterada ou com código bloqueante;
4. provar que o agente não consegue emitir a decisão nem avançar a tarefa;
5. provar cache válido quando o fingerprint não mudou e nova auditoria quando
   o código mudou.

Configurar `antdelirium/protected-gate` em GitHub, GitLab ou outro provedor pode
reutilizar a atestação existente, mas fica fora do gate local do produto.
