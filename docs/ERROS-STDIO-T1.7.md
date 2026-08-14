# Códigos de erro e contrato de stdout/stderr

Tarefa: T1.7  
Versão da decisão: `errors-v1`  
Schema público: `public-error-v1`

## Resultado

O AntDelirium passa a distinguir condições próprias por código estável, categoria e exit code. O wrapper nunca mistura diagnóstico próprio ao stdout do compilador e nunca substitui uma falha real do `rustc` por uma falha de governança.

## Exit codes próprios

| Código | Categoria | Exit | Retry | Uso |
|---|---|---:|:---:|---|
| `AD-USAGE-0001` | `usage_error` | 64 | não | argumento ou combinação inválida |
| `AD-CODE-0001` | `code_violation` | 65 | não | política de código bloqueante no projeto primário |
| `AD-SERVICE-0001` | `service_unavailable` | 69 | sim | daemon/canal limitado indisponível no modo protegido |
| `AD-INTERNAL-0001` | `tool_internal_error` | 70 | não | bug, panic capturado ou invariante interna quebrada |
| `AD-TEMP-0001` | `temporary_failure` | 75 | sim | contenção, deadline ou recurso temporário |
| `AD-PROTOCOL-0001` | `protocol_error` | 76 | não | framing, versão ou envelope incompatível |
| `AD-AUTH-0001` | `authorization_error` | 77 | não | capability ausente, revogada, expirada ou fora de escopo |
| `AD-CONFIG-0001` | `governance_configuration_error` | 78 | não | configuração protegida inválida ou inconsistente |
| `AD-TOOLCHAIN-0001` | `toolchain_execution_error` | 126 | não | compilador encontrado, mas não executável |
| `AD-TOOLCHAIN-0002` | `toolchain_execution_error` | 127 | não | compilador real não encontrado |

Os valores 64–78 seguem a faixa tradicional de erros de infraestrutura/uso e ficam separados do exit `1` normalmente produzido pelo compilador. Ainda assim, o prefixo `AD-*` é a identificação autoritativa da origem; um processo filho pode retornar qualquer byte.

## Precedência no wrapper

O resultado final é escolhido nesta ordem:

1. falha ao iniciar o compilador real retorna 126 ou 127 com código `AD-TOOLCHAIN` sanitizado;
2. se o compilador inicia e termina por exit não zero, o wrapper retorna exatamente esse exit;
3. se o compilador termina por sinal, o wrapper reproduz o sinal; fallback documentado é `128 + signal`;
4. somente quando o compilador termina com sucesso uma condição bloqueante do AntDelirium pode escolher 65, 69, 70, 75, 76, 77 ou 78;
5. sucesso do compilador e governança retorna 0.

O wrapper não converte erro Rust em `AD-CODE-0001`. Violação do compilador e violação de governança são resultados diferentes.

## Audit-only e enforcement

No rollout audit-only:

- código-fonte com violações atualiza `guard/governance_error.json`, emite no máximo um aviso `AD-GOV` e preserva o exit do `rustc`;
- indisponibilidade ou falha interna registra degradação e preserva o exit do `rustc`;
- nenhuma dependência externa é bloqueada pela governança do projeto.

No modo protegido:

- projeto primário com violação bloqueante usa exit 65 depois de `rustc` bem-sucedido;
- ausência de serviço/capability/configuração válida falha fechado depois de `rustc` bem-sucedido;
- probes, consultas do toolchain e dependências não primárias apenas delegam;
- CI pode exigir participação do wrapper independentemente do resultado local.

A política que classifica quais regras são bloqueantes ainda pertence ao rollout da Onda 9; esta tarefa define como comunicar o resultado, não quais regras bloqueiam.

## Contrato por modo

| Modo | stdout | stderr |
|---|---|---|
| wrapper Cargo | bytes do `rustc`, sem prefixos AntDelirium | bytes do `rustc` e, no máximo uma vez por compilação primária, uma linha sanitizada AntDelirium |
| CLI humana com sucesso | resultado solicitado | vazio |
| CLI humana com erro | vazio | uma linha pública com código |
| CLI `--json` com sucesso | um objeto JSON e newline | vazio |
| CLI `--json` com erro conhecido | um `public-error-v1` e newline | vazio |
| daemon | nenhum terminal como API | logs internos somente no journal protegido |
| operador autorizado | resultado solicitado; paths apenas quando a operação privada os exigir | erro público, com diagnóstico detalhado no canal/storage privado |

O modo wrapper nunca imprime `cargo:warning=`: esse protocolo pertence a build scripts e poluiria a saída do compilador/wrapper. Em builds paralelos, o serviço concede um lease de emissão somente ao evento primário deduplicado; os demais processos ficam silenciosos.

