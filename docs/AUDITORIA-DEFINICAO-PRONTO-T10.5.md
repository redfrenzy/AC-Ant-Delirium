# Auditoria da definição de pronto — T10.5 parcial

## Resultado atual

A definição de pronto foi conferida item por item depois da matriz T10.4. Vinte
e um de vinte e quatro requisitos possuem evidência suficiente; três permanecem abertos. Por
isso T10.5 e o gate final do produto não estão concluídos.

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
| nenhum binário/guard exigido em `plugins/` | comprovado no fluxo atual | build hook ativo removido; templates compilados de rollback ainda saem em T10.2/T10.3 |
| somente duas projeções locais encontráveis | comprovado | `to-do_list.json` e `governance_error.json`; `.gitignore` é política Git, não projeção |
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
| autoridade local contra bypass intencional | **pendente** | política, assinatura e autoridade estão instaladas, mas a transição canônica ainda não exige auditoria atual e sem violações |
| checkpoint automático por tarefa aceita | **pendente** | transações, journals e checkpoint Ed25519 existem; ainda faltam gatilho automático, vínculo com fingerprint do código e paridade de recuperação do commit legado |
| implantação genérica com rollback sem projeto externo | comprovado | T8G.1–T8G.4, storage-v11 e T10.4 |
| toda a suíte do AntDelirium verde no estado final | **pendente** | testes focados e matrizes T10.4 passaram; a suíte integral final deve rodar depois de T10.2/T10.3 para validar o produto sem rollback compilado |

## Dependências restantes

1. integrar a decisão assinada à transição canônica local e concluir T9.7;
2. substituir e validar o commit automático legado na T9.8;
3. criar a referência recuperável do release e executar a remoção inventariada
   em T10.2;
4. retirar mensagens/templates exclusivos do rollback e concluir T10.3;
5. executar `make ci`, a matriz final e a auditoria ELF no estado definitivo;
6. atualizar esta auditoria, marcar T10.5 e fechar o gate final somente se tudo
   permanecer verde.

## Não objetivos preservados

O produto não reimplementa `rustc`, não depende de obscuridade, não promete
impedir engenharia reversa pelo mesmo usuário, não substitui o proxy `cargo`,
não copia repositórios para o Cargo home e não transforma JSON editável em
autoridade. Revisão separada e controle de acesso do sistema operacional
continuam autoridades necessárias; CI/branch protection é integração opcional.
