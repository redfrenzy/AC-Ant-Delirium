# Validação do endpoint Cargo automático — T8.6 parcial

Data: 2026-08-20

## Objetivo

Fechar a lacuna entre instalar `antdelirium` como
`rustc-workspace-wrapper` e fazer os comandos Cargo existentes alcançarem o
canal limitado sem depender de o agente exportar uma variável ou aprender um
novo comando.

Esta etapa modifica somente o repositório AC-Ant-Delirium e usa Cargo homes
descartáveis sob `target/`. Ela não instala no Cargo home real, não cria contas,
não ativa unidades systemd e não altera nenhum projeto piloto.

## Contrato implementado

O operador fornece `ANTDELIRIUM_LIMITED_SOCKET` somente ao executar o instalador.
O valor precisa ser um pathname absoluto e lexicalmente normalizado. A transação
existente passa a persistir exatamente estes dois campos em
`$CARGO_HOME/config.toml`:

```toml
[build]
rustc-workspace-wrapper = "<CARGO_HOME>/bin/antdelirium"

[env]
ANTDELIRIUM_LIMITED_SOCKET = { value = "<ENDPOINT_LIMITADO>", force = true }
```

O endpoint limitado pode ser conhecido pelo cliente e não concede capability.
O endpoint administrativo, o state root e os planos protegidos não são escritos
na configuração Cargo nem compilados como fallback no binário.

`force = true` faz o Cargo fornecer o endpoint aos processos que inicia mesmo
quando o shell não exporta a variável. O wrapper remove a variável antes de se
transformar no `rustc`; apenas o filho audit-only usa o canal.
Esse comportamento segue a seção oficial [`[env]` da configuração do
Cargo](https://doc.rust-lang.org/cargo/reference/config.html#env), que permite
definir variáveis para processos iniciados pelo Cargo e usar `force` para
substituir o ambiente herdado.

## Falha fechada e rollback

- instalação recusa endpoint ausente, vazio, relativo ou com componentes `.`/`..`;
- integração preexistente divergente é preservada e causa conflito;
- `build.rustc-wrapper`, inclusive cache como `sccache`, continua sem composição
  implícita;
- o backup já existente registra o hash do arquivo completo instalado;
- `uninstall` restaura os bytes e o modo originais, removendo também o endpoint
  apenas quando a prova de ownership corresponde;
- `doctor` lê o endpoint da configuração instalada sem publicá-lo e classifica
  uma definição local do mesmo campo como `override-detected`.

## Evidência executada

- testes focados de `cargo_config`: 14 aprovados;
- testes focados de `cargo_installer`: 12 aprovados;
- testes focados de `cargo_doctor`: 7 aprovados;
- `make cargo-command-matrix`: executa `cargo check`, `build`, `test`, `run` e
  `clippy` após remover a variável do shell e exige que o endpoint continue na
  configuração instalada;
- `make ci`: 189 testes de biblioteca, 282 testes do binário aprovados com sete
  probes reais ignorados, nove testes de integração aprovados e nondisclosure do
  release verde.

## Limite desta etapa

Isto prepara a integração automática, mas não conclui a T8.6. Ainda faltam o
bundle e a implantação real das identidades/unidades systemd, a instalação no
Cargo home real e a troca operacional do projeto piloto. Essas mudanças devem
continuar separadas e reversíveis.