## Mensagens públicas normativas

```text
antdelirium[AD-TASK]: leia guard/to-do_list.json
antdelirium[AD-GOV]: leia guard/governance_error.json
antdelirium[AD-CODE-0001]: governança bloqueou a compilação; leia guard/governance_error.json
antdelirium[AD-CONFIG-0001]: configuração de governança inválida; não altere arquivos externos ao projeto; solicite o operador
antdelirium[AD-SERVICE-0001]: serviço de governança indisponível; tente novamente
antdelirium[AD-INTERNAL-0001]: falha interna de governança; informe a referência <ref_opaca>
antdelirium[AD-TEMP-0001]: governança temporariamente ocupada; tente novamente
antdelirium[AD-PROTOCOL-0001]: cliente e serviço de governança são incompatíveis; solicite o operador
antdelirium[AD-AUTH-0001]: operação não autorizada para esta sessão
antdelirium[AD-USAGE-0001]: invocação inválida; consulte a ajuda disponível para esta sessão
antdelirium[AD-TOOLCHAIN-0001]: não foi possível executar o compilador configurado
antdelirium[AD-TOOLCHAIN-0002]: compilador configurado não foi encontrado
```

`AD-TASK` e `AD-GOV` são notificações, não códigos de falha. Elas não mudam exit status.

## Dados proibidos em erros públicos

Nenhum erro público pode incluir:

- state root, banco, WAL, socket administrativo ou raiz legada;
- path de plano, TOML, configuração protegida, executable real ou capability;
- SQL, tabela, migration, backtrace, source line interna ou erro bruto do sistema;
- lista de operações, projetos, tarefas ou capabilities existentes;
- UID/GID/PID do serviço ou operador;
- sugestão de editar JSON administrativo, desabilitar regra, trocar wrapper ou procurar plugins.

Paths de código só aparecem relativos ao projeto e preferencialmente dentro das duas projeções. Erros de autorização para ID inexistente, fora do escopo, revogado ou expirado têm o mesmo código e a mesma mensagem.

## Correlação e diagnóstico privado

Falhas internas podem retornar `correlation_id` aleatório `ref_<26 caracteres>`. A referência não codifica horário, PID, sequência, path ou projeto. Somente o operador autorizado resolve essa referência no journal protegido.

Panic no daemon ou cliente é capturado na fronteira superior em release. O agente recebe `AD-INTERNAL-0001`; payload, backtrace e causa permanecem privados. `RUST_BACKTRACE` fornecido pelo agente não força exposição em modo protegido.

## JSON público

`public-error-v1` contém apenas:

- versão;
- código e categoria estáveis;
- exit code recomendado;
- `retryable`;
- mensagem pública;
- ação limitada;
- correlação opaca opcional.

Ações possíveis são `read_task_projection`, `read_governance_projection`, `retry`, `fix_invocation`, `contact_operator` e `none`. O único path projetável é `guard/to-do_list.json` ou `guard/governance_error.json`.

No wire, o serviço devolve código, categoria, retry, mensagem, ação e correlação. O cliente valida essa enumeração e adiciona o exit code definido localmente por `errors-v1`; um valor livre ou uma combinação desconhecida vira `AD-PROTOCOL-0001`.

## Estado atual que deverá ser removido

O inventário do binário atual encontrou:

- `elite_out` envia inclusive erros para stdout;
- uma busca inexistente imprime `[!] Path or Parameter not found` em stdout e retorna 0;
- `config new_task` inválido imprime em stdout e retorna 1;
- violações usam exit 1 genérico;
- erros brutos são interpolados em mensagens;
- `--version` expõe o path usado para iniciar o binário;
- os logs globais previsíveis do runtime foram removidos na T5.7 e o gate de release impede sua reintrodução;
- o build guard antigo usa panic e localização de plugin.

Esses comportamentos são baseline de migração, não exceções aceitas. T3.6 centraliza `OutputRouter`/`PublicError`; T5.7 elimina logs globais e T6 aplica o contrato no wrapper.

## Testes obrigatórios derivados

1. stdout do wrapper é idêntico ao do compilador real em sucesso e erro;
2. exit/sinal do compilador é preservado;
3. cada erro próprio mapeia ao exit da tabela;
4. `--json` nunca mistura texto humano;
5. autorização não funciona como oráculo;
6. nenhum erro público contém path absoluto, token, SQL ou backtrace;
7. build paralelo emite no máximo uma notificação AntDelirium;
8. audit-only nunca transforma uma violação em falha;
9. protected mode falha fechado somente para o projeto primário e depois de preservar falha anterior do compilador.
