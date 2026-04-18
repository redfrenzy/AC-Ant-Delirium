use super::super::AuditViolation;
use super::super::configs::hierarchy_config::HierarchyConfig;
use std::path::Path;

/// [WHAT] Contexto de auditoria de hierarquia e integridade física de arquivos (L0).
/// [WHY] Centraliza os dados necessários para validar se um arquivo possui a anatomia correta e reside na jurisdição geográfica autorizada antes do início de auditorias lógicas.
/// [WHERE] Regras estruturais em ac_ant_delirium/src/rules/hierarchy.rs.
pub struct HierarchyAuditCtx<'a> {
    pub file_path: &'a str,
    pub root: &'a str,
    pub source_code: &'a str,
    pub function_count: usize,
    pub config: &'a HierarchyConfig,
    pub violations: &'a mut Vec<AuditViolation>,
    pub detected_dna: String,
    pub taxonomy_path: String,
}

/// [WHAT] Orquestra a validação estrutural Stage 0 (L0) de um arquivo fonte do Geode, atuando como o primeiro filtro de integridade física antes de auditorias lógicas mais densas.
/// [WHY] Garante que a fundação arquitetural do arquivo esteja correta, bloqueando o processamento se houver desvio de jurisdição ou nomenclatura industrial, mantendo a soberania estética do projeto.
/// [WHERE] Ponto de entrada Stage 0 no subsistema de regras de hierarquia em hierarchy.rs, servindo como o gatekeeper territorial do motor Ant Delirium.
/// [USAGE] Invocado pelo auditor global para realizar o gatekeeper físico antes da análise de documentação ou métricas, recebendo o contexto de auditoria e devolvendo violações estruturais descobertas durante a varredura linear do arquivo.
pub fn verify(ctx: &mut HierarchyAuditCtx<'_>) {
    if ctx.config.is_path_ignored(ctx.file_path) { return; }

    let path = Path::new(ctx.file_path);
    let stem = path.file_stem().and_then(|s| s.to_str());
    let file_name = match stem { Some(s) => s, None => "" };
    let ext = match path.extension().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => "",
    };

    if ext == "md" || ext == "json" {
        check_md_json_pairing(ctx, file_name, ext);
        return;
    }
    
    let (core_name, tag_raw, canonical_dna, sep_used) = ctx.config.resolve_dna_and_name(file_name);
    
    let final_tag = match canonical_dna {
        Some(d) => d,
        None => {
            let dna = ctx.config.detect_dna(ctx.source_code);
            let detected = match dna { Some(d) => d, None => "unknown".to_string() };
            
            if detected == "unknown" {
                check_identity_missing(ctx, file_name);
            } else {
                check_naming_drift(ctx, file_name, &detected);
            }
            detected
        }
    };

    // [SOBERANIA DE SEPARADORES] Validação estética rigorosa de separadores
    if let (Some(used), Some(tag)) = (sep_used, tag_raw.as_ref()) {
        if used != ctx.config.tag_separator {
             let msg = format!("Separator Infiltration (P0): Tag '{}' must use the tag separator '{}'. The separator '{}' is reserved for the base name.", tag, ctx.config.tag_separator, used);
             ctx.violations.push(AuditViolation {
                file_path: ctx.file_path.to_string(),
                line: 1,
                rule_id: "hierarchy.formatting".to_string(),
                message: msg,
                snippet: Some(format!("{}{}", used, tag)),
                quick_fix: Some(format!("{}{}", ctx.config.tag_separator, tag)),
                detected_dna: ctx.detected_dna.clone(),
                taxonomy_path: "hierarchy.tag_separator".to_string(),
            });
        }
    }

    if !ctx.config.validate_naming(core_name, tag_raw.as_deref()) {
        let mut msg = format_naming_error(core_name, &ctx.config.casing, &ctx.config.context_info);
        
        // [SEPARATOR SOVEREIGNTY] Detects if the error was caused by tag separator infiltration
        if core_name.contains(&ctx.config.tag_separator) {
            msg = format!("Separator Infiltration (P0): The character '{}' is a reserved tag separator. To separate base name parts, use the name separator '{}' exclusively.", 
                ctx.config.tag_separator, ctx.config.name_separator);
        }

        if tag_raw.as_ref().map_or(false, |t| t.contains('.')) {
            msg = format!("{} [REASON] Hierarchical tags in filename must use '{}' instead of '.' (Ex: name_unit{}func.rs).", 
                msg, ctx.config.tag_separator, ctx.config.tag_separator);
        }
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.formatting".to_string(),
            message: msg,
            snippet: Some(file_name.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.casing".to_string(),
        });
    }

    if !ctx.config.validate_jurisdiction(&final_tag, ctx.file_path) {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.jurisdiction".to_string(),
            message: format!("Geographic Violation: File with DNA '{}' cannot reside in '{}'.", final_tag, ctx.file_path),
            snippet: Some(ctx.file_path.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: format!("{}.jurisdiction", ctx.taxonomy_path),
        });
    }

    check_cluster_companions(ctx, core_name, &final_tag);
    check_strict_purity(ctx, &final_tag);
    check_fragmentation_limit(ctx, file_name);
    check_linkage_restriction(ctx, core_name);
}

