# Auditoria da definição de pronto — T10.5 concluída

## Resultado atual

A definição de pronto foi conferida item por item depois da matriz T10.4 e da
retirada T10.2/T10.3. Os vinte e quatro requisitos possuem evidência suficiente;
T10.5 e o gate final do produto estão concluídos.

Esta auditoria trata somente o AntDelirium. AutoBrowser e qualquer migração de
projeto externo estão fora do gate.

## Conferência item por item

| Requisito | Estado | Evidência ou pendência |
|---|---|---|
| único binário `antdelirium` | comprovado | T10.1 e `cargo metadata` expõem um único target executável |
| configuração em `$CARGO_HOME/config.toml`, não em projetos | comprovado | instalador Cargo T7.1–T7.3 e `doctor active-owned` |
| MSRV declarado e Cargo compatível | comprovado | `rust-version = 1.88`; Cargo/rustc 1.97.1 prontos, acima do piso Cargo 1.50 |
| delegação transparente ao `rustc` real | comprovado | matrizes T6.1–T6.6 e T10.4 |
| estado de projetos novos fora do repositório | comprovado | aceitação instalada T8G/T10.4 e serviço dedicado |
| nenhum binário/guard exigido em `plugins/` | comprovado | build hook, runtime de pass-through e templates compilados do guard antigo removidos em T10.2/T10.3 |
| somente duas projeções locais encontráveis | comprovado | `to-do_list.json` e `governance_error.json`; a aceitação instalada rejeita qualquer terceira entrada |
| projeções sem paths externos ou nomes administrativos | comprovado | T5.2–T5.7, T9.5 e T10.4 |
| agente sem leitura/escrita dos planos autoritativos | comprovado no host/sandbox testado | T9.4–T9.6; não é promessa contra o mesmo usuário sem sandbox |
| submissão não implementa, revisa ou aprova tarefa | comprovado | T8G.3 e ciclo instalado com authorities distintas |
| wrapper não abre/testa/enumera storage autoritativo | comprovado | T9.4 syscall/process matrix |
| ELF, argumentos, ambiente, syscalls e projeções sem locator protegido | comprovado | T9.3/T9.4 e release nondisclosure |
| agente não anexa debugger nem inspeciona o serviço | comprovado no host instalado | T9.4 com `ptrace` e `/proc` |
| nenhum `login.json` ou log previsível em `/tmp` | comprovado | T5.7 e matrizes de workspace |
| comandos Cargo permanecem comuns | comprovado | T7.6 e T10.4: check/build/test/run/clippy |
| proxy `cargo` do rustup preservado | comprovado | instalador, rollback e matriz T7.6 |
| targets suportados continuam compilando | comprovado | native e WASM, build script, proc-macro, exemplos e testes |
| concorrência atômica e recuperável | comprovado | storage transacional, builds paralelos e rollback instalado |
| agente recebe ações de código, não edição de JSON | comprovado | API limitada e projeções com ação/evidência |
| documentação não promete proteção contornável pelo mesmo usuário | comprovado | limites locais/host/CI declarados e não objetivos mantidos |
| autoridade local contra bypass intencional | comprovado | T9.7 exige decisão protegida atual, assinada e sem violações antes da transição canônica; projeção local editada não concede autoridade |
| backup Git automático por ciclo de tarefa | comprovado | T9.8 mantém Git bare privado com commits de baseline, submissão, revisão e avanço, contexto derivado dos commits e recuperação reentrante, sem consultar `.git` do projeto |
| implantação genérica com rollback sem projeto externo | comprovado | T8G.1–T8G.4, storage-v13, T10.4 e aceitação instalada final |
| toda a suíte do AntDelirium verde no estado final | comprovado | `make ci`: 183 testes da biblioteca, 190 do binário, 9 de integração e auditoria ELF passaram sem falhas; 8 testes de migração externa permaneceram ignorados por serem não objetivos |

## Dependências restantes

Nenhuma dependência do gate do produto permanece aberta. Migrações de projetos
externos e configuração de branch protection no provedor continuam fora do
escopo do AntDelirium.

## Não objetivos preservados

O produto não reimplementa `rustc`, não depende de obscuridade, não promete
impedir engenharia reversa pelo mesmo usuário, não substitui o proxy `cargo`,
não usa o backup Git privado como workspace do agente, não expõe sua localização
e não transforma JSON editável em autoridade. Revisão separada e controle de acesso do sistema operacional
continuam autoridades necessárias; CI/branch protection é integração opcional.
