# Guia de Integração: Governança Industrial Elite

Este documento descreve o protocolo passo a passo para habilitar a governança automática de alta integridade em um novo projeto (*Geode*) do ecossistema.

## 1. Implantação do Auditor Binário
O primeiro passo para a industrialização é garantir que o binário compressor de conformidade esteja disponível no caminho de plugins do ecossistema.

- Certifique-se de que o executável `ac_ant_delirium` (ou o binário de governança exportado) esteja localizado no diretório de plugins comum: `../plugins/`.

## 2. Configuração do Manifesto de Governança
Cada projeto deve possuir seu próprio conjunto de regras e exceções definido em um arquivo de manifesto.

1. Crie um arquivo `delirium.toml` na raiz do novo projeto.
2. Configure as zonas ativas e os limites estruturais:
   ```toml
   [active_to]
   "src/" = "all"

   [rules.limits]
   max_lines = 300
   max_nesting = 3
   ```
3. Defina as justificativas de "Safe Harbor" (quarentena) para tokens proibidos se necessário:
   ```toml
   [ignore_to]
   "src/restricted_tokens/" = ["all"]
   ```

## 3. Implementação do Gatekeeper de Build
Para garantir que nenhum código não-conforme seja compilado, o auditor deve ser integrado ao ciclo de build do Cargo.

1. Crie um arquivo `build.rs` na raiz do projeto.
2. Configure o script para invocar o binário de governança seguindo o padrão de re-execução industrial:
   ```rust
   fn main() {
       // Garante que o auditor rode se houver mudanças no código ou regras
       println!("cargo:rerun-if-changed=src");
       println!("cargo:rerun-if-changed=delirium.toml");

       let status = std::process::Command::new("../plugins/ac_ant_delirium")
           .arg(".") // Caminho do Geode alvo
           .status()
           .expect("Falha ao executar o auditor.");

       if !status.success() {
           panic!("\n\n[GOVERNANCE_FAILURE] O Geode não atingiu o estado Elite.\n");
       }
   }
   ```

## 4. Certificação de Pareamento Industrial
A governança Elite exige que cada módulo de código possua uma contraparte documental clara.

- Para cada arquivo `.rs` importante no código fonte, crie um arquivo `.md` correspondente no mesmo diretório.
- Exemplo: `src/logic.rs` deve ser acompanhado por `src/logic.md`.
- O auditor falhará se detectar o código sem a devida documentação pedagógica pareada.

## 5. Verificação de Densidade Pedagógica
Finalmente, garanta que todas as funções públicas e privadas sigam o padrão de tags Elite:

- `[WHAT]`: O que o componente faz.
- `[WHY]`: Justificativa arquitetural (mínimo de 64 caracteres).
- `[WHERE]`: Localização na hierarquia industrial.
- `[USAGE]`: Como operar a lógica (mínimo de 128 caracteres).

---
**Status da Integração**: Seguindo este protocolo, o novo projeto estará automaticamente protegido pela governança industrial e pronto para certificação Elite.