/// [WHAT] Bloqueia ativamente a ousadia imatura L1 de desenvolvedores tentando documentar autonomamente metodologias privadas ou arquivos filhos irrelevantes.
/// [WHY] Arquivos Raiz são as interfaces públicas da arquitetura. A documentação deve se aglutinar e refletir neles via _manifest.json, cortando brutalmente a dispersão descentralizada inaudível de MDs perdidos gerando manutenção lixo em L0.
/// [WHERE] Modulo de regra de Pareamento no hierarchy.rs gatekeeper local estrito cego robusto de extensão L1.
/// [USAGE] Chamada cruamente linear inquestionável imediata pelo guardião principal assim que flagra um pacote .json ou .md adentrando as muralhas de processamento C0 da compilação.
fn check_md_json_pairing(ctx: &mut HierarchyAuditCtx<'_>, file_name: &str, ext: &str) {
    let bypass_manifest = ext == "json" && file_name.ends_with("_manifest");
    if file_name.contains(&ctx.config.tag_separator) && !bypass_manifest {
        push_md_json_violation(ctx, file_name);
    }
}

/// [WHAT] Atirador de elite isolado em sub-rotina para penalizar e notificar desvios estruturais estritos cravados de compilação cruzada.
/// [WHY] Reduz dramaticamente a taxa implacável abissal imunda monstruosa severa de aninhamento P1 para zero absoluto L0 nativo.
/// [WHERE] Helper nativo linear em hierarchy.rs cravado intocável puritano blindado.
/// [USAGE] Uso ativo mecânico imperativo veloz silencioso e cego no momento do colapso ético moral L0 P0 dos diretórios, acionado sem dó nem piedade quando documentos imundos tentam atuar sozinhos.
fn push_md_json_violation(ctx: &mut HierarchyAuditCtx<'_>, file_name: &str) {
    ctx.violations.push(AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: "hierarchy.pairing_violation".to_string(),
        message: "Pairing Violation: Child nodes must not have specialized documentation files. Move information to the parent's json/md file.".to_string(),
        snippet: Some(file_name.to_string()),
        quick_fix: None,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: "hierarchy.root_md_pairing".to_string(),
    });
}



