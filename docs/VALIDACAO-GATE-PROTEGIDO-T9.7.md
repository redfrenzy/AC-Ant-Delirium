# Gate fail-closed protegido — T9.7

Data da prova mecânica: 1 de setembro de 2026.

## Estado honesto

A política e a decisão fail-closed assinada estão implementadas e passaram na
matriz sintética. A autoridade root-owned também foi instalada e validada no
host. A proteção de branch em um provedor hospedado ainda não está ativa: este
checkout não possui remoto nem workflow de CI configurado. Por isso a T9.7
permanece aberta e nenhum texto deste documento trata a autoridade instalada
como proteção de branch já implantada.

## Onde o bloqueio ocorre

O wrapper usado pelos comandos Cargo permanece `audit-only-local` e continua
fazendo `exec` do `rustc`. Indisponibilidade do serviço não substitui o status,
o sinal ou o diagnóstico do compilador. O fail-closed é uma decisão posterior
do CI/supervisor externo e deve ser publicado no contexto obrigatório
`antdelirium/protected-gate`.

O gate exige simultaneamente:

1. compilação primária em target limpo pelo wrapper cujo digest veio da
   autoridade protegida;
2. evidência mecânica em modo `protected-digest`, nunca `local-mechanics`;
3. checkpoint de integridade Ed25519 válido e vinculado à chave externa;
4. política fechada e atestação final assinada pela autoridade externa.

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

## O que falta para concluir T9.7

Falta uma prova externa:

1. configurar no provedor real as branches relevantes para exigir o contexto
   `antdelirium/protected-gate` e provar que merge sem esse resultado é negado.

O segundo item depende de existir um remoto e de saber qual provedor administra
o repositório; nenhum arquivo local pode provar essa configuração sozinho.
