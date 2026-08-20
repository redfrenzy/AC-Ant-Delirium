# Validação do cutover transacional controlado — AutoBrowser T8.6

Data da prova: 2026-08-20.

## Escopo

Esta prova implementa e exercita a mecânica de troca de autoridade em uma cópia
privada sob o storage de teste do projeto. Ela não instala o serviço global,
não altera o Cargo home real e não declara que o AutoBrowser original já está
operando pelo wrapper global.

O comando administrativo fechado é `migration.cutover`. Ele exige capability
global `Operator/Administrative/StateAdmin` e não aparece no help ou na API
limitada entregue ao agente. O request contém apenas IDs opacos, hashes e a
janela de rollback; nenhuma localização física atravessa o protocolo.

## Transação de autoridade

[`Legacy-Cutover_unit`](../src/Legacy-Cutover_unit.rs) revalida o manifesto
`approved`, relatório de conversão, atestação CI, evento de aprovação do
operador, fingerprint lógico e saúde SQLite 7. Em seguida:

1. cria uma cópia consistente do candidato pela API de backup online do SQLite;
2. grava na cópia promovida o primeiro evento da nova autoridade;
3. publica a raiz privada do projeto por rename e sincroniza o diretório;
4. registra projeto, workspace e binding no registry em uma única transação;
5. publica o manifesto `cutover` por arquivo parcial privado e rename atômico.

A linha `projects` do registry é o ponteiro canônico já usado pelo resolver. O
evento `migration-cutover-binding-v1`, persistido na mesma transação, vincula o
ponteiro aos hashes aprovados sem acrescentar paths. Isso evita mudar o schema
7 durante o mesmo passo operacional. Não há dual-write: o candidato e o legado
ficam congelados, enquanto somente o banco publicado passa a ser resolvido pelo
serviço.

Uma interrupção antes da transação do registry deixa no máximo uma cópia
promovida ainda não selecionada. Uma interrupção depois dela deixa projeto e
binding já consistentes; nova execução termina a publicação do manifesto. A
operação foi executada duas vezes e retornou o mesmo recibo, sem duplicar evento
ou registro.

## Evidência controlada

- migration: `mig_4tdwqnh4dngbhaxhehbvzzsx4t`
- project: `prj_19vszrww3v57118bqhakg704vd`
- workspace: `wsp_7er1pg0mn2cj0mwd7a8jf3sq4k`
- cutover: `cut_7x22ddwydy3m26gq3ewkn9twvt`
- manifesto aprovado: `80647a3d931c36e44584890e1e2cf87e1883d79322585f2877392e0d0525b241`
- manifesto após cutover: `bab8d755faa16460686a381784031233f1c606084df84821e23ec49a39e14b19`
- binding canônico: `fa4d2826eef32d3b9191a8d8ee1f675d7eafc9b994a97ae1d2e9636bf9c801b2`
- recibo de cutover: `85ac5997a23d09fda9f10de04d360b9d424645080beca35d8d6d30834d18c9de`
- primeira sequência pós-cutover: `3`
- instante do cutover: `2026-08-20T06:58:20Z`
- deadline mínimo de rollback: `2026-09-19T06:58:20Z`
- rollback: `available`
- origem preservada: `true`

O manifesto aprovado, a aprovação `apr_487na5p624m08rh7qydqd8nkxh`, a
atestação `att_6dy1g04n17nw1ygr3b6s5yw6jc`, o snapshot legado e o fingerprint
lógico foram vinculados integralmente. O schema do candidato não mudou.

`make ci` terminou verde com 189 testes da biblioteca, 281 testes do binário
mais sete provas reais ignoradas por padrão e nove testes de integração. O
release foi compilado com warnings como erro e não contém paths físicos
protegidos ou nomes de logs globais previsíveis. Fixtures antigos de capability,
staging e atestação também passaram a incluir nonce temporal no nome: execuções
repetidas preservam `target/` sem colidir com artefatos anteriores.

## Limite restante

Esta é prova mecânica em storage controlado. A conclusão da T8.6 ainda exige a
instalação das identidades/unidade, a ativação do serviço e wrapper globais e a
troca operacional do AutoBrowser real. A validação funcional pós-cutover e o
rollback pertencem à T8.7; a origem não pode ser arquivada ou apagada neste
passo.