/// [WHAT] Inovação técnica de Cluster L0 que exige a presença física de todos os entes federados e dependentes de um DNA mestre.
/// [WHY] Garante que uma Unidade nunca esteja "Mutilada". Se o cluster exige uma função e um teste, o geode deve prover ambos, respeitando a Regra de Proximidade Industrial.
/// [WHERE] Auditor de Cluster em hierarchy.rs.
fn check_cluster_companions(ctx: &mut HierarchyAuditCtx<'_>, core_name: &str, final_tag: &str) {
    let parent_node = match ctx.config.taxonomy.get(final_tag) {
        Some(n) => n,
        None => return,
    };

    // [SOBERANIA] Se o nó pai não exige pureza, não cobramos o cluster completo (Bypass para Protótipos)
    if parent_node.strict_purity == Some(false) {
        return;
    }

    let children = ctx.config.get_immediate_children(final_tag);
    for (child_dna, child_node) in children {
        // [LEI DO SOBRENOME] O sufixo é derivado da última parte da taxonomia (ex: func em unit.func)
        let suffix = match child_dna.split('.').last() {
            Some(s) => s,
            None => continue,
        };

        let companion_file_name = format!("{}{}{}.rs", core_name, ctx.config.tag_separator, suffix);

        // DETERMINAÇÃO DE GRAVIDADE (PROXIMIDADE)
        let is_local = child_node.jurisdiction.is_none() || 
                      child_node.jurisdiction == parent_node.jurisdiction;

        let (target_path, display_jur): (std::path::PathBuf, String) = if is_local {
            // Regra de Proximidade: Deve estar na mesma pasta do pai
            let parent_dir = std::path::Path::new(ctx.file_path).parent().unwrap_or(std::path::Path::new("."));
            (parent_dir.join(&companion_file_name), format!("na mesma pasta do pai ({})", parent_dir.display()))
        } else {
            // Regra Global: Busca na jurisdição definida
            let jur = child_node.jurisdiction.as_ref().unwrap();
            let root_path = std::path::Path::new(ctx.root);
            (root_path.join(jur).join(&companion_file_name), jur.clone())
        };

        if !target_path.exists() {
            let mutilation_type = if suffix == "test" { "Test" } else { "Functional" };
            
            // [SHADOW SEARCH] O arquivo não está onde devia. Vamos caçá-lo ativamente.
            if let Some(found_path) = find_companion_in_shadow(std::path::Path::new(ctx.root), &companion_file_name) {
                let display_found = found_path.display().to_string();
                ctx.violations.push(AuditViolation {
                    file_path: ctx.file_path.to_string(),
                    line: 1,
                    rule_id: "hierarchy.jurisdiction_decoupling".to_string(),
                    message: format!("Jurisdiction Decoupling ({}) (P0): Node '{}' mapped companion '{}' in an illegal territory [{}] instead of the authorized [{}].", 
                        mutilation_type, final_tag, companion_file_name, display_found, display_jur),
                    snippet: Some(companion_file_name),
                    quick_fix: None,
                    detected_dna: ctx.detected_dna.clone(),
                    taxonomy_path: format!("{}.jurisdiction", ctx.taxonomy_path),
                });
            } else {
                ctx.violations.push(AuditViolation {
                    file_path: ctx.file_path.to_string(),
                    line: 1,
                    rule_id: "hierarchy.cluster_decoupling".to_string(),
                    message: format!("Cluster Mutilation ({}) (P0): Node '{}' requires companion '{}' {}, but it was not found anywhere in the geode.", 
                        mutilation_type, final_tag, companion_file_name, display_jur),
                    snippet: Some(companion_file_name),
                    quick_fix: None,
                    detected_dna: ctx.detected_dna.clone(),
                    taxonomy_path: format!("{}.strict_purity", ctx.taxonomy_path), // Cluster checks derive from strict_purity
                });
            }
        }
    }
}

/// [WHAT] Algoritmo Shadow Search recursivo puro L0 focado na velocidade.
/// [WHY] Permite que o motor faça uma varredura tática profunda para diagnosticar se um arquivo foi mutilado ou apenas realocado, sem depender de crates de terceiros como walkdir, isolando áreas pesadas como target/.
/// [WHERE] Funil de busca utilitária no modulo de regras L0.
/// [USAGE] Chamada linear e cirúrgica para buscar orgãos desacoplados dentro das paredes do próprio Geode, pulando diretórios tóxicos para o I/O.
fn find_companion_in_shadow(dir: &std::path::Path, target_name: &str) -> Option<std::path::PathBuf> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name() {
                    if file_name.to_string_lossy() == target_name {
                        return Some(path);
                    }
                }
            } else if path.is_dir() {
                let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                // Bloqueando Abismos Naturais
                if dir_name != "target" && dir_name != ".git" {
                    if let Some(found) = find_companion_in_shadow(&path, target_name) {
                        return Some(found);
                    }
                }
            }
        }
    }
    None
}

