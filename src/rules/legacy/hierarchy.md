# Governança de Hierarquia e Integridade Física (L0)

Este módulo implementa o Stage 0 (L0) da auditoria industrial, atuando como o primeiro gateway de integridade física e geográfica do código.

## Propósito
Garantir que cada arquivo no repositório possua uma anatomia clara, uma nomenclatura semântica e resida na jurisdição geográfica autorizada. O Gate L0 impede que inconsistências estruturais "poluam" as auditorias lógicas subsequentes.

## Regras Implementadas
- **DNA Enforcement**: Identificação de tipos de arquivos (Unit, Macro, Struct) baseada em tokens léxicos.
- **Jurisdiction Check**: Validação de caminhos físicos contra o manifesto de hierarquia funcional.
- **Naming Esthetics**: Imposição de Padrão PascalCase com suporte a separadores customizados (Hífen/Underline).
- **Fragmentation Gate**: Bloqueio de arquivos com alta densidade de lógica sem identificação de tag explícita.

## Impacto Arquitetural
A falha em qualquer uma destas regras deve ser tratada como CRÍTICA (P0/L0), pois indica um desvio fundamental na organização física do Geode.
