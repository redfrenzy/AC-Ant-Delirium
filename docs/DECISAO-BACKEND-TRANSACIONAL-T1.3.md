# Decisão do backend canônico

Versão: `storage-v1`  
Tarefa: T1.3  
Decisão: SQLite transacional em modo WAL

Estado atual: a decisão nasceu como `storage-v1` e permanece válida; o schema de
produção evoluiu por migrations monotônicas até `storage-v4`, que acrescenta o
journal tipado de segurança do serviço sem substituir o journal `events` de
cada projeto.

## Resultado

O estado canônico do AntDelirium será armazenado em um banco SQLite protegido. JSON continuará existindo somente como projeção sanitizada, exportação compatível, fixture ou artefato de diagnóstico autorizado.

O backend Rust inicial será:

```toml
rusqlite = { version = "=0.37.0", default-features = false, features = ["bundled"] }
```

Essa versão foi compilada e executada em Rust 1.88 com SQLite 3.50.2 embutido. `ldd` não encontrou dependência dinâmica de SQLite, preservando o requisito de um único binário distribuível.

A dependência já integra o crate principal por meio do wrapper em `src/libres`,
com pin no lockfile e validação no MSRV 1.88.

## Protótipos executados

Os protótipos ficaram em `tmp/`, fora do Git. Ambos receberam 16 escritores concorrentes com 100 eventos por escritor.

### Journal JSONL com lock

Modelo:

- `flock` exclusivo por evento;
- contador de sequência em arquivo separado;
- append em `events.log`;
- snapshot em arquivo temporário seguido de rename.

Resultado inicial:

```text
tempo: 12,84 s
linhas: 1.600
fingerprints únicos: 1.600
última sequência: 1.600
```

No replay dos mesmos fingerprints:

```text
tempo: 13,17 s
linhas: 3.200
fingerprints únicos: 1.600
```

O lock de kernel foi liberado depois que um escritor foi morto, mas o journal permaneceu com uma linha JSON parcial enquanto o snapshot continuou na sequência anterior. Recuperação segura exigiria framing, checksum, scanner de cauda, truncamento validado, índice persistente de fingerprints, compactação e coordenação adicional entre journal e snapshot.

O lockfile `create_new` usado atualmente pelo runtime é ainda mais frágil: uma saída sem `Drop` pode deixar arquivo stale e bloquear operações seguintes até intervenção.

### SQLite transacional/WAL

Modelo:

- SQLite 3.50.4 no spike de CLI;
- `journal_mode=WAL`;
- `synchronous=FULL`;
- `busy_timeout=5000`;
- transações `BEGIN IMMEDIATE`;
- constraint `UNIQUE` para fingerprint.

Resultado inicial:

```text
tempo: 1,70 s
eventos: 1.600
fingerprints únicos: 1.600
integrity_check: ok
```

No replay:

```text
tempo: 1,58 s
eventos: 1.600
fingerprints únicos: 1.600
integrity_check: ok
```

Um escritor foi morto depois de `BEGIN IMMEDIATE` e de um insert não commitado. Ao reabrir:

- o evento não commitado não existia;
- `integrity_check` retornou `ok`;
- a transação seguinte foi aceita normalmente.

O protótipo também revelou um detalhe importante: `AUTOINCREMENT` pode avançar sua sequência interna mesmo quando `INSERT OR IGNORE` deduplica um evento. O schema de produção não usará esse padrão para a sequência auditável.

### Binding Rust embutido

Um segundo spike isolado validou:

```text
Rust: 1.88.0
rusqlite: 0.37.0
libsqlite3-sys: 0.35.0
SQLite bundled: 3.50.2
features: default-features=false, bundled
resultado: compilou e executou
dependência dinâmica libsqlite3: nenhuma
```

O spike abriu uma base, ativou WAL/FULL, criou tabela `STRICT`, iniciou transação `Immediate`, deduplicou por fingerprint e confirmou um único evento após replay.

## Matriz de decisão

| Critério | Journal com lock | SQLite/WAL |
|---|---|---|
| atomicidade multiobjeto | precisa de protocolo próprio | transação nativa |
| concorrência | lock global manual | um writer transacional, leitores concorrentes |
| crash antes do commit | pode deixar cauda parcial | rollback automático |
| deduplicação | índice próprio | constraint `UNIQUE` |
| evolução de schema | parser/migração próprios | `user_version` e migrations transacionais |
| consultas e índices | varredura ou índices duplicados | SQL e índices nativos |
| snapshot consistente | coordenação manual | read transaction |
| custo de implementação segura | alto | moderado |
| dependência adicional | nenhuma | `rusqlite` + SQLite bundled |
| binário único | sim | sim com feature `bundled` |
| MSRV 1.88 comprovado | sim | sim em `rusqlite 0.37.0` |