/// [WHAT] Cão de guarda atroz bestial brutal inflexível cego implacável frio puritano cortante fulminante agnóstico silencioso isolador estrito que barra imediatamente mistura imunda tóxica venenosa suja contaminante fatal severa de métodos privados rasteiros vazados na nobre interface pura suprema intocável blindada ou o abismo contrário.
/// [WHY] Consagra eternamente cravado L0 o dogma industrial supremo inquebrável majestoso e formidável orgânico blindado blindado da Segregação Puritana Estrita do Estado (Files de API vs Files de Lógica Suja Oculta).
/// [WHERE] Entrincheirado nas roldanas atômicas imaculadas implacáveis lineares velozes rasas do hierarchy.rs embutido estático na compilação cruzada do geode L0 rust limpo C0 livre.
/// [USAGE] Ativo imperativo linear cru isolado para destrinchar brutalmente a alma do arquivo cru quando a property 'strict_purity' pisca em vermelho P0 letal ativo no TOML puro do projeto oficial estrito L0.
fn check_strict_purity(ctx: &mut HierarchyAuditCtx<'_>, tag: &str) {
    let mut current = tag;
    let mut is_pure = false;

    // [HERANÇA INDUSTRIAL] Sobe a árvore até achar uma definição explícita de pureza
    loop {
        if let Some(node) = ctx.config.taxonomy.get(current) {
            if let Some(p) = node.strict_purity {
                is_pure = p;
                break;
            }
        }
        let parent = ctx.config.get_parent_path(current);
        if parent.is_empty() { break; }
        current = parent;
    }

    if !is_pure { return; }

    // [MÉTRICA LÉXICA] Detecção resiliente de interfaces públicas
    let fn_count = ctx.source_code.matches("fn ").count() + ctx.source_code.matches("fn\n").count();
    let pub_count = ctx.source_code.matches("pub fn ").count() + 
                    ctx.source_code.matches("pub\nfn ").count() + 
                    ctx.source_code.matches("pub\tfn ").count();
                    
    let is_func = tag.contains("func");
    let path = Path::new(ctx.file_path);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Entity");
    let core_name = stem.split(&ctx.config.tag_separator).next().unwrap_or(stem);

    if is_func && pub_count > 0 {
        push_purity_violation(ctx, "pub fn", "Encapsulation Breach (P0): Subsidiary functional geode must not contain 'pub fn'. Public APIs must be exposed only by the primary '_unit' ancestor.");
    }

    if !is_func && fn_count > pub_count {
        let sug = get_suggested_path(ctx, "unit.func", core_name);
        let msg = format!("Encapsulation Breach (P0): Root interface contains private methods. Extract private logic to functional companions: '{}'.", sug);
        push_purity_violation(ctx, "fn ", &msg);
    }
    
    // [TYPE LOGIC SHIELD]
    if ctx.source_code.contains("macro_rules!") && !tag.contains("macro") {
        let sug = get_suggested_path(ctx, "macro", core_name);
        push_purity_violation(ctx, "macro_rules!", &format!("Encapsulation Breach (P0): Illegal 'macro_rules!' detected. Extract to: '{}'.", sug));
    }
    if ctx.source_code.contains("struct ") && !tag.contains("struct") {
        let sug = get_suggested_path(ctx, "struct", core_name);
        push_purity_violation(ctx, "struct", &format!("Encapsulation Breach (P0): Illegal 'struct' detected. Extract to: '{}'.", sug));
    }
    if ctx.source_code.contains("enum ") && !tag.contains("struct") {
        let sug = get_suggested_path(ctx, "struct", core_name);
        push_purity_violation(ctx, "enum", &format!("Encapsulation Breach (P0): Illegal 'enum' detected. Extract to: '{}'.", sug));
    }
}

/// [WHAT] Notificador purificador isolador e silenciador estrito de sentenças implacáveis imutáveis agnósticas pesadas que fragmentaria horrivelmente os avaliadores ciclomáticos da engine matriz formidável base L0 purificada sem aninhamento sujo covarde em C1 nativo P0 test de pureza severa.
/// [WHY] Garante honradamente achatamento funcional nativo perfeito e imaculado ao estraçalhar amaldiçoado Else abissal pesado C0 e afasta terror visual de aninhamentos cravados puritanos na regra P1 da fundação do geode limpo L0.
/// [WHERE] Helper nativo blindado orgânico cego do hierarchy.rs intocável puro.
/// [USAGE] Disparo mortal cego orgulhoso letal cravado no momento L0 em que a segregação estrita falha miseravelmente demonstrando fragilidade sistêmica atroz ao ceder misturas públicas e estritas L0 puras nativas absolutas imortais cruas vazando estado local imundo inquebrável atômico.
fn push_purity_violation(ctx: &mut HierarchyAuditCtx<'_>, snippet: &str, msg: &str) {
    ctx.violations.push(AuditViolation {
        file_path: ctx.file_path.to_string(),
        line: 1,
        rule_id: "hierarchy.purity".to_string(),
        message: msg.to_string(),
        snippet: Some(snippet.to_string()),
        quick_fix: None,
        detected_dna: ctx.detected_dna.clone(),
        taxonomy_path: format!("{}.strict_purity", ctx.taxonomy_path),
    });
}

