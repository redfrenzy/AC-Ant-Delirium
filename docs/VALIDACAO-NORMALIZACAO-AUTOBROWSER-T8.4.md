# Normalização e equivalência do AutoBrowser — T8.4

## Resultado

A origem validada na T8.2 e a cópia protegida publicada na T8.3 produziram a
mesma representação semântica após a normalização de referências físicas. A
origem, o `source/` do staging e o manifesto de migração não foram editados. A
nova saída é uma derivação privada em `normalized/`; ela não é estado canônico
e não autoriza cutover.

Recibo observado:

| Campo | Valor |
|---|---:|
| `migration_id` | `mig_4tdwqnh4dngbhaxhehbvzzsx4t` |
| `project_id` | `prj_19vszrww3v57118bqhakg704vd` |
| entries comparados | 3.102 |
| arquivos regulares | 2.722 |
| diretórios | 380 |
| documentos JSON | 144 |
| referências físicas distintas | 47 |
| ocorrências substituídas | 208 |
| digest normalizado | `ec9569a4e652a73813466ae82c687868f9145245e8e5ce543b707ed5518d98e0` |
| digest semântico | `b66eb44d7c6e01d6ad0061e36ba87cef40db758add47287be15631a4a4129fad` |
| comparação | `equivalent` |
| estado da derivação | `normalized` |
| estado do manifesto | `staged` |

O source digest permaneceu
`7d11b66e162a10313ae3601982ce234653329743031d4a1a9ec874c34b2ccbcd`
e o staging digest permaneceu
`859131fcac8810989ccfb594fedb77db452556355950c4d3879a0741e654ba82`.

## Como a comparação funciona

1. O cliente operador reabre a origem usando o inventário T8.2. Cada componente
   é percorrido com `openat`, `O_NOFOLLOW` e descriptor da raiz; conteúdo,
   metadata e enumeração final precisam continuar iguais ao ledger.
2. Somente arquivos UTF-8 sem controles binários são classificados como texto.
   Localizadores POSIX sob raízes físicas fechadas, `~/...` e paths Windows são
   extraídos. URLs e referências relativas como `plan/task.json` não são
   classificadas como paths físicos.
3. Para cada valor físico distinto o operador gera um ID `ref_` aleatório. O
   request contém somente SHA-256 do valor e ID; o valor e o locator da origem
   não atravessam o socket.
4. O serviço revalida os bytes do `source/` staged contra o mesmo inventário e
   aplica exatamente o mapa recebido. Binding ausente, duplicado, extra ou ID
   inválido falha fechado.
5. Para JSON, a comparação usa SHA-256 da serialização canônica após
   normalização. Para os demais textos usa os bytes normalizados; para binários,
   os bytes originais. O snapshot final inclui path relativo, tipo e digest de
   cada entry em ordem.
6. Os digests calculados independentemente na origem e no staging precisam ser
   idênticos antes da publicação atômica de `normalized/`.

Isso compara conteúdo e estrutura sem tratar formatação de JSON como mudança
semântica. Não tenta interpretar o significado de código Rust, shell ou texto
livre além da transformação fechada de localizadores.

## Fronteira administrativa

`migration.normalize` é uma variante fechada do protocolo `ADA1` e exige a
mesma capability global `Operator/Administrative/StateAdmin` usada no staging.
O cliente comum e o wrapper Cargo não recebem socket administrativo, descriptor
de capability, origem, inventário ou migration ID. A operação interna
`migrate-plugin normalize` não aparece na API nem na ajuda do agente.

O serviço é o único writer. Ele grava com modos `0700`/`0600`, usa
`normalized.partial`, sincroniza arquivos/diretórios e publica por rename. O
documento `physical-reference-bindings-v1` contém somente hashes e IDs opacos;
o recibo contém apenas IDs, contagens, estados e digests.

O transporte mantém 120 segundos para handshake e frames comuns e reserva 600
segundos somente para stream/resposta das operações de migração. A prova real
levou cerca de 143 segundos por causa dos `fsync`, portanto o deadline comum
seria insuficiente mesmo sem bloqueio externo.

## Evidência real

A prova ignorada por padrão foi executada explicitamente com os locators
fornecidos pelo operador. O teste levou aproximadamente 143 segundos porque a
derivação sincroniza individualmente os 2.722 arquivos. Foram observados:

- 2.722 arquivos e 380 diretórios na derivação;
- 144 JSONs aceitos por `jq empty`;
- 47 hashes de binding, 47 IDs `ref_` e nenhuma duplicata;
- nenhuma ocorrência reconhecível sob os roots físicos fechados POSIX,
  `~/...` ou Windows na derivação atual;
- ausência de `normalized.partial` após a publicação;
- `.git/index` com SHA-256
  `6d4535d10692684b3e1194b5fa7a2c650dbafe891e0e9cacbf01411111c25652`
  tanto no staging original quanto na derivação;
- `context.json` original staged com SHA-256
  `651a6bcd3ca266630ff63aeb52486dd97e851b1a09d53603b59d1156de66c490`,
  igual ao ledger T8.2.

Duas derivações preliminares identificaram casos que não podiam ser ignorados:
sequência semelhante a path dentro do `.git/index` binário, paths físicos
precedidos por `=` e roots de sistema presentes em shebangs/probes, como
`/bin/`, `/dev/` e `/proc/`. O classificador foi corrigido para preservar
binários, reconhecer delimitadores de atribuição e cobrir roots POSIX físicos
sem converter rotas lógicas. As gerações anteriores foram mantidas sob
`target/` como evidência local, não promovidas nem apagadas. Os números acima
pertencem exclusivamente à derivação definitiva.

Após a prova, a origem continuou com:

- HEAD `f7405872e980da505eadc53e40be6589a7507aa4`;
- hash SHA-256 do `git status --porcelain=v1 --untracked-files=all`:
  `f3b371a69fbb42474abbc4bb6c76324f601896b99f3be7d34e0933c062013033`.

São os mesmos valores registrados nas T8.2 e T8.3.

O gate final `make ci` passou com 188 testes da biblioteca, 267 testes do
binário (mais as duas provas reais ignoradas por padrão), integrações 2+1+6,
checks com warnings negados, schemas JSON válidos e release sem caminhos
físicos protegidos.

## O que permanece aberto

A T8.4 não importou dados no SQLite canônico, não executou migrations do estado
alvo, não mudou o manifesto para `validated`, não coletou approval e não fez
cutover. A T8.5 deve produzir dry-run e relatório explícito de diferenças,
inclusive contagens por domínio e tudo que não puder ser convertido sem decisão
humana, antes de qualquer autorização sobre um único projeto.
