use crate::AuditViolation;
use crate::config::AuditConfig;

/// [WHAT] Orchestrates dependency governance for Cargo.toml files using a 3-Tier Classification Engine.
/// [WHY] To enforce architectural layering and dependency purity at the build level for Elite geodes.
/// [WHERE] Called by the audit_code entry point specifically for dependency files.
pub fn verify(path: &str, source: &str, config: &AuditConfig, violations: &mut Vec<AuditViolation>) {
    if !path.ends_with("Cargo.toml") { return; }

    let activated = resolve_default_deps(source);
    let mut in_deps = false;
    let mut acc = String::new();

    for (idx, line) in source.lines().enumerate() {
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
        process_line_with_purity_checks(idx, stripped, &mut acc, &activated, config, violations);
    }
}

/// [WHAT] determines whether a specific section header in a Cargo.toml file represents a block of primary production dependencies that should be subject to governance auditing. This function performs string-based matching to identify headers containing the 'dependencies' keyword while explicitly excluding sections related to dev-dependencies or build-dependencies to ensure that only the final binary's dependency graph is validated against purity standards.
/// [WHY] To focus the auditing resources on the critical components that define the geode's runtime footprint and security posture. By filtering out non-production dependencies, the engine avoids raising false positives for developer tools and test utilities that do not affect the final Elite WASM asset, thereby maintaining the high signal-to-noise ratio required for automated AI compliance monitoring within the Aesthetic Core ecosystem.
/// [WHERE] Called internally by the primary `verify` loop in `deps.rs` whenever a line starting and ending with square brackets is encountered, acting as a structural gatekeeper for the subsequent dependency classification and verification phases.
fn is_active_dependency_section(header: &str) -> bool {
    header.contains("dependencies") 
        && !header.contains("dev-dependencies") 
        && !header.contains("build-dependencies")
}



/// [WHAT] Processes an individual line from a Cargo.toml dependency section, handling both simple single-line key-value pairs and complex multi-line inline table definitions. This function manages an accumulation buffer that allows the parser to reconstruct full dependency statements that span across multiple lines of text, ensuring that the structural integrity of the TOML syntax is preserved before it is passed to the tiered classification engine for final validation.
/// [WHY] To provide a robust, zero-dependency parsing mechanism that can handle the structural variability of Cargo's dependency declarations without relying on heavy external crates like 'toml' or 'serde'. This manual approach maintains the geode's minimal footprint and allows for granular, line-by-line governance reporting that maps violations directly back to the original source location in a way that is immediately actionable for AI remediation agents.
/// [WHERE] Executed for every line within an active dependency section during the file scanning phase, serving as the bridge between raw file content and the high-level semantic validation logic of the 3-Tier Classification Engine.
fn process_line_with_purity_checks(
    idx: usize, 
    line: &str, 
    acc: &mut String, 
    activated: &[String], 
    config: &AuditConfig, 
    violations: &mut Vec<AuditViolation>
) {
    if line.contains('{') && !line.contains('}') {
        acc.push_str(line);
        return;
    }

    if acc.is_empty() {
        return handle_full_dep_line(idx, line.to_string(), activated, config, violations);
    }
    let full_line = format!("{} {}", acc, line);
    handle_full_dep_line(idx, full_line, activated, config, violations)
}

/// [WHAT] Finalize and verify a complete dependency line.
/// [WHY] Extracted to avoid else blocks in line processing.
/// [WHERE] Internal helper for process_line_with_purity_checks.
/// [WHAT] Orchestrates the final reconstruction and verification of a completed dependency declaration. It performs the splitting of the raw TOML line into its constituent key-value components, isolating the dependency name and its corresponding configuration table or version string, and prepares these elements for the subsequent tiered classification pass where architectural and purity constraints are enforced.
/// [WHY] To decouple the structural line-accumulation logic from the semantic dependency verification logic, maintaining a clear separation of concerns within the auditor. This modularity ensures that the parsing layer remains focused on TOML syntax while the verification layer focuses on governance policy, facilitating easier maintenance and allowing for independent updates to either the parsing strategy or the architectural layering rules.
/// [WHERE] Invoked as the terminal step in the dependency line processing pipeline in `deps.rs`, either immediately for single-line declarations or after a multi-line buffer has been successfully accumulated and reconstructed into a single coherent statement.
fn handle_full_dep_line(
    idx: usize, 
    full_line: String, 
    activated: &[String], 
    config: &AuditConfig, 
    violations: &mut Vec<AuditViolation>
) {
    let parts: Vec<&str> = full_line.splitn(2, '=').collect();
    if parts.len() < 2 { return; }

    let name = parts[0].trim().trim_matches(|c| c == '"' || c == '\'');
    let val = parts[1].trim();

    classify_and_verify_dep(idx, name, val, &full_line, activated, config, violations);
}

