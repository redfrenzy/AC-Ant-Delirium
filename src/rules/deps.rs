use super::super::AuditViolation;
use super::super::config::AuditConfig;
use super::super::rules_logs::messages;
use crate::restricted_tokens::resolve::elite_resolve_or;

/// [WHAT] Orchestrates dependency governance for Cargo.toml files using a 3-Tier Classification Engine.
/// [WHY] Para assegurar a pureza do ecossistema e evitar a infiltração de crates não auditados na árvore de dependências final.
/// [WHERE] Called by the audit_code entry point specifically for dependency files.
/// [USAGE] Orquestra a descoberta e validação por camadas das dependências de produção em arquivos Cargo.toml, garantindo que nenhum artefato externo comprometa a integridade industrial do geode.
/// [WHAT] Contexto de auditoria de dependências para redução de acoplamento funcional (rule_params).
pub struct DepsAuditCtx<'a> {
    pub path: &'a str,
    pub source: &'a str,
    pub config: &'a AuditConfig,
    pub violations: &'a mut Vec<AuditViolation>,
}

/// [WHAT] Orchestrates dependency governance for Cargo.toml files using a 3-Tier Classification Engine to maintain total architectural purity and security in the Aesthetic Core.
/// [WHY] To satisfy pure rust ecosystem constraints and prevent the infiltration of un-audited third party crates that could introduce vulnerabilities or compromise the build's integrity.
/// [WHERE] Dependency validation entry point in deps.rs, acting as the primary gatekeeper for external assets and layer compliance within the geode's project management manifest.
/// [USAGE] Called by audit_ecosystem to analyze Cargo.toml content via a context struct, triggering a line-by-line classification of all declared production dependencies.
pub fn verify(ctx: &mut DepsAuditCtx<'_>) {
    if !ctx.path.ends_with("Cargo.toml") { return; }

    let activated = resolve_default_deps(ctx.source);
    let mut in_deps = false;
    let mut acc = String::new();

    for (idx, line) in ctx.source.lines().enumerate() {
        let mut stripped = line.trim();
        // Remove trailing comments for parsing logic (but keep original for snippets if needed)
        if let Some(pos) = stripped.find('#') {
            stripped = &stripped[..pos].trim();
        }
        
        if stripped.is_empty() { continue; }

        if stripped.starts_with('[') && stripped.ends_with(']') {
            in_deps = is_active_dependency_section(stripped);
            continue;
        }

        if !in_deps { continue; }
        process_line_purity(ctx, idx, stripped, &mut acc, &activated);
    }
}

/// [WHAT] determines whether a specific section header in a Cargo.toml file represents a block of primary production dependencies that should be subject to governance auditing. This function performs string-based matching to identify headers containing the 'dependencies' keyword while explicitly excluding sections related to dev-dependencies or build-dependencies to ensure that only the final binary's dependency graph is validated against purity standards.
/// [WHY] To focus the auditing resources on the critical components that define the geode's runtime footprint and security posture. By filtering out non-production dependencies, the engine avoids raising false positives for developer tools and test utilities that do not affect the final Elite WASM asset, thereby maintaining the high signal-to-noise ratio required for automated AI compliance monitoring within the Aesthetic Core ecosystem.
/// [WHERE] Called internally by the primary `verify` loop in `deps.rs` whenever a line starting and ending with square brackets is encountered, acting as a structural gatekeeper for the subsequent dependency classification and verification phases.
/// [USAGE] Processada pelo motor de varredura para definir se o bloco subsequente de chaves deve ter suas dependências validadas contra a lista de permissões industrial.
fn is_active_dependency_section(header: &str) -> bool {
    header.contains("dependencies") 
        && !header.contains("dev-dependencies") 
        && !header.contains("build-dependencies")
}