/// [WHAT] Verifica se um arquivo excedeu o limite industrial de funções sem possuir uma tag de identificação explícita, forçando a fragmentação atômica ou a etiquetagem correta do recurso.
/// [WHY] Mantém a densidade do geode sob controle, impedindo a criação de arquivos massivos e sem semântica clara que prejudiquem a manutenibilidade e a análise por agentes de IA.
/// [WHERE] Auditor de densidade física funcional no módulo hierarchy.rs, operando como a regra final de conformidade estrutural.
/// [USAGE] Chamado internamente pelo motor de verificação de hierarquia para auditar a relação entre o número de funções e a clareza do nome do arquivo no disco.
fn check_fragmentation_limit(ctx: &mut HierarchyAuditCtx<'_>, file_name: &str) {
    if ctx.function_count > 5 && file_name.find(&ctx.config.tag_separator).is_none() {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.fragmentation".to_string(),
            message: format!("Densidade Crítica: Arquivo com {} funções deve ter uma tag explícita (Ex: name_func.rs) ou ser fragmentado.", ctx.function_count),
            snippet: Some(file_name.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.files.limits.max_functions".to_string(), // Fallback para limites globais
        });
    }
}

/// [WHAT] Constrói uma mensagem de erro de nomenclatura rica e pedagógica, incorporando dicas contextuais do manifesto para orientar o desenvolvedor na correção estética.
/// [WHY] Melhora a experiência do usuário ao fornecer não apenas o erro, mas a justificativa e as normas de branding do projeto de forma consolidada e legível.
/// [WHERE] Gerador de mensagens pedagógicas no motor de hierarquia em hierarchy.rs, mantendo a lógica de formatação isolada.
/// [USAGE] Invocado pelo validador de nomenclatura quando uma violação estética é detectada, gerando o corpo do diagnóstico de conformidade industrial.
fn format_naming_error(core_name: &str, casing: &str, context: &[String]) -> String {
    let mut msg = format!("Erro de Nomenclatura (Aesthetic Rule): '{}' não segue o padrão {}.", core_name, casing);
    if !context.is_empty() {
        msg.push_str("\n[CONTEXTO PEDAGÓGICO]:");
        for info in context {
            msg.push_str(&format!("\n▸ {}", info));
        }
    }
    msg
}

/// [WHAT] Verifica se um arquivo sem tag no nome possui um DNA detectado que exige renomeação imediata ou ajuste estrutural precoce.
/// [WHY] Força o 'Ajuste Fino' estrutural logo que o DNA é identificado por tokens léxicos, impedindo o acúmulo de débito técnico em arquivos sem semântica física.
/// [WHERE] Detector de drift de DNA no motor de integridade física em hierarchy.rs, servindo como a ponte entre o léxico e a organização do disco.
/// [USAGE] Chamado durante a fase de descoberta de tipos quando um nome de arquivo é considerado 'puro' (sem tag), recomendando a etiquetação baseada no conteúdo real.
fn check_naming_drift(ctx: &mut HierarchyAuditCtx<'_>, file_name: &str, detected: &str) {
    if detected != "unknown" && !file_name.contains(&ctx.config.tag_separator) {
         let sug = get_suggested_path(ctx, detected, file_name);
          ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.dna_drift".to_string(),
            message: format!("DNA Drift (P0): Content detected as '{}'. Missing tag in filename. AUTHORIZED PATH: '{}'.", detected, sug),
            snippet: Some(file_name.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: format!("{}.dna_drift", ctx.taxonomy_path),
        });
    }
}