Os tempos não são um benchmark definitivo porque o journal executou uma seção crítica por evento e o SQLite agrupou eventos por transação de escritor. Eles demonstram custo e contenção, mas a escolha é sustentada principalmente por atomicidade, deduplicação e recuperação nativas.

## Configuração obrigatória

Cada banco de projeto deve aplicar e verificar:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
PRAGMA trusted_schema = OFF;
```

O serviço manterá uma fila de escrita por projeto. Escritores não ficarão bloqueando o wrapper Cargo: o cliente envia a observação, recebe confirmação limitada e o daemon serializa a transação. Leituras usarão transações consistentes e nunca serão feitas diretamente pelo agente.

## Sequência e deduplicação

Regras:

1. fingerprint possui constraint única no escopo `(project_id, event_kind, fingerprint)`;
2. o serviço verifica deduplicação dentro de `BEGIN IMMEDIATE`;
3. sequência auditável é atribuída somente quando um evento novo realmente será inserido;
4. replay retorna o resultado persistido, sem criar sequência vazia;
5. toda mutação canônica e seu evento de auditoria pertencem à mesma transação;
6. nenhuma resposta pública expõe rowid, path do banco ou schema interno.

## Schema inicial conceitual

O primeiro schema deverá separar:

- `schema_migrations`;
- `projects` e `workspaces`;
- `plans` e `tasks` canônicos;
- `submissions` e `reviews`;
- `compilation_observations` e `attestations`;
- `events` append-only;
- `projection_versions`;
- `capability_grants` e `revocations`, se o backend de capability escolhido usar persistência local.

Payloads complexos podem ser armazenados como JSON/BLOB versionado, mas campos usados em autorização, sequência, relacionamento ou lookup devem ser colunas tipadas e validadas.

## Projeções JSON

`guard/to-do_list.json` e `guard/governance_error.json` são derivados do banco:

1. serviço abre read transaction;
2. renderiza apenas o contrato permitido ao agente;
3. escreve arquivo temporário no mesmo diretório do destino;
4. aplica permissão prevista;
5. executa flush/sync conforme política;
6. renomeia atomicamente;
7. registra a versão da projeção sem transformar o arquivo em autoridade.

Editar ou apagar esses JSONs não altera SQLite. A próxima atualização válida os regenera.

## Recuperação e manutenção

- startup executa version check e quick/integrity check conforme condição de shutdown;
- migrations são transacionais, monotônicas e nunca executadas pelo cliente do agente;
- WAL checkpoint ocorre por política do daemon, não em cada chamada Cargo;
- backup usa snapshot consistente e destino protegido;
- restauração exige banco fechado ou fluxo explícito de restore, hash e evento administrativo;
- corrupção não aciona criação silenciosa de base vazia;
- arquivos `-wal` e `-shm` recebem a mesma proteção do banco principal.

## Segurança e limites

SQLite não é usado como mecanismo de autorização ou criptografia. A proteção vem do UID dedicado, DAC, sandbox, AppArmor e serviço limitado definidos na T1.2.

O banco, WAL, SHM, backups e migrations ficam exclusivamente sob `/var/lib/antdelirium`. O wrapper e o agente não abrem, testam nem recebem seus paths.

Extensões carregáveis, SQL fornecido por cliente e funções administrativas arbitrárias permanecerão desabilitados. Todas as queries são internas, parametrizadas e limitadas por capability antes de chegar ao storage.

## Política da dependência

`rusqlite 0.37.0` é o pin inicial porque foi comprovado no MSRV 1.88. A linha mais nova adota como política de MSRV a versão stable corrente no momento do release, portanto atualizações não serão automáticas.

Cada atualização exige:

- revisão de advisories e changelog;
- build/teste em Rust 1.88;
- versão SQLite embutida registrada;
- teste de reprodutibilidade do binário;
- matriz de concorrência, crash e migrations;
- atualização coordenada do wrapper `libres` e da allowlist.

## Consequências para as próximas tarefas

- T1.4 versiona schema canônico, submissão, auditoria e projeções separadamente;
- T2.1 introduz a abstração de state root sem permitir acesso pelo cliente;
- T2.3 implementa connection policy, migrations, transações, deduplicação e exportação atômica;
- o journal JSONL deixa de ser candidato a autoridade, mas eventos continuam logicamente append-only dentro da tabela `events`;
- JSON legado será importado por migration controlada, nunca consultado como segunda fonte de verdade.
