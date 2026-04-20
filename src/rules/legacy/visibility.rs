use super::super::AuditViolation;
use super::super::config::AuditConfig;

/// [WHAT] Contexto de auditoria de visibilidade estrutural para detecção de structs privadas que bloqueiam orquestração industrial.
/// [WHY] Evita amalgamento do geode detectando structs cegas, permitindo a transposição livre para _unit, _func e _struct sem dependências ciclicas no rustc.
/// [WHERE] Região de detecção em visibility.rs, no motor P1 do Geode L0.
/// [USAGE] Varre código ativamente e invoca punição educacional instigando práticas puras de isolamento por "getters" ao invés de apagão físico.
pub struct VisibilityAuditCtx<'a> {
    pub file_path: &'a str,
    pub source: &'a str,
    pub config: &'a AuditConfig,
    pub violations: &'a mut Vec<AuditViolation>,
    pub detected_dna: String,
    pub taxonomy_path: String,
}

/// [WHAT] Scanner brutal linha-a-linha de tokens léxicos para descobrir structs cegas na geometria arquitetural pura.
/// [WHY] Reduz a barreira do pânico nativo Rustc antecipando quebra de compilação em refactors modulares no geode L0.
/// [WHERE] Root principal no módulo visibility.rs em rules L0 baseadas na gramática Rust crua.
/// [USAGE] Ativo puramente no estágio central de parsing para fatiar arquivos, extrair struct decls e validar o preâmbulo do pub.
pub fn verify(ctx: &mut VisibilityAuditCtx<'_>) {
    let check_private = !crate::governance::is_rule_ignored(ctx.file_path, "hierarchy.structure.private_struct", ctx.config);
    let check_nested = !crate::governance::is_rule_ignored(ctx.file_path, "hierarchy.structure.nested_struct", ctx.config);
    let check_unit = !crate::governance::is_rule_ignored(ctx.file_path, "hierarchy.structure.unit_struct", ctx.config);
    let check_tuple = !crate::governance::is_rule_ignored(ctx.file_path, "hierarchy.structure.tuple_struct", ctx.config);
    let check_generics = !crate::governance::is_rule_ignored(ctx.file_path, "hierarchy.structure.inline_generics", ctx.config);
    
    let mut depth: isize = 0;

    for (idx, line) in ctx.source.lines().enumerate() {
        // Rastreio de profundidade modular C0
        for c in line.chars() {
            if c == '{' { depth += 1; }
            if c == '}' { depth -= 1; }
        }

        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") { continue; }
        
        let code = match trimmed.find("//") {
            Some(i) => trimmed[..i].trim_end(),
            None => trimmed,
        };

        if is_struct_declaration(code) {
            let name = extract_struct_name(code);
            
            let is_nested = detect_nested_struct(depth, line);
            let is_tuple = detect_tuple_struct(trimmed);
            let is_unit = detect_unit_struct(trimmed);
            let is_private = detect_private_struct(code);

            // [Mutually Exclusive Precedence Architecture]
            if is_nested && check_nested {
                ctx.violations.push(AuditViolation {
                    file_path: ctx.file_path.to_string(),
                    line: idx + 1,
                    rule_id: crate::governance::rule_id("hierarchy.structure.nested_struct", &name),
                    message: super::super::rules_logs::messages::NESTED_STRUCT.to_string(),
                    snippet: Some(code.to_string()),
                    quick_fix: Some("// [MIGRATE] Keep fully untouched inside _func.rs closure".to_string()),
                    detected_dna: ctx.detected_dna.clone(),
                    taxonomy_path: format!("{}.limits", ctx.taxonomy_path),
                });
            } else if is_tuple && check_tuple {
                ctx.violations.push(AuditViolation {
                    file_path: ctx.file_path.to_string(),
                    line: idx + 1,
                    rule_id: crate::governance::rule_id("hierarchy.structure.tuple_struct", &name),
                    message: super::super::rules_logs::messages::TUPLE_STRUCT.to_string(),
                    snippet: Some(code.to_string()),
                    quick_fix: Some("// [MIGRATE] Morph to Field Struct {} and move to _struct.rs".to_string()),
                    detected_dna: ctx.detected_dna.clone(),
                    taxonomy_path: format!("{}.limits", ctx.taxonomy_path),
                });
            } else if is_unit && check_unit {
                ctx.violations.push(AuditViolation {
                    file_path: ctx.file_path.to_string(),
                    line: idx + 1,
                    rule_id: crate::governance::rule_id("hierarchy.structure.unit_struct", &name),
                    message: super::super::rules_logs::messages::UNIT_STRUCT.to_string(),
                    snippet: Some(code.to_string()),
                    quick_fix: Some("// [MIGRATE] Move unmodified to _struct.rs".to_string()),
                    detected_dna: ctx.detected_dna.clone(),
                    taxonomy_path: format!("{}.limits", ctx.taxonomy_path),
                });
                if is_private && check_private {
                    ctx.violations.push(AuditViolation {
                        file_path: ctx.file_path.to_string(),
                        line: idx + 1,
                        rule_id: crate::governance::rule_id("hierarchy.structure.private_struct", &name),
                        message: super::super::rules_logs::messages::PRIVATE_STRUCT.to_string(),
                        snippet: Some(code.to_string()),
                        quick_fix: Some(format!("pub struct {}", name)),
                        detected_dna: ctx.detected_dna.clone(),
                        taxonomy_path: format!("{}.limits", ctx.taxonomy_path),
                    });
                }
            } else if check_private && is_private {
                ctx.violations.push(AuditViolation {
                    file_path: ctx.file_path.to_string(),
                    line: idx + 1,
                    rule_id: crate::governance::rule_id("hierarchy.structure.private_struct", &name),
                    message: super::super::rules_logs::messages::PRIVATE_STRUCT.to_string(),
                    snippet: Some(code.to_string()),
                    quick_fix: Some(format!("pub struct {}", name)),
                    detected_dna: ctx.detected_dna.clone(),
                    taxonomy_path: format!("{}.limits", ctx.taxonomy_path),
                });
            }

            // [Parallel Checks]
            if check_generics && detect_inline_generics(code) {
                ctx.violations.push(AuditViolation {
                    file_path: ctx.file_path.to_string(),
                    line: idx + 1,
                    rule_id: crate::governance::rule_id("hierarchy.structure.inline_generics", &name),
                    message: super::super::rules_logs::messages::INLINE_GENERICS.to_string(),
                    snippet: Some(code.to_string()),
                    quick_fix: Some(format!("struct {} where /* */", name)),
                    detected_dna: ctx.detected_dna.clone(),
                    taxonomy_path: format!("{}.limits", ctx.taxonomy_path),
                });
            }
        }
    }
}