/// [WHAT] Processes an individual line from a Cargo.toml dependency section.
/// [WHY] Para fornecer um motor de análise de manifestos independente de crates externos, garantindo a autonomia da governança.
/// [WHERE] Executed for every line within an active dependency section in deps.rs.
/// [USAGE] Called by verify() during the line-by-line scanning of Cargo.toml.
fn process_line_purity(
    ctx: &mut DepsAuditCtx<'_>,
    idx: usize, 
    line: &str, 
    acc: &mut String, 
    activated: &[String]
) {
    if line.contains('{') && !line.contains('}') {
        acc.push_str(line);
        return;
    }

    if acc.is_empty() {
        return handle_full_dep_line(ctx, idx, line.to_string(), activated);
    }
    let full_line = format!("{} {}", acc, line);
    handle_full_dep_line(ctx, idx, full_line, activated)
}

/// [WHAT] Finalize and verify a complete dependency declaration after reconstruction.
/// [WHY] Para separar a complexidade de reconstrução de strings da lógica semântica de validação técnica das dependências.
/// [WHERE] Terminal step in the dependency line processing pipeline in deps.rs.
/// [USAGE] Called by process_line_with_purity_checks once a full statement is ready.
fn handle_full_dep_line(
    ctx: &mut DepsAuditCtx<'_>,
    idx: usize, 
    full: String, 
    activated: &[String]
) {
    let parts: Vec<&str> = full.splitn(2, '=').collect();
    if parts.len() < 2 { return; }

    let name = parts[0].trim().trim_matches(|c| c == '"' || c == '\'');
    let val = parts[1].trim();

    classify_and_verify_dep(ctx, idx, name, val, &full, activated);
}

/// [WHAT] Evaluates a parsed dependency and assigns it to a governance Tier (LOCAL, ALLOWED, OPTIONAL).
/// [WHY] Para aplicar uma política de tolerância zero contra código de terceiros e garantir a integridade da arquitetura em camadas.
/// [WHERE] Core validation logic for the 3-Tier Classification Engine in deps.rs.
/// [USAGE] Called by handle_full_dep_line to perform the final security and architectural check.
fn classify_and_verify_dep(
    ctx: &mut DepsAuditCtx<'_>,
    idx: usize, 
    name: &str, 
    val: &str, 
    full: &str, 
    activated: &[String]
) {
    // TIER 1: LOCAL (Layering)
    if full.contains("path = \"../") {
        verify_architectural_layering(ctx, idx, name, full);
        return;
    }

    // TIER 2: ALLOWED (Whitelist)
    if ctx.config.rules.allowed_crates.contains(&name.to_string()) { return; }
    if name == "std" || name == "core" || name == "alloc" { return; }

    // TIER 3: OPTIONAL (Sanitized)
    if val.contains("optional = true") {
        verify_optional_purity(ctx, idx, name, activated);
        return;
    }

    // TIER 4: IMPURE (Violation)
    ctx.violations.push(AuditViolation {
        file_path: ctx.path.to_string(),
        line: idx + 1,
        rule_id: crate::governance::rule_id("rule_deps", name),
        message: format!("{}: External crate '{}' is restricted.", messages::DEPS_UNAUTHORIZED, name),
        snippet: Some(full.to_string()),
        quick_fix: None,
    });
}

/// [WHAT] Validates that a path-based internal dependency adheres to the layering hierarchy and does not violate the established geode level constraints.
/// [WHY] Para impedir a criação de grafos de dependência cíclicos ou ilegais que comprometem a estabilidade e a manutenibilidade de longo prazo do projeto Aesthetic Core.
/// [WHERE] Internal pass for Tier 1 validation in deps.rs, integrated into the dependency classification engine to enforce architectural layering rules during the scan.
/// [USAGE] Called when a dependency is identified as a local geode via its relative path, comparing the target layer level with the current geode's architectural position.
fn verify_architectural_layering(ctx: &mut DepsAuditCtx<'_>, idx: usize, name: &str, full: &str) {
    let dep_name = name; // Use the crate name directly for layering check
    
    // [DYNAMIC DISCOVERY] Detect current project name from [package] section
    let mut current_project = "unknown";
    let mut in_package = false;
    for line in ctx.source.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" { in_package = true; continue; }
        if trimmed.starts_with('[') { in_package = false; continue; }
        if in_package && trimmed.starts_with("name =") {
            current_project = elite_resolve_or(trimmed.split('=').nth(1), "").trim().trim_matches('"');
            break;
        }
    }

    let current_geode_layer = map_geode_to_layer(current_project);
    let dep_layer = map_geode_to_layer(dep_name);

    // [ELITE RULE] A higher layer number (Platform/Base) cannot depend on a lower layer number (Governance/Top).
    if dep_layer < current_geode_layer && dep_layer != 99 {
        ctx.violations.push(AuditViolation {
            file_path: ctx.path.to_string(),
            line: idx + 1,
            rule_id: crate::governance::rule_id("rule_layering", dep_name),
            message: format!("{}: L{} geode cannot depend on '{}' (L{}).", messages::LAYERING_VIOLATION, current_geode_layer, dep_name, dep_layer),
            snippet: Some(full.to_string()),
            quick_fix: None,
        });
    }
}

