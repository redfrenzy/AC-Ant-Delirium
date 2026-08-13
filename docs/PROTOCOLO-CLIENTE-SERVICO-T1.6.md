# Protocolo cliente-serviço e separação de canais

Tarefa: T1.6  
Versão da decisão: `protocol-v1`  
Wire limitado: `adp/1`  
Wire administrativo: `ada/1`

## Resultado

A implantação Linux usa dois sockets Unix `SOCK_STREAM` ativados pelo systemd, com listeners, magic bytes, schemas, parsers e conjuntos de operações separados.

| Canal | Magic | Schema | Autoridade máxima |
|---|---|---|---|
| limitado | `ADL1` | `limited-protocol-v1` | bootstrap provisório, compilação, projeção, tarefa, submissão, revisão atribuída e attestation |
| administrativo | `ADA1` | `admin-protocol-v1` | instalação, migração, rebind, capabilities, diagnóstico privado e transição administrativa |

Não existe operação `admin`, flag de perfil ou campo `channel` capaz de elevar uma conexão limitada. O listener escolhe o parser antes de ler JSON. Um frame com magic do outro canal é descartado.

## Transporte

- AF_UNIX de filesystem; sockets abstratos foram rejeitados porque não oferecem a fronteira de permissões do pathname;
- `SOCK_STREAM` com framing explícito;
- socket activation do systemd, `Accept=no`;
- listeners entregues ao daemon com nomes `agent` e `admin`;
- nenhuma interface TCP, HTTP, D-Bus ou rede em `v1`;
- nenhum `SCM_RIGHTS`, descriptor de state root ou descriptor administrativo aceito no canal limitado.

O endpoint limitado pode ser conhecido pelo cliente: encontrá-lo não revela planos e não concede autoridade. O path administrativo fica em configuração/unidade root-owned ou chega como descriptor já aberto ao launcher do operador; não é retornado por ajuda, erro ou protocolo limitado.

## Framing

Cada frame é:

```text
offset  tamanho  conteúdo
0       4        ADL1 ou ADA1
4       4        comprimento unsigned big-endian do JSON
8       N        JSON UTF-8 estrito
```

Regras:

1. limite de 4 MiB por frame em `v1`;
2. comprimento é validado antes de alocação;
3. sem compressão, trailing bytes, JSON concatenado ou framing por newline;
4. chaves duplicadas, UTF-8 inválido, nesting excessivo e campos desconhecidos são rejeitados;
5. leitura e escrita têm deadline;
6. uma conexão processa um hello, uma requisição e uma resposta; não há pipelining;
7. accepted sockets e descriptors do systemd recebem `FD_CLOEXEC` assim que apropriado.

Uma resposta grande não deve carregar plano, histórico ou artefato bruto. Projeções respeitam o contrato `agent-projection-v1`; evidência grande é representada por digest.

## Handshake

Fluxo:

```text
accept
  -> serviço captura SO_PEERCRED
  <- client_hello com versões suportadas e nonce do cliente
  -> server_hello com versão escolhida, nonce da conexão e limites
  <- request vinculada ao nonce
  -> response e close
```

O cliente oferece versões, mas não escolhe unilateralmente. `v1` exige interseção exata com `adp/1` ou `ada/1`; não há downgrade silencioso. O hello nunca lista operações, projetos, capabilities ou paths.

## Autenticação cumulativa

Uma requisição normal só é autorizada quando as três camadas concordam:

1. **DAC do socket:** o kernel permite a conexão ao pathname conforme diretório, owner, grupo e mode;
2. **peer do kernel:** o daemon lê `SO_PEERCRED` imediatamente após `accept` e fixa PID, UID e GID daquela conexão;
3. **capability:** token aleatório de 256 bits, vinculado a subject UID, ator, operações, projeto, workspace, tarefa opcional, validade e epoch de revogação.

PID é evidência da conexão, não identidade persistente: pode ser reciclado depois que a conexão fecha. GID retornado por `SO_PEERCRED` não substitui a checagem que o kernel já fez ao conectar no socket. Nenhum UID/GID/PID enviado no JSON é considerado.

