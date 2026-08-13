# Decisão de autoridade e isolamento do serviço

Versão: `authority-v1`  
Tarefa: T1.2  
Plataforma inicial: Linux com systemd

## Decisão

O enforcement local será dividido entre um cliente não confiável e um serviço protegido:

- o executável chamado pelo Cargo roda com a mesma autoridade limitada do agente e nunca acessa o estado canônico;
- o daemon `antdeliriumd` roda sob o usuário de sistema dedicado `antdelirium`;
- o estado canônico fica em `/var/lib/antdelirium`, pertencente exclusivamente ao serviço;
- agentes acessam somente um socket Unix de protocolo limitado;
- operadores usam uma superfície administrativa separada, não concedida ao grupo dos agentes;
- CI protegido permanece a autoridade externa capaz de detectar bypass do cliente local.

O executável físico continuará sendo produzido como um único binário `antdelirium`. O mesmo artefato poderá entrar em modo cliente, wrapper, serviço ou operador conforme argv inicial e capability autenticada. Compartilhar o artefato não compartilha autoridade.

## Estado observado nesta máquina

Captura de 2026-08-11:

- systemd 255;
- kernel Linux 7.0 x86-64;
- AppArmor e seccomp disponíveis no build do systemd;
- `kernel.yama.ptrace_scope = 1`;
- `/proc` sem `hidepid=2` no namespace atual;
- usuário e grupos `antdelirium` ainda inexistentes;
- usuário interativo `dracker` não possui grupo administrativo adicional.

Logo, esta tarefa fecha a decisão, mas não declara a implantação como ativa. A criação de contas, unidades, diretórios e políticas ocorrerá somente na fase de instalação autorizada.

## Contas e grupos

| Identidade | Finalidade | Login | Acesso ao estado canônico |
|---|---|---:|---:|
| `antdelirium` | executar o daemon | não | leitura e escrita |
| `antdelirium-client` | conectar ao socket limitado | grupo, sem usuário próprio | nenhum |
| `antdelirium-admin` | conectar à superfície administrativa | grupo concedido pelo operador | indireto via serviço |
| usuário do agente | editar somente o projeto atribuído | existente | nenhum |

O usuário `antdelirium` será uma conta de sistema sem shell, senha ou home interativo. Ele não pertencerá ao grupo do agente. O usuário do agente poderá pertencer a `antdelirium-client`, nunca a `antdelirium` ou `antdelirium-admin`.

## Layout protegido

```text
/usr/libexec/antdelirium/antdelirium     root:root 0755
/var/lib/antdelirium/                    antdelirium:antdelirium 0700
/var/lib/antdelirium/registry/
/var/lib/antdelirium/projects/
/var/lib/antdelirium/events/
/var/lib/antdelirium/keys/
/run/antdelirium/agent.sock              antdelirium:antdelirium-client 0660
/run/antdelirium-admin/admin.sock         root:antdelirium-admin 0660
```

O binário em `$CARGO_HOME/bin/antdelirium` será um ponto de entrada ou link para o artefato instalado. Ele é tratado como cliente substituível e não contém chaves nem ganha acesso ao state root. Se o agente o alterar ou contornar, não obtém autoridade canônica; o CI e o serviço registram ou detectam a ausência esperada.

`$CARGO_HOME/antdelirium` não armazenará planos nem estado autoritativo no modo protegido. No máximo poderá conter cache não sensível, diagnóstico local ou ponteiro público, todos descartáveis. Usá-lo como state root canônico colocaria os planos sob o mesmo UID do agente e violaria o contrato `actors-v1`.

## Sockets e protocolo

### Socket limitado

O socket `/run/antdelirium/agent.sock` aceita somente capabilities limitadas:

- `project.bootstrap.observe` em estado provisório e sem plano;
- `compile.observe`;
- `projection.refresh`;
- `task.current.read`;
- `submission.create`;
- `submission.read.assigned`;
- `review.decide.assigned` para peer/capability de revisor;
- `attestation.create` para peer/capability de CI.

O serviço valida `SO_PEERCRED`, escopo do projeto/workspace e capability do envelope. O bootstrap sem capability só cria registro provisório vazio e sujeito a quota; ele não importa nem reencontra plano protegido. Conhecer o caminho do socket ou reproduzir uma mensagem não concede outra capability.

### Socket administrativo

O socket administrativo fica em um diretório separado sem travessia para o agente. Ele aceita capabilities de operador e não aparece no help do perfil de agente. O daemon recebe ambos por socket activation; o processo cliente nunca herda descritor administrativo.

A T1.6 definirá formato, versionamento, autenticação, replay protection e binding entre peer e capability. Esta decisão fixa desde já que não haverá multiplexação de comandos administrativos no socket público.

## Unidade de serviço

A implantação deverá usar uma unidade de sistema, não uma unidade `--user`, porque uma unidade do mesmo usuário do agente não cria fronteira de DAC.

Baseline de hardening obrigatória:

```ini
[Service]
User=antdelirium
Group=antdelirium
NoNewPrivileges=yes
PrivateTmp=yes
PrivateDevices=yes
PrivateNetwork=yes
ProtectSystem=strict
ProtectHome=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectKernelLogs=yes
ProtectControlGroups=yes
ProtectClock=yes
ProtectHostname=yes
ProtectProc=invisible
ProcSubset=pid
RestrictAddressFamilies=AF_UNIX
RestrictSUIDSGID=yes
LockPersonality=yes
MemoryDenyWriteExecute=yes
CapabilityBoundingSet=
AmbientCapabilities=
SystemCallArchitectures=native
StateDirectory=antdelirium
StateDirectoryMode=0700
RuntimeDirectory=antdelirium
RuntimeDirectoryMode=0755
```