/// [WHAT] Maps a specific geode project name to its assigned architectural layer (L0 to L6) based on the globally defined Aesthetic Core hierarchy rules.
/// [WHY] Para centralizar o mapa de camadas da arquitetura, permitindo uma validação determinística de toda a hierarquia geode e impedindo o acoplamento ilegal entre camadas.
/// [WHERE] Central infrastructure mapping utility in deps.rs, serving as the source of truth for geode levels within the governance and dependency validation engine.
/// [USAGE] Used by the layering enforcer to translate alphanumeric geode names into comparable integer levels, facilitating automated compliance checks during the build script execution.
fn map_geode_to_layer(name: &str) -> i32 {
    match name {
        "ac_ant_delirium" | "ant_delirium" | "test_delirium" => 0,
        "ac_png" | "ac_jpeg" | "ac_gif" | "ac_webp" | "ac_pnm" | "ac_ico" | "ac_hdr" | "ac_openexr" | "ac_farbfeld" | "ac_avif" | "ac_qoi" | "ac_tga" | "ac_bmp" | "ac_tiff" => 1,
        "ac_image" => 2,
        "ac_core" => 3,
        "ac_material" | "p0_layering" => 4,
        "ac_components" => 5,
        "ac_platform_linux" | "ac_platform_android" | "ac_platform_windows" | "p0_platform" => 6,
        _ => 99,
    }
}

/// [WHAT] Extrai a lista de dependências e features ativas a partir de um fragmento bruto do manifesto Cargo.toml.
/// [WHY] Para permitir a validação de camadas arquiteturais cruzando as definições do manifesto com o código fonte, garantindo que o Geode utilize apenas recursos autorizados.
/// [WHERE] Utilitário de análise de manifesto localizado em deps.rs, servindo como o scanner secundário para a política rule_deps.
/// [USAGE] Invocado pelo orquestrador de auditoria de dependências para identificar quais sub-recursos do crate estão habilitados, fornecendo a base para verificar se a utilização de crates de terceiros ou recursos internos do compilador está em conformidade com o gatekeeper industrial definido para o projeto Aesthetic Core.
fn resolve_default_deps(source: &str) -> Vec<String> {
    let mut active_features = Vec::new();
    let mut in_features = false;

    for line in source.lines() {
        let mut trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        
        // [ELITE SANITIZATION] Ignora comentários em linha antes do parsing.
        if let Some(pos) = trimmed.find('#') {
            trimmed = trimmed[..pos].trim();
        }
        if trimmed.is_empty() { continue; }

        // [Elite Section Discovery]
        if trimmed.starts_with("[features]") { in_features = true; continue; }
        if trimmed.starts_with('[') { in_features = false; continue; }
        
        if in_features && (trimmed.starts_with("default =") || trimmed.starts_with("default=")) {
            extract_features_from_line(trimmed, &mut active_features);
        }
    }
    active_features
}

