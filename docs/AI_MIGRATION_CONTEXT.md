# AI Migration Context - Aesthetic Core 

Este documento consolida as táticas, armadilhas (blindspots) e "Regras de Ouro L0" estabelecidas para orientar **Inteligências Artificiais e Agentes de Refatoração** na hora de quebrar código Legado Massivo para a taxonomia do Aesthetic Core dentro de Rust. 

A não adesão absoluta a estas regras resultará na falha da construção de sintaxe do Rust (Morte pelo Compilador) ou Exaustão Cognitiva da IA.

---

## 1. O Escalonamento Operacional (A Dança do Escudo)
A engine `ac_ant_delirium` está arquitetada em Modo Triagem Híbrida. 

**Ao receber um arquivo legado massivo para refatorar, siga este fluxo mental:**
1. **Atente-se aos Erros:** Você receberá uma visão JSON filtrada (Shadow Context). A Engine intencionalmente OCULTARÁ todos os erros **P2** (Semânticos, `[WHAT]`, docstrings) enquanto houver erros **P0** (Dependências proscritas) ou **P1** (Tamanho gigante do arquivo, alto aninhamento).
2. **Ignorar Documentação na Virada L0:** Sendo assim, na primeira passagem, você **NÃO DEVE tentar escrever documentação pedagógica**. Limite-se a extrair funções densas do código raiz, separá-las em funções `_func.rs` ou `_struct.rs` e zerar as anomalias numéricas que o SARIF acusar!
3. **Estágio Escritor (L2):** Apenas depois que o `cargo check` brilhar limpo na estrutura (0 violações em P1), o orquestrador cuspirá os chamados originais e exigirá a pedagogia (P2) que foi prometida à casca (_unit). Daí será a sua hora de redigir.

---

## 2. A Morte de Privacidade e o Isolamento Estrutural 
A maior causa de corrupção do Geode durante a refatoração LLM advém de violação de visibilidade da crate (`Private fields access`).

No **Clean Architecture L0**, seguimos separação física de domínio e serviço:
- **`NomeDoGigante_struct.rs`**: Onde as Structs e Enums viverão sozinhas (Domain Data).
- **`NomeDoGigante_func.rs`**: Onde as implementações complexas e cálculos pesados habitarão.
- **`NomeDoGigante_unit.rs`**: O Maetro (A Fachada Pública), que importará e guiará tudo via traits/inclusão estendida.

> [!WARNING]  
> **A Regra de Ouro do Encapsulamento de Structs Múltiplas:**
> Se no arquivo recém quebrado, a struct e sua lógica matemática ficarem separados em arquivos diferentes, **não tente mutar as propriedades escondidas que são privadas dentro do novo `_func.rs` sem construir getters/setters ou falhará com Borrow Checker Rule violation.**

### A Grande Diferença (DTOs vs Objects)
Ao processar componentes e structs, escolha um entre dois modos com brutalidade:
- **Payload Transport (Plain Old Data):** É só uma struct que guarda String, números frouxos ou pacotes que não impõem risco sistêmico se sofrerem mutações na memória paralela? **FAÇA TUDO `pub`!** Pense `pub field`. Libere as correntes do borrow.
- **Domínio Lógico Defensivo:** Se a Struct tem um limite severo e inquebrável (*ex: "start nunca pode ser maior que end"*), **Mantenha-os esfericamente escondidos**. NÃO USE `pub` nas fields. Crie um Cinto de Segurança no `novo(start, end)` com cláusulas de `panic!` impiedosas e valide antes de injetar na heap. Assegure a "Física do Código".

---

## 3. Retenções Biológicas Críticas à Resolução Modular 
O arquivo raiz `Monolito.rs` original possuía dezenas de `use` no topo da página. Quando a anatomia dele explodir para pastas filhas fragmentadas, o Agente tenderá a esquecer os binds, causando falha grave de symbol matching.

**Diretrizes de Extração:**
- **Injeção Obrigatória no Func:** Comece as testas de todos os arquivos internos (`_func`) com o selo universal `use super::*;`, e transfira todos os links estritos (Dependencies) herdados ou use imports diretos de trait lá.
- **Traits de Visibilidade Frouxa:** Em Rust, impls de Traits podem ser fatiados em múltiplos módulos, mas o `Cargo.toml` não aceitará falhas nos limites de escopo (crate).

---

## 4. O Abismo dos Macros
A `rule_macro_isolation` (Severidade P1 do Aesthetic Core) impõe lei marcial. Rotinas sistêmicas que manipulam o stack Rust como o `macro_rules!` nunca podem coexistir no meio de laços `for/if` de bibliotecas funcionais atômicas em arquivos públicos `unit`.

**O Mandato do Macro:**
- Extraia imediatamente a raiz do macro e exile-a para um arquivo exclusivo tipado como `alvo_macro.rs`.
- Puxe o macro aprisionado de volta para o escopo lógico central utilizando puramente `#[macro_use]` das estirpes Rust nativas e continue a operação em paz.

---
> *Este documento foi gravado a partir da congregação operacional entre Humano e I.A Central na Matriz Aesthetic Core, projetado para que todas as futuras sub-IAs do Geode que executarem os comandos de migração compreendam perfeitamente os contornos de vida do Rust, sem sacrificar tempo de máquina com anomalias nativas da linguagem.*