/// [WHAT] Analisador cego inicial de pré-requisitos para fatiar apenas blocos validos de estrutura L0 do RUST.
fn is_struct_declaration(code: &str) -> bool {
    if !code.contains("struct ") { return false; }
    if code.contains("impl ") || code.contains("for ") { return false; }
    
    let mut words = code.split_whitespace();
    // Pula possíveis modificadores e chega no target
    while let Some(w) = words.next() {
        if w == "struct" { return true; }
        if !w.starts_with("pub") { break; }
    }
    false
}

/// [WHAT] Lógica atômica para fatiamento booleano léxico de visibilidade da string de struct blindando declarações corretas e filtrando uso em generic impls iterativas purificadas L0.
/// [WHY] Minimiza pânicos puros de falsos positivos no painel de SARIF focado apenas em root blocks de inicialização nativa no Rust funcional.
/// [WHERE] Delegador interno de scanner no visibility.rs puro contido e autônomo L1 no painel base.
/// [USAGE] Avalia o formato `struct Nome {`, filtrando as strings que carreguem preâmbulos como `pub`, `pub(crate)` e blindando `impl ` e `for `.
fn detect_private_struct(code: &str) -> bool {
    if code.starts_with("pub ") || code.starts_with("pub(") { return false; }
    let has = code.split_whitespace().next() == Some("struct");
    has
}

/// [WHAT] Identificador L0 de aninhamento irregular orgânico verificando se a struct foi criada no recesso obscuro de uma scope de função (profundidade de brackets).
fn detect_nested_struct(depth: isize, raw_line: &str) -> bool {
    // Se não for root level (ou level = 1 dentro de macro_rules)
    // Usamos heurística de tabulação/espaço junto do tracker
    (depth > 1) || raw_line.starts_with("    struct") || raw_line.starts_with("\tstruct")
}

/// [WHAT] Identificador de Headless Entities (Unit Structs) estritos: blindados sem parenteses para não colidir com Tuple Structs.
fn detect_unit_struct(trimmed: &str) -> bool {
    trimmed.ends_with(';') && !trimmed.contains('{') && !trimmed.contains('(')
}

/// [WHAT] Identificador letal brutal L0 para Tuplas de estado anônimo que suprimem nomeações.
fn detect_tuple_struct(trimmed: &str) -> bool {
    trimmed.ends_with(';') && !trimmed.contains('{') && trimmed.contains('(') && trimmed.contains(')')
}

/// [WHAT] Identificador orgânico P1 para varrer Generic Constrains embutidos num fátio, com Exceção Imunológica const.
fn detect_inline_generics(code: &str) -> bool {
    code.contains('<') && code.contains(':') && code.contains('>') && !code.contains("where") && !code.contains("const ")
}


/// [WHAT] Analisador primitivo de extração sintática do sufixo identificador de declaração.
/// [WHY] Melhora drasticamente a usabilidade do JSON apontando o nome direto no P1 sem obrigar o dev a ler o dump longo L0 estrito do host.
/// [WHERE] Extrator léxico inativo em visibility.rs alocado na ramificação L0 base.
/// [USAGE] Roda agressivo sempre que a punição P1 for inevitável, puxando a String suja do array até encontrar blocos neutros puros e devolvendo a matriz da syntax.
fn extract_struct_name(code: &str) -> String {
    let mut words = code.split_whitespace();
    while let Some(w) = words.next() {
        if w == "struct" {
            return words.next().unwrap_or("unknown")
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                .to_string();
        }
    }
    "unknown".to_string()
}