O serviço armazena apenas hash do segredo da capability e compara em tempo constante. Token, nonce e material de capability nunca entram em journal, erro, projeção ou log. Possuir uma capability limitada não autoriza o socket administrativo.

### Entrega de capability

No modo protegido, o supervisor entrega a capability limitada por descriptor herdado, não por argv, cwd, arquivo do projeto ou variável contendo o segredo. Uma variável pode no máximo indicar o número do descriptor. O wrapper lê o segredo, usa-o no protocolo e fecha o descriptor antes de delegar ao `rustc`.

O agente pode observar a própria capability limitada; isso é esperado. Ela continua presa ao seu UID e escopo e não contém poder de aprovação ou administração. Capability administrativa é entregue somente por launcher privilegiado ou sessão de operador e nunca é herdada por Cargo.

## Replay, retry e concorrência

- servidor gera `connection_nonce` de 256 bits para cada conexão;
- cliente gera `request_id` aleatório `req_<26 caracteres>`;
- request inclui o nonce recebido e uma capability válida;
- o serviço persiste unicidade de `(capability_id, request_id)`;
- retry com mesmo ID e mesmo fingerprint retorna o resultado persistido;
- mesmo ID com body, operação ou escopo diferente é rejeitado e auditado;
- nonce só vale na conexão que o emitiu;
- mutação e registro do request/evento ocorrem na mesma transação SQLite;
- expiração ou revogação é verificada antes de resolver qualquer recurso.

Esse desenho impede que a repetição do frame aplique a mutação duas vezes. Não pretende esconder do próprio agente a capability limitada que ele recebeu.

## Envelope limitado

Operações reconhecidas pelo parser limitado:

- `project.bootstrap.observe`;
- `compile.observe`;
- `projection.refresh`;
- `task.current.read`;
- `submission.create`;
- `submission.read.assigned`;
- `review.decide.assigned`;
- `attestation.create`.

Cada variante terá um body Rust próprio. Não haverá `serde_json::Value` genérico encaminhado ao domínio, método arbitrário, SQL, path de state root, argv administrativo ou chave `implemented=true`.

`--api` do perfil agente traduz apenas comandos públicos para essas variantes. Ele não envia strings de query legadas diretamente ao daemon.

### Bootstrap automático e seguro

`cargo check` em projeto ainda desconhecido precisa criar dados sem comando novo. Por isso `project.bootstrap.observe` é a única requisição aceita sem capability preexistente, mas exige acesso ao socket limitado e produz apenas registro **provisório**:

- cria projeto/workspace vazio, sujeito a quota e deduplicação;
- não importa plano, tarefa, TOML, histórico ou aprovação;
- não associa um `project_ref` copiado a estado já existente;
- emite capability bootstrap curta somente para `compile.observe` e `projection.refresh` daquele registro;
- não pode promover o projeto a governança protegida.

No modo protegido, o supervisor valida Git/filesystem/manifest, promove o registro e entrega capability vinculada. O daemon de runtime continua sem montar ou percorrer o projeto. Assim, o bootstrap automático existe sem transformar observações do cliente não confiável em autoridade.

## Envelope administrativo

O parser administrativo reconhece somente variantes administrativas tipadas, incluindo:

- `project.enroll`, `project.rebind` e `project.retire`;
- `migration.stage`, `migration.commit` e `migration.rollback`;
- `capability.grant` e `capability.revoke`;
- `state.transition` e overrides auditados;
- `diagnostic.private` e manutenção do storage;
- preparação/validação de instalação.

A conexão requer simultaneamente acesso ao socket administrativo, peer permitido e capability administrativa. Root sem capability pode executar apenas bootstrap local explicitamente previsto pela instalação; isso gera auditoria e não é acessível no listener limitado.

O schema administrativo não é empacotado nas projeções nem na ajuda do agente. Mesmo que nomes sejam descobertos por engenharia reversa do artefato único, não há rota do listener limitado para esses handlers.

## Revisores e CI

Revisor e CI usam o canal limitado porque suas operações também são estreitas, mas recebem capabilities próprias:

