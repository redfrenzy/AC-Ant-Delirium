# Matriz de bypass e revalidação canônica — T9.2

Data da prova: 28 de agosto de 2026.

## Escopo

Esta prova usa somente o release AntDelirium e cópias descartáveis da fixture
`tests/fixtures/t76-standard-cargo` sob `target/`. Nenhum projeto externo,
estado canônico protegido ou socket de serviço é necessário.

O gate reproduzível é:

```bash
make bypass-matrix
```

O relatório público segue o contrato fechado
`bypass-matrix-evidence-v1`, não contém localizadores físicos e mantém
`enforcement_activated=false`.

## O que a matriz precisa provar

Uma configuração global em `$CARGO_HOME` melhora o fluxo normal, mas não é uma
fronteira contra código hostil executado pelo mesmo usuário. Portanto o teste
não considera sucesso esconder ou negar essa limitação. Cada cenário tem duas
etapas obrigatórias:

1. comprovar que o fluxo local consegue compilar sem o wrapper confiável;
2. revalidar a fonte canônica em target novo, removendo overrides e selecionando
   `rustc` e wrapper confiáveis pela política do CI/supervisor.

A segunda etapa verifica no `cargo -vv` uma compilação primária com
`CARGO_PRIMARY_PACKAGE=1` através do release revisado. A política protegida não
aceita como prova o build local que acabou de ser contornado.

## Vetores exercitados

A execução `20260828T170233Z-23` comprovou os cinco vetores previstos:

- `RUSTC_WORKSPACE_WRAPPER=""` no ambiente;
- `rustc-workspace-wrapper = ""` em configuração local mais próxima;
- `cargo --config 'build.rustc-workspace-wrapper=""'`;
- chamada direta ao `rustc` real;
- cópia byte a byte da fonte para outro workspace, seguida de compilação com a
  integração local desabilitada.

Nos quatro casos Cargo, o controle local mostrou a linha de compilação primária
chamando diretamente o `rustc`, sem o release AntDelirium. No caso `rustc`
direto, o compilador produziu o artefato sem passar pelo Cargo. Nenhum caso
publicou `guard/`.

Depois de cada controle, o gate recompilou a fonte canônica em target limpo,
limpou os seis overrides oficiais e injetou por configuração do supervisor:

- o `rustc` real selecionado;
- wrapper externo vazio;
- o release AntDelirium como workspace wrapper;
- endpoint sintético indisponível, preservando o modo fail-open da T9.1.

Todas as cinco revalidações passaram e comprovaram participação do wrapper
confiável. No cenário de cópia, o digest compara apenas conteúdo, não paths, e
a cópia não substitui a revalidação da raiz canônica escolhida pelo supervisor.

## Evidência observada

O relatório registrou:

```text
case_count=5
local_bypass_observed_count=5
protected_revalidation_count=5
authority_boundary=ci-supervisor-required
copy_case=canonical-source-revalidated
enforcement_activated=false
```

O release medido teve SHA-256
`82ac81be96fe507ba98a63f63a125b75cfbdf77bcd247db66f1f854a61c0727c`.

## Limite e decisão

O gate local implementa e testa a mecânica de revalidação. Ele não transforma
este checkout, seu script ou seu digest em autoridade externa. Para impedir que
o agente modifique simultaneamente código, gate e resultado, o runner e o
digest esperado precisam ser fornecidos por CI/supervisor não gravável pelo
agente. Assinatura e branch protection continuam em T9.6 e T9.7.

## Resultado

T9.2 está concluída. Os cinco bypasses foram observados, não ocultados, e todos
exigiram revalidação canônica pelo wrapper confiável. A conclusão é que
enforcement real pertence ao CI/supervisor protegido, não à configuração Cargo
gravável pelo mesmo usuário.
