# Validação da autoridade das projeções — T9.5

## Objetivo

Provar que `guard/to-do_list.json` e `guard/governance_error.json` são projeções
descartáveis para orientação do agente, nunca o estado autoritativo. Editar,
apagar, trocar permissões ou substituir esses arquivos por links não pode mudar
plano, tarefa, revisão, snapshot de governança ou decisão canônica.

Também é obrigatório demonstrar que a superfície pública recusa comandos de
mutação e que uma capability limitada de agente não pode instalar pacote de
tarefas nem avançar uma tarefa. As tentativas que alcançam o serviço precisam
ser registradas sem payload, segredo ou localização física.

## Comando único

```bash
make projection-authority-matrix
```

O gate não usa `sudo`, serviço instalado ou projeto externo. Ele executa somente
fixtures sintéticas deste repositório sob `target/` e o release local revisado.

## Matriz

1. Exclui e adultera `to-do_list.json`, tenta introduzir tarefa falsa,
   `implemented=true` e referência privada; a projeção é reconstruída byte a
   byte e estado, plano e payload da tarefa permanecem idênticos.
2. Exclui e adultera `governance_error.json`, tenta introduzir violação falsa,
   plano e quick-fix administrativo; projeção, snapshot protegido e estado
   canônico permanecem idênticos.
3. Exercita a gramática fechada da CLI do agente contra overrides, conclusão,
   revisão, nota, criação de tarefa e comandos legados/inventados; todos retornam
   erro de autorização.
4. Invoca o release real contra conclusão e troca de plano. Ambos retornam exit
   `77`, uma única mensagem pública e não repetem o argumento hostil.
5. Entrega ao serviço uma capability válida de `Agent/Limited/SubmissionCreate`
   e tenta usá-la para instalar e avançar tarefas. As duas chamadas são negadas,
   a revisão canônica permanece idêntica e dois eventos opacos distintos são
   persistidos.

## Evidência

Cada execução grava logs locais em
`target/t95-projection-authority/<run-id>/`. O resumo
`projection-authority-evidence-v1.json` segue o contrato fechado
[`projection-authority-evidence-v1`](./contracts/v1/projection-authority-evidence-v1.schema.json).
O JSON não contém paths físicos, IDs reais, argumentos hostis ou conteúdo das
projeções; ausência de ferramenta, teste filtrado, diagnóstico divergente ou
quantidade incorreta de eventos falha o gate.

## Resultado observado

Em 28 de agosto de 2026, a execução
`20260828T204737Z-23` terminou com exit `0`. As recuperações após exclusão e
adulteração passaram para as duas projeções; plano, tarefa, snapshot e estado
canônicos permaneceram `unchanged`. Os dois probes públicos e as duas mutações
enviadas com identidade de agente foram `denied`; o serviço persistiu exatamente
dois eventos distintos de rejeição, e o teste de não divulgação confirmou
ausência de locator no journal. Nenhuma fonte ou projeto externo foi acessado ou
alterado pelo gate.