`PrivateNetwork=yes` é compatível com sockets Unix entregues por socket activation e impede que o daemon abra conexões de rede comuns. A lista final de syscalls será derivada por teste; não será adotado um filtro que quebre Rust, alocação, filesystem ou locking silenciosamente.

O daemon normal não recebe bind mount de projetos em `/home`. Ele processa estado canônico e solicitações sanitizadas. Importação de planos legados é uma operação administrativa separada, com origem explícita e staging; o daemon de runtime não percorre a árvore histórica de plugins.

## `ptrace`, debugger e `/proc`

A fronteira usa camadas cumulativas:

1. UID do daemon diferente do UID do agente;
2. ausência de `CAP_SYS_PTRACE` em ambos;
3. `ptrace_scope=1` ou mais restritivo;
4. daemon não dumpable após carregar qualquer segredo;
5. agente em namespace PID/proc próprio, com `/proc` montado usando ocultação equivalente a `hidepid=2`;
6. perfil AppArmor do agente negando `/var/lib/antdelirium/**`, sockets administrativos e raízes legadas protegidas;
7. perfil AppArmor do serviço permitindo somente binário, state root, sockets e recursos de runtime declarados.

`ProtectProc=invisible` protege a visão de `/proc` dentro do serviço, mas não esconde sozinho o daemon do host. Por isso a implantação do agente precisa de PID namespace ou procfs restrito. Apenas `ptrace_scope=1`, sem UID separado, também não seria suficiente.

### Estado implementado na T4.7

O bootstrap do mesmo binário agora aplica a parte que precisa ser garantida pelo
próprio daemon antes de abrir storage ou listeners: `PR_SET_NO_NEW_PRIVS=1`,
`PR_SET_DUMPABLE=0`, limites de core em zero, `umask 0077`, recusa de um
`TracerPid` já anexado e `CLOSE_RANGE_CLOEXEC` em todos os descritores herdados
acima de stderr. Listeners e conexões aceitas continuam recebendo verificação
individual de `FD_CLOEXEC`. Um token Rust não construível fora do módulo torna
essa ordem obrigatória para iniciar os canais.

O template da unidade reforça essa política com `LimitCORE=0`, `UMask=0077`,
`RestrictNamespaces=yes`, `RestrictRealtime=yes`, `KeyringMode=private`,
`ProtectProc=invisible` e `ProcSubset=pid`. Isso reduz exposição e falha fechado,
mas não substitui o sandbox do cliente. A T4.8 exercita essa fronteira em
namespaces reais e descartáveis; a T9.4 repetirá a matriz contra a conta, unidade
e sockets instalados.

## Sandbox do agente

O sandbox do agente permite:

- leitura e escrita no projeto atribuído;
- execução de Cargo/toolchain;
- acesso ao socket limitado;
- leitura dos dois arquivos em `guard/`.

Ele nega:

- `/var/lib/antdelirium/**`;
- `/run/antdelirium-admin/**`;
- `$CARGO_HOME` fora das entradas estritamente necessárias ao toolchain;
- raízes legadas que contenham planos e orquestrador;
- attach ou inspeção de processos fora do namespace;
- montagem, mudança de namespace, BPF, perf e interfaces de kernel que permitam contornar o isolamento.

O sandbox é parte do enforcement. Executar o agente diretamente como `dracker` no host, sem essa contenção, permanece modo de desenvolvimento e não satisfaz o gate G9.

## Migração dos planos atuais

Os planos atuais permanecem intocados até a fase de migração. O fluxo futuro será:

1. operador identifica a raiz legada explicitamente;
2. copia para staging protegido sem remover a origem;
3. compara contagens, hashes, schema e histórico;
4. importa para `/var/lib/antdelirium/projects/<project-key>/`;
5. ativa o serviço sobre a cópia validada;
6. executa testes de projeção e rollback;
7. só então retira a raiz legada do alcance do sandbox do agente.

O runtime nunca retorna a origem legada, o destino físico ou relações entre IDs opacos e paths.

## CI autoritativo

O CI não compartilha credenciais do serviço local. Ele usa sua própria autoridade protegida para:

- validar uma compilação primária com target limpo;
- confirmar a participação e identidade do wrapper;
- verificar integridade das projeções e do protocolo;
- emitir attestation assinada;
- bloquear integração quando o cliente local tiver sido removido ou substituído.

Uma attestation de CI é evidência para o serviço ou revisor; não concede ao agente mutação direta do plano.

## Decisões rejeitadas

- **daemon sob o usuário `dracker`:** permite leitura, escrita e tracing pelo mesmo domínio do agente;
- **unidade systemd `--user`:** melhora lifecycle, mas não cria separação de autoridade;
- **somente arquivos readonly:** o mesmo usuário pode restaurar permissões ou substituir pais;
- **state root em `$CARGO_HOME/antdelirium`:** fica acessível ao agente e não protege os planos;
- **apenas ocultar nomes e paths:** strings, syscalls e `/proc` continuam revelando o fluxo;
- **um único socket com comandos escondidos:** engenharia reversa encontra comandos e amplia o impacto de falhas de roteamento;
- **confiar apenas no wrapper local:** Cargo permite override e chamadas diretas; CI/supervisor continuam necessários.

## Critérios de validação futura

1. agente conecta ao socket limitado e recebe somente projeção sanitizada;
2. agente recebe `EACCES` ao tentar state root, socket administrativo e raiz legada;
3. `strace -p` e debugger contra o daemon são recusados;
4. `/proc` do sandbox não revela ambiente, argumentos ou descritores do daemon;
5. daemon não possui rede, capabilities Linux ou acesso a home/projeto;
6. alteração do cliente Cargo não muda estado canônico;
7. operação administrativa exige peer/capability administrativa real;
8. rollback restaura estado validado sem depender da origem legada.
