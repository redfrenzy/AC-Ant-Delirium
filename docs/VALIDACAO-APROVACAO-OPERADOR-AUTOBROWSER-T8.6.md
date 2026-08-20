# Validação da aprovação do operador no AutoBrowser — T8.6 parcial

Data da execução: 2026-08-20.

## Escopo desta etapa

Esta etapa implementa e executa a decisão explícita do operador sobre o
candidato já atestado. Ela não instala ou ativa o serviço/wrapper global, não
troca o ponteiro canônico, não inicia a janela de rollback e não executa
cutover. A origem legada continua intacta e recuperável; a T8.6 permanece
aberta.

## Autoridades e contratos

`migration.approve` é uma operação administrativa fechada, ausente da API do
agente. Ela exige capability global
`Operator/Administrative/StateAdmin`. O request path-free vincula exatamente:

- migration, projeto e workspace;
- manifesto anterior, snapshot de origem e snapshot de staging;
- relatório de conversão;
- ID e digest da atestação independente;
- fingerprint lógico do candidato;
- snapshot legado que forma o rollback point.

O serviço deriva `operator_subject_sha256` da identidade e do grant
autenticados. Agente, wrapper, revisor, projeção e JSON do projeto não podem
produzir essa autoridade.

Foram adicionados os contratos fechados
`migration-approval-request-v1` e `migration-approval-receipt-v1`. O manifesto
foi corrigido para representar a separação real de decisões:

- o verificador registra `decision=attested` e aponta para a evidência da
  atestação;
- o operador registra `decision=approved` e aponta para o request integral;
- `rollback_deadline` permanece nulo em `approved`, pois os 30 dias mínimos
  começam no instante do futuro cutover.

O `approval_sha256` é calculado sobre a serialização JSON compacta do recibo com
o próprio campo temporariamente vazio, evitando uma referência circular e
permitindo recomposição determinística no replay.

## Revalidação e retomada

Antes de aprovar, o serviço reabre o relatório, o manifesto e o SQLite
candidato com política privada, exige schema 7, `quick_check=ok`, foreign keys
ativas e nenhuma violação. Ele localiza a atestação no banco, recalcula seu
digest, exige seu evento CI correspondente, confere os seis checks e recalcula
o fingerprint lógico sobre planos, tarefas, registros protegidos e governança.

A aprovação usa duas fases recuperáveis:

1. grava o evento de operador no journal transacional do candidato;
2. publica o manifesto aprovado por arquivo privado, `rename` atômico e `fsync`
   do diretório.

Se houver interrupção entre essas fases, uma repetição encontra o evento pelo
ID determinístico, reutiliza o mesmo instante e só aceita o manifesto parcial
se os bytes forem exatos. Depois da publicação não sobra arquivo parcial. Uma
segunda execução real retornou o mesmo recibo, sem duplicar evento ou sequência.

## Resultado real

Bindings aprovados:

- migration: `mig_4tdwqnh4dngbhaxhehbvzzsx4t`;
- projeto: `prj_19vszrww3v57118bqhakg704vd`;
- workspace: `wsp_7er1pg0mn2cj0mwd7a8jf3sq4k`;
- manifesto anterior:
  `d9cba477a77a4c1b0a6cef9c9521e53fa678a3a16507ae951160d9dcdfeb27ec`;
- manifesto aprovado:
  `80647a3d931c36e44584890e1e2cf87e1883d79322585f2877392e0d0525b241`;
- snapshot de origem/rollback point:
  `7d11b66e162a10313ae3601982ce234653329743031d4a1a9ec874c34b2ccbcd`;
- snapshot de staging:
  `859131fcac8810989ccfb594fedb77db452556355950c4d3879a0741e654ba82`;
- relatório de conversão:
  `4c6382ed324b304fe1ee94082b81f3685eaf5973055592e37088aeed5e5b38a5`;
- atestação: `att_6dy1g04n17nw1ygr3b6s5yw6jc`;
- digest da atestação:
  `ae9e966be9b19e22eec5a18ba5a700500bcb160dd40ef8eb912e9c32e0117617`;
- fingerprint lógico:
  `6c7f433a86998f3c26c461135265756d798f9a370d779bab4619d8343aa6655a`;
- identidade hash do operador local:
  `eca1acc8185f0fb5fe7823ba086214436d3dd5d9062a5e691bb2d7d99e9fad93`;
- request de aprovação:
  `2be45801f41ab9ff31ec1bd52b512556521b84b7abf6dd72f92339d436c949ac`;
- aprovação: `apr_487na5p624m08rh7qydqd8nkxh`;
- digest da aprovação:
  `b02a42303124aa0b24d8c3d9237275ceb5951ebf46c2017dcc18ba412cb384c9`;
- decisão do operador: `2026-08-20T06:56:40Z`.

O manifesto terminou em `approved`, com `cutover_at=null`,
`rollback_deadline=null`, `post_cutover_first_sequence=null` e rollback
`prepared`. O banco terminou com uma atestação e dois eventos: sequência 1 da
CI e sequência 2 do operador. O schema continuou 7, `quick_check=ok` e zero
violações de foreign key.

## Preservação e limite da prova

A execução real chamou o núcleo mecânico com um hash explícito de operador
local; ela não comprova uma identidade de operador implantada nem assinatura
externa. A rota de produção exige a capability global autenticada e essa
separação é coberta pelos testes do protocolo e da matriz de capabilities.

A origem AutoBrowser continuou no HEAD
`f7405872e980da505eadc53e40be6589a7507aa4`. O hash de `git status --short`
permaneceu
`f3b371a69fbb42474abbc4bb6c76324f601896b99f3be7d34e0933c062013033`,
igual às provas anteriores.

O estado final é `approved_awaiting_cutover`. O próximo passo não deve presumir
que a aprovação por si só mudou a autoridade: ainda é necessário definir e
testar o cutover transacional, instalar/ativar o serviço e wrapper globais e
somente então iniciar uma janela de rollback de pelo menos 30 dias.

## Gate final

`make ci` passou com:

- 189 testes da biblioteca;
- 277 testes do binário aprovados e seis provas reais ignoradas por exigirem
  locators fornecidos pelo operador;
- integrações 2+1+6 aprovadas;
- check com warnings negados no target `x86_64-unknown-linux-gnu`;
- contratos JSON e inventário T8.2 válidos;
- release sem paths físicos protegidos ou nomes globais previsíveis de log.

O manifesto aprovado real também passou no JSON Schema Draft 2020-12 com
validação de formatos. A inspeção final confirmou ausência de arquivo parcial,
schema 7, `quick_check=ok`, zero violações de foreign key, uma atestação e dois
eventos auditados em sequência.