/// [WHAT] Analisa uma linha do manifesto Cargo para identificar e extrair ativações de features, normalizando tokens e alimentando o acumulador de estado.
/// [WHY] Separa a lógica de parsing bruto da orquestração de dependências para manter a nidificação cognitivamente rasa e garantir que ativações complexas não sejam perdidas.
/// [WHERE] Utilitário de parsing de manifesto em deps.rs, atuando como o motor de descoberta de sub-recursos.
/// [USAGE] Invocado pelo scanner de manifesto quando uma linha contendo a chave 'features' é detectada, permitindo que o geode reconstrua o grafo de ativação industrial do projeto de forma determinística.
fn extract_features_from_line(line: &str, active_features: &mut Vec<String>) {
    let list = match line.split('=').nth(1) { Some(l) => l.trim(), None => "" };
    // Normalização Elite: remove colchetes e aspas externas da lista completa
    let inner = list.trim_matches(|c: char| c == '[' || c == ']' || c == '"' || c == '\'');
    
    for entry in inner.split(',') {
        add_feature_entry(entry, active_features);
    }
}

/// [WHAT] Normalizes a single feature entry and identifies implicit dependency activations. It handles quote stripping and detects tokens that trigger optional dependencies via Cargo's feature activation syntax.
/// [WHY] Para evitar o 'Abismo Cognitivo' durante a análise de funções, garantindo que ativações ocultas sejam devidamente detectadas.
/// [WHERE] Invoked by `extract_features_from_line` for each comma-separated value in a feature list.
/// [USAGE] Chamada iterativamente para cada entrada de feature, garantindo que ativações de crates via barra diagonal sejam rastreadas e auditadas.
fn add_feature_entry(entry: &str, active_features: &mut Vec<String>) {
    let clean = entry.trim().trim_matches(|c| c == '"' || c == '\'');
    if clean.is_empty() { return; }
    
    active_features.push(clean.to_string());
    
    // Detect hidden activation via crate/feature syntax
    if let Some(crate_name) = clean.split('/').next() {
        if crate_name != clean {
            active_features.push(crate_name.to_string());
        }
    }
}

/// [WHAT] Verifies the purity of a dependency marked as optional against the set of default features that will be compiled into the final industrial binary.
/// [WHY] Para assegurar que nenhum código de terceiros seja ativado por engano no binário final através de features padrão, mantendo a árvore de dependências limpa e segura.
/// [WHERE] Internal pass for Tier 3 validation in deps.rs, focusing on sanitized dependency injection and feature-gate purity for the Elite governance gatekeeper.
/// [USAGE] Called when a dependency contains the 'optional = true' attribute, cross-referencing it with the resolved default features manifest to identify hidden activations.
fn verify_optional_purity(ctx: &mut DepsAuditCtx<'_>, idx: usize, name: &str, activated: &[String]) {
    let is_force_activated = activated.iter().any(|act| act == name || act.contains(&format!("dep:{}", name)));
    if is_force_activated {
        ctx.violations.push(AuditViolation {
            file_path: ctx.path.to_string(),
            line: idx + 1,
            rule_id: crate::governance::rule_id("rule_deps_purity", name),
            message: format!("{}: Crate '{}' is force-activated in default features.", messages::IMPURE_ACTIVATION, name),
            snippet: Some(format!("{} (Activated by default)", name)),
            quick_fix: None,
        });
    }
}

/// [WHAT] Extracts the raw internal name of a geode from its relative Cargo.toml path declaration to enable architectural identification and layering checks.
/// [WHY] Para permitir que o sistema de governança traduza caminhos de arquivos em nomes de projetos para aplicação consistente de regras de camadas em toda a árvore de arquivos.
/// [WHERE] Path normalization utility in deps.rs, used during the dependency resolution phase to map file system locations back to defined architectural components.
/// [USAGE] Called by Tier 1 validation to translate paths into project names, facilitating the comparison of current and dependent geode layers during the governance scan cycle.
#[allow(dead_code)]
fn extract_internal_geode_name(line: &str) -> String {
    // [Elite Regex-like Split] Procura pela última ocorrência de path = "../" ou similar
    if let Some(pos) = line.find("path = \"") {
        let path_part = &line[pos + 8 ..].trim_matches('"');
        return elite_resolve_or(path_part.split('/').last(), "unknown").to_string();
    }
    "unknown".to_string()
}