/// [WHAT] Evaluates a fully parsed dependency definition and assigns it to one of the four governance Tiers: LOCAL, ALLOWED, OPTIONAL, or IMPURE. This function implements the core architectural shield of the Geode, checking for architectural layering violations, whitelisted crate membership, and hidden feature-set activations to ensure that the final dependency graph remains strictly within the bounds of Elite governance.
/// [WHY] To enforce a zero-violation policy on third-party code and architectural layering without sacrificing the flexibility needed for legitimate internal or whitelisted dependencies. By classifying each dependency into a clear tier, the engine can provide specific, actionable recovery context for each type of violation, guiding the developer or AI agent toward a compliant resolution that maintains the project's long-term maintainability and security integrity.
/// [WHERE] Called at the end of the dependency processing pipeline in `deps.rs` once a full, potentially multi-line dependency declaration has been successfully reconstructed and split into its constituent key-value components.
fn classify_and_verify_dep(
    idx: usize, 
    name: &str, 
    val: &str, 
    full: &str, 
    activated: &[String], 
    config: &AuditConfig, 
    violations: &mut Vec<AuditViolation>
) {
    // TIER 1: LOCAL (Layering)
    if full.contains("path = \"../") {
        verify_architectural_layering(idx, name, full, violations);
        return;
    }

    // TIER 2: ALLOWED (Whitelist)
    if config.rules.allowed_crates.contains(&name.to_string()) { return; }
    if name == "std" || name == "core" || name == "alloc" { return; }

    // TIER 3: OPTIONAL (Sanitized)
    if val.contains("optional = true") {
        verify_optional_purity(idx, name, activated, violations);
        return;
    }

    // TIER 4: IMPURE (Violation)
    violations.push(AuditViolation {
        line: idx + 1,
        rule_id: "rule_deps",
        message: format!("Impure Dependency: External crate '{}' is restricted. Move to allowed_crates or mark as optional.", name),
        snippet: Some(full.to_string()),
        quick_fix: None,
    });
}

/// [WHAT] Validates that a path-based internal dependency adheres to the strict architectural layering hierarchy established for the Aesthetic Core ecosystem. It extracts the geode's name from the relative path, determines its corresponding layer (from L0 to L6), and compares it against the current geode's level to ensure that low-level infrastructure components do not inadvertently depend on higher-level feature modules or platform-specific implementations.
/// [WHY] To prevent cyclic dependency graphs and maintain a clean, directed acyclic graph (DAG) of architectural dependencies across the entire project. This layering rigour is essential for creating a modular system where core geodes remain lightweight and platform-agnostic, enabling efficient WASM compilation and ensuring that security-critical audit engines can be maintained and verified independently of the high-level application logic.
/// [WHERE] Invoked by the classification engine during the Tier 1 validation phase whenever a dependency line is identified as a local geode reference containing the relative path prefix designated for internal project components.
fn verify_architectural_layering(idx: usize, _name: &str, full: &str, violations: &mut Vec<AuditViolation>) {
    let dep_name = extract_internal_geode_name(full);
    let current_geode_layer = 0; // ac_ant_delirium is L0
    let dep_layer = map_geode_to_layer(&dep_name);

    if dep_layer > current_geode_layer && dep_layer != 99 {
        violations.push(AuditViolation {
            line: idx + 1,
            rule_id: "rule_layering",
            message: format!("Layering Violation: L{} geode cannot depend on '{}' (L{}).", current_geode_layer, dep_name, dep_layer),
            snippet: Some(full.to_string()),
            quick_fix: None,
        });
    }
}

/// [WHAT] Maps a specific geode project name to its assigned architectural layer within the standardized hierarchy that governs all Aesthetic Core components. It uses an exhaustive match-based classification to assign levels ranging from L0 (Core Audit Engines and Infrastructure) to L6 (Platform-Specific Adapters), providing a stable numerical reference point for the layering enforcement algorithm.
/// [WHY] To provide a centralized and deterministic definition of the project's architectural structure that is accessible to all governance tools. This mapping ensures that layering checks remain consistent throughout the codebase, preventing architectural drift and guaranteeing that the project's modularity is enforced at the build level, which is critical for maintaining long-term project health and facilitating automated reasoning about component isolation.
/// [WHERE] Used as an internal utility function by the layering verification logic to translate geode identifiers into comparable architectural tiers for dependency graph validation.
fn map_geode_to_layer(name: &str) -> i32 {
    match name {
        "ac_ant_delirium" | "ant_delirium" => 0,
        "ac_png" | "ac_jpeg" | "ac_gif" | "ac_webp" | "ac_pnm" | "ac_ico" | "ac_hdr" | "ac_openexr" | "ac_farbfeld" | "ac_avif" | "ac_qoi" | "ac_tga" | "ac_bmp" | "ac_tiff" => 1,
        "ac_image" => 2,
        "ac_core" => 3,
        "ac_material" => 4,
        "ac_components" => 5,
        "ac_platform_linux" | "ac_platform_android" | "ac_platform_windows" => 6,
        _ => 99,
    }
}