- revisor só pode decidir submissão atribuída e não muda plano diretamente;
- CI só pode criar attestation no escopo do input verificado;
- capability de agente não pode invocar essas variantes;
- resposta de autorização não confirma se tarefa, submissão ou projeto fora do escopo existe.

## Socket activation

Baseline das unidades, sem tornar paths compilados no cliente:

```ini
# canal limitado
[Socket]
ListenStream=/run/antdelirium/agent.sock
FileDescriptorName=agent
SocketUser=antdelirium
SocketGroup=antdelirium-client
SocketMode=0660
DirectoryMode=0755
Accept=no

# canal administrativo, em outra unidade
[Socket]
ListenStream=<endpoint definido pela instalação root-owned>
FileDescriptorName=admin
SocketUser=root
SocketGroup=antdelirium-admin
SocketMode=0660
DirectoryMode=0750
Accept=no
```

O daemon aceita somente descriptors nomeados esperados e falha fechado diante de descriptor extra, ausente, duplicado ou com tipo diferente. Não faz bind alternativo silencioso quando iniciado como serviço protegido.

## Erros e ausência de oráculo

O envelope reserva `status`, `error.code`, `retryable` e uma mensagem pública limitada. A taxonomia definitiva e a tradução para stdout/stderr pertencem à T1.7.

Antes da autorização, erros de projeto, tarefa, capability revogada, ID inexistente e escopo incorreto usam resposta indistinguível. Diagnóstico com paths e causa interna só existe no canal administrativo.

## Limites e decisões rejeitadas

- `SO_PEERCRED` sozinho: vários agentes/wrappers podem compartilhar UID;
- token sozinho: não vincula a conexão ao peer autorizado;
- um socket com campo `admin=true`: cria risco de confused deputy;
- socket abstrato: não oferece owner/group/mode do pathname;
- confiar no GID declarado no JSON: é input do atacante;
- usar PID como identidade longa: PIDs são recicláveis;
- colocar token em argv, projeto ou log: expõe o segredo por superfícies desnecessárias;
- RPC genérico com strings: amplia enumeração e bypass do roteamento tipado;
- TLS no socket local `v1`: não substitui DAC, peer credentials e capabilities; transporte remoto será outro protocolo;
- permitir ao bootstrap provisório importar ou reencontrar plano protegido: transforma claim não confiável em autoridade.

## Evidência local e fontes

No host atual, um probe fora do sandbox confirmou `SO_PEERCRED` sobre `socketpair(AF_UNIX, SOCK_STREAM)` retornando PID, UID e GID reais; o framing `magic + u32 big-endian + JSON` também foi exercitado. O sandbox do agente bloqueou deliberadamente esse getsockopt, portanto o teste foi repetido somente após autorização.

A especificação se apoia no comportamento documentado de [AF_UNIX e `SO_PEERCRED`](https://man7.org/linux/man-pages/man7/unix.7.html) e nas opções de [socket activation, owners, modes e descriptors do systemd](https://man7.org/linux/man-pages/man5/systemd.socket.5.html).

## Consequências para implementação

- T3.2 escolhe perfil antes de qualquer parser legado.
- T3.4 implementa apenas o mapeamento CLI limitado.
- T4.1 estabelece a autoridade distinta do modo de serviço.
- T4.2 materializa registry e storage canônico sob essa autoridade.
- T4.3 implementa os dois listeners e os parsers de canal separados, ainda fail-closed.
- T4.4 captura peer credentials e implementa o motor de capability/nonce/replay; a leitura dos frames permanece fechada até a ligação dos handlers tipados.
- T4.5 resolve IDs opacos somente a partir do token autorizado dentro do serviço; nenhum frame é lido até a ligação posterior dos handlers, e a matriz de capability impede que o canal limitado produza autorização administrativa.
- T4.6 registra submissões e revisões junto de seus eventos na mesma transação e mantém, no registry, o journal tipado de negações e rejeições sem payload ou localização física; os listeners continuam sem ler frames até a ligação posterior dos handlers.
- T6 fecha descriptors antes de delegar ao compilador.