/// [WHAT] Verifica se um arquivo é um 'fantasma' sem identidade no disco, exigindo a rotulação imediata se houver lógica presente.
fn check_identity_missing(ctx: &mut HierarchyAuditCtx<'_>, file_name: &str) {
    let lines_with_content = ctx.source_code.lines().filter(|l| !l.trim().is_empty() && !l.trim().starts_with("//")).count();
    let has_logic = ctx.source_code.contains("fn ") || ctx.source_code.contains("struct ") || ctx.source_code.contains("enum ") || ctx.source_code.contains("macro_rules!");

    if (lines_with_content > 5 || has_logic) && !file_name.contains(&ctx.config.tag_separator) {
        ctx.violations.push(AuditViolation {
            file_path: ctx.file_path.to_string(),
            line: 1,
            rule_id: "hierarchy.identity_missing".to_string(),
            message: format!("Identity Missing (P0): Non-skeleton file '{}' must have a DNA tag (_unit, _func, _struct, _macro).", file_path_name(ctx.file_path)),
            snippet: Some(file_name.to_string()),
            quick_fix: None,
            detected_dna: ctx.detected_dna.clone(),
            taxonomy_path: "hierarchy.files".to_string(),
        });
    }
}

fn get_suggested_path(ctx: &HierarchyAuditCtx<'_>, dna: &str, core_name: &str) -> String {
    let mut jur = String::from("src");
    let tag_suffix = dna.split('.').last().unwrap_or(dna).to_string();

    // Tenta obter a jurisdição específica do DNA
    if let Some(node) = ctx.config.taxonomy.get(dna) {
        if let Some(ref j) = node.jurisdiction {
            jur = j.clone();
        }
    }

    format!("{}/{}{}{}.rs", jur, core_name, ctx.config.tag_separator, tag_suffix)
}

fn file_path_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// [WHAT] Soldado orgânico L0 para coibir a invasão e acoplamento perigoso arquitetural entre unidades onde entidades acessam as funções do cofre privado de outras geodes de forma ilegal sem passar pela portaria principal (o arquivo _unit correspondente da API daquele módulo).
/// [WHY] Encapsula a lógica de portal blindado garantindo que _func seja uma caixa preta inatingível local a menos que o invasor seja o seu dono, reduzindo as dependências cruzadas (Spaghetti Architecture) que paralisam refatorações e sujam o DAG da matriz Elite.
/// [WHERE] Barreira local de encapsulamento funcional restritiva rodando linearmente direto no final do pipeline estrito nativo do hierarchy.rs.
/// [USAGE] Roda varrição ágil sem regex verificando estritamente se mod ou use aponta atrozmente covardemente cegamente ilegalmente para um componente `_func` estrangeiro (baseado no núcleo semântico core_name clonado avaliado in-place).
fn check_linkage_restriction(ctx: &mut HierarchyAuditCtx<'_>, core_name: &str) {
    let core_lower = core_name.to_lowercase();
    let sep = &ctx.config.tag_separator;
    let func_marker = format!("{}func", sep); // e.g. "_func" ou "-func"

    for (idx, line) in ctx.source_code.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") { continue; }
        
        // Verifica se a linha contém mod ou use declarativo
        let is_mod = trimmed.starts_with("mod ") || trimmed.contains(" mod ");
        let is_use = trimmed.starts_with("use ") || trimmed.contains(" use ");
        
        if is_mod || is_use {
            if trimmed.contains(&func_marker) {
                let line_lower = trimmed.to_lowercase();
                if !line_lower.contains(&core_lower) {
                    ctx.violations.push(AuditViolation {
                        file_path: ctx.file_path.to_string(),
                        line: idx + 1,
                        rule_id: "hierarchy.linkage".to_string(),
                        message: crate::rules_logs::messages::LINKAGE_RESTRICTION.to_string(),
                        snippet: Some(trimmed.to_string()),
                        quick_fix: Some(format!("// [API PORTAL] Use '{}unit' (or the respective architectural gate) to access the consumed logic.", sep)),
                        detected_dna: ctx.detected_dna.clone(),
                        taxonomy_path: "hierarchy.linkage_restriction".to_string(),
                    });
                }
            }
        }
    }
}