/// [WHAT] Resolves the set of activated features and implicit dependencies from the project's Cargo.toml source. It orchestrates the discovery of the `[features]` section and delegates the granular parsing of the `default` feature set to specialized sub-collectors.
/// [WHY] To provide a unified list of activated components that must be audited for architectural purity and security compliance.
/// [WHERE] Called as a preliminary discovery phase within the main `verify` function in `deps.rs`.
fn resolve_default_deps(source: &str) -> Vec<String> {
    let mut active_features = Vec::new();
    let mut in_features = false;

    for line in source.lines() {
        let stripped = line.trim();
        if stripped.starts_with("[features]") { in_features = true; continue; }
        if stripped.starts_with('[') { in_features = false; continue; }
        if !in_features || !stripped.starts_with("default =") { continue; }
        extract_features_from_line(stripped, &mut active_features);
    }
    active_features
}

/// [WHAT] Extracts individual feature names and implicit dependency activations from a formatted TOML feature list line. This helper normalizes the raw line and iterates over its comma-separated entries to populate the active feature set.
/// [WHY] To reduce the cognitive complexity and nesting depth of the primary feature resolution logic, satisfying Elite governance standards.
/// [WHERE] Called internally by `resolve_default_deps` during the manifest parsing phase in `deps.rs`.
fn extract_features_from_line(line: &str, active_features: &mut Vec<String>) {
    let list = match line.split('=').nth(1) { Some(l) => l.trim(), None => "" };
    let inner = list.trim_matches(|c| c == '[' || c == ']');
    for entry in inner.split(',') {
        add_feature_entry(entry, active_features);
    }
}

/// [WHAT] Normalizes a single feature entry and identifies implicit dependency activations. It handles quote stripping and detects tokens that trigger optional dependencies via Cargo's feature activation syntax.
/// [WHY] To provide a granular and flat implementation of feature processing, avoiding the 'Cognitive Abyss' while ensuring all hidden dependency activations are correctly identified.
/// [WHERE] Invoked by `extract_features_from_line` for each comma-separated value in a feature list.
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

/// [WHAT] Verifies the purity of a dependency marked as optional by checking it against the list of features activated by default. This function ensures that any external crate marked with `optional = true` is truly gated behind an opt-in feature and is not being force-activated, which would effectively treat it as a mandatory production dependency in violation of the geode's purity mandate.
/// [WHY] To maintain strict control over the geode's runtime dependencies and ensure that no third-party code is included in the final binary unless it is explicitly requested by the end-user via a feature flag. This enforcement prevents 'crate creep' and ensures that the Elite governance standard is not bypassed by the clever use of feature toggles, maintaining the security and performance guarantees of the Aesthetic Core platform.
/// [WHERE] Triggered during the Tier 3 validation phase of the classification engine whenever a dependency declaration containing the `optional = true` attribute is successfully parsed and resolved.
fn verify_optional_purity(idx: usize, name: &str, activated: &[String], violations: &mut Vec<AuditViolation>) {
    let is_force_activated = activated.iter().any(|act| act == name || act.contains(&format!("dep:{}", name)));
    if is_force_activated {
        violations.push(AuditViolation {
            line: idx + 1,
            rule_id: "rule_deps_purity",
            message: format!("Impure Activation: Crate '{}' is marked optional but ACTIVATED in default features.", name),
            snippet: Some(format!("{} (Activated by default)", name)),
            quick_fix: None,
        });
    }
}

/// [WHAT] Extracts the raw internal name of a geode from its relative file system path definition within a Cargo.toml manifest. It parses the string between the standardized geode directory prefix and the closing quote character, normalizing the path-based reference into a clean identifier that can be correlated with the project's architectural layering definitions.
/// [WHY] To enable accurate architectural layering enforcement using the path-based dependency syntax required by Cargo for multi-geode workspace development. By normalizing these paths into project names, the auditor can apply consistent rules regardless of how a geode is referenced, ensuring that the layering hierarchy is respected even when modules are moved or renamed within the project's directory structure.
/// [WHERE] Used as a string-processing utility within the TIER 1 classification logic to translate relative Cargo paths into geode identifiers for hierarchical layer lookup.
fn extract_internal_geode_name(line: &str) -> String {
    line.split("path = \"../").nth(1)
        .and_then(|s| s.split('"').next())
        .map_or("unknown".to_string(), |n| n.to_string())
}
