use crate::structs::{DynamicNode, HierarchyConfig, SearchFiles};
use std::collections::HashMap;
use std::fs;

/// [WHAT] Abstract Hierarchy Motor & Query Engine.
pub(crate) struct HierarchyResolver;

impl SearchFiles {
    /// [AESTHETIC CORE - PHYSICAL SCANNER]
    /// Varre recursivamente o sistema de arquivos para materializar caminhos abstratos em arquivos reais.
    pub(crate) fn scan_physical_jurisdiction(
        base_path: &str,
        project_root: &std::path::Path,
        ignored_patterns: &[String],
    ) -> Vec<String> {
        let mut files = Vec::new();
        let path = std::path::Path::new(base_path);

        if should_ignore_path(path, project_root, ignored_patterns) {
            return files;
        }

        if path.is_file() {
            files.push(relativize_for_display(path, project_root));
        } else if path.is_dir() {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let entry_path = entry.path();
                    if should_ignore_path(&entry_path, project_root, ignored_patterns) {
                        continue;
                    }

                    if entry_path.is_dir() {
                        // Mergulho recursivo no abismo de pastas
                        let path_str = entry_path.to_string_lossy().into_owned();
                        files.extend(Self::scan_physical_jurisdiction(
                            &path_str,
                            project_root,
                            ignored_patterns,
                        ));
                    } else {
                        // O motor ativo audita apenas código Rust fonte.
                        if entry_path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                            files.push(relativize_for_display(&entry_path, project_root));
                        }
                    }
                }
            }
        }
        files
    }
}

/// [AESTHETIC CORE - DISCOVERY INTERNAL]
/// Resolve a jurisdição física (lista de arquivos) para um ID atômico através da Descoberta Abissal.
pub(crate) fn resolve_jurisdiction(hierarchy: &HierarchyConfig, id: &str) -> Vec<String> {
    let functionalities = hierarchy.search_functionalities(id);
    let mut physical_files = Vec::new();
    let project_root = runtime_project_root(hierarchy);
    let ignored_patterns = resolve_ignored_patterns(hierarchy);

    for (_, jurisdictions) in functionalities {
        for folder in jurisdictions {
            let resolved = resolve_runtime_path(&project_root, &folder);
            physical_files.extend(SearchFiles::scan_physical_jurisdiction(
                &resolved.to_string_lossy(),
                &project_root,
                &ignored_patterns,
            ));
        }
    }
    physical_files.sort();
    physical_files.dedup();
    physical_files
}

fn runtime_project_root(hierarchy: &HierarchyConfig) -> std::path::PathBuf {
    hierarchy
        .search_params("runtime.project_root")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        })
}

fn resolve_runtime_path(project_root: &std::path::Path, value: &str) -> std::path::PathBuf {
    let candidate = std::path::Path::new(value);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    }
}

fn relativize_for_display(path: &std::path::Path, project_root: &std::path::Path) -> String {
    path.strip_prefix(project_root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
}

fn resolve_ignored_patterns(hierarchy: &HierarchyConfig) -> Vec<String> {
    let mut patterns = Vec::new();

    for node in hierarchy.nodes.values() {
        let ignore_enabled = node
            .properties
            .get("ignore")
            .map(|value| is_truthy(value))
            .unwrap_or(false);
        if !ignore_enabled {
            continue;
        }

        if let Some(pattern) = node.properties.get("match") {
            let trimmed = pattern.trim().trim_matches('"');
            if !trimmed.is_empty() {
                patterns.push(trimmed.replace('\\', "/"));
            }
        }
    }

    patterns.sort();
    patterns.dedup();
    patterns
}

fn should_ignore_path(
    path: &std::path::Path,
    project_root: &std::path::Path,
    ignored_patterns: &[String],
) -> bool {
    if ignored_patterns.is_empty() {
        return false;
    }

    let relative = relativize_for_display(path, project_root);
    let absolute = path.to_string_lossy().replace('\\', "/");

    ignored_patterns.iter().any(|pattern| {
        path_matches_ignore_pattern(&relative, pattern)
            || path_matches_ignore_pattern(&absolute, pattern)
    })
}

fn path_matches_ignore_pattern(path: &str, pattern: &str) -> bool {
    let normalized_path = path.trim_matches('/');
    let normalized_pattern = pattern.trim_matches('/');

    if normalized_path.is_empty() || normalized_pattern.is_empty() {
        return false;
    }

    if normalized_pattern.contains('/') {
        return normalized_path == normalized_pattern
            || normalized_path.starts_with(&format!("{}/", normalized_pattern))
            || normalized_path.ends_with(&format!("/{}", normalized_pattern))
            || normalized_path.contains(&format!("/{}/", normalized_pattern));
    }

    normalized_path
        .split('/')
        .any(|segment| segment == normalized_pattern)
}

fn is_truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "false" | "0" | "off" | "no" | "disabled"
    )
}

impl HierarchyResolver {
    const REQUIREMENT_PREFIX: &'static str = "requirement::";

    /// [AESTHETIC CORE - SWARM MERGE]
    pub(crate) fn merge_config(base: &mut HierarchyConfig, extension: HierarchyConfig) {
        for (path, ext_node) in extension.nodes {
            let base_node = base.nodes.entry(path).or_insert_with(DynamicNode::default);
            for (k, v) in ext_node.properties {
                base_node.properties.insert(k, v);
            }
        }
    }

    /// [AESTHETIC CORE - DYNAMIC QUERY ENGINE]
    pub(crate) fn search_params(config: &HierarchyConfig, path: &str) -> Option<String> {
        if let Some(node) = config.nodes.get(path) {
            let mut result = String::new();
            let mut keys: Vec<&String> = node.properties.keys().collect();
            keys.sort();
            for k in keys {
                if let Some(v) = node.properties.get(k) {
                    result.push_str(&format!("{} = {}\n", k, v));
                }
            }
            return if result.is_empty() {
                None
            } else {
                Some(result)
            };
        }

        if let Some(idx) = path.rfind('.') {
            let parent_path = &path[..idx];
            let attr = &path[idx + 1..];
            if let Some(node) = config.nodes.get(parent_path) {
                if let Some(val) = node.properties.get(attr) {
                    return Some(val.clone());
                }
            }
        }
        None
    }

    /// [WHAT] Retorna todos os caminhos da hierarquia que terminam com o identificador fornecido (IDENTIDADE).
    pub(crate) fn find_paths_by_id(config: &HierarchyConfig, id: &str) -> Vec<String> {
        let mut results = Vec::new();
        let dot_id = format!(".{}", id);

        for key in config.nodes.keys() {
            let key_str: &String = key;
            if key_str == id || key_str.ends_with(&dot_id) {
                results.push(key_str.clone());
            }
        }
        results
    }

    pub(crate) fn find_requirements(config: &HierarchyConfig, identity_path: &str) -> Vec<String> {
        let mut requirements = Vec::new();

        if let Some(node) = config.nodes.get(identity_path) {
            requirements.extend(node.properties.keys().filter_map(|key| {
                key.strip_prefix(Self::REQUIREMENT_PREFIX)
                    .map(|value| value.to_string())
            }));
        }

        let legacy_prefix = format!("{}.", identity_path);
        requirements.extend(config.nodes.keys().filter_map(|path| {
            path.strip_prefix(&legacy_prefix)
                .map(|value| value.to_string())
        }));

        requirements.sort();
        requirements.dedup();
        requirements
    }
    /// [WHAT] Traduz um ID atômico para seu path taxonômico.
    pub(crate) fn resolve_id(config: &HierarchyConfig, id: &str) -> String {
        let _ = config;
        id.to_string()
    }

    /// [AESTHETIC CORE - SOVEREIGN PARAMETER RESOLVER]
    /// Resolve um ID de parâmetro por Identidade Direta e por atributos concretos.
    pub(crate) fn resolve_parameter(config: &HierarchyConfig, id: &str) -> Option<String> {
        Self::resolve_parameter_binding(config, id).map(|(_, value)| value)
    }

    /// [WHAT] Resolve um ID de parâmetro e informa o caminho concreto que materializou o valor final.
    pub(crate) fn resolve_parameter_binding(
        config: &HierarchyConfig,
        id: &str,
    ) -> Option<(String, String)> {
        let path = Self::resolve_id(config, id);

        // [1] Tentativa de busca direta pelo path/id
        if let Some(val) = Self::search_concrete_value(config, &path) {
            return Some((path, val));
        }

        // [1.5] IDs compostos como "target.core" ou "unit.type_logic"
        if let Some((resolved_path, val)) = Self::resolve_compound_identifier(config, id) {
            return Some((resolved_path, val));
        }

        // [1.75] Propriedade concreta global/ramificada antes de saltar para nós-identidade
        if let Some((resolved_path, val)) = Self::find_property_value(config, id) {
            return Some((resolved_path, val));
        }

        // [2] [ABYSSAL JUMP] Tentativa de busca por Identidade se for um ID atômico
        let discovery_paths = Self::find_paths_by_id(config, id);
        for p in discovery_paths {
            if let Some(val) = Self::search_concrete_value(config, &p) {
                return Some((p, val));
            }
        }

        None
    }

    fn resolve_compound_identifier(config: &HierarchyConfig, id: &str) -> Option<(String, String)> {
        let (parent_id, attr) = id.rsplit_once('.')?;
        let mut candidates = Self::find_paths_by_id(config, parent_id);
        candidates.sort_by_key(|path| path.matches('.').count());

        for candidate in candidates {
            let candidate_path = format!("{}.{}", candidate, attr);
            if let Some(val) = Self::search_concrete_value(config, &candidate_path) {
                return Some((candidate_path, val));
            }
        }

        None
    }

    pub(crate) fn resolve_taxonomy_parameter(
        config: &HierarchyConfig,
        taxonomy_path: &str,
        parameter: &str,
    ) -> Result<(String, String), String> {
        let mut current = taxonomy_path.trim().trim_matches('.').to_string();
        if current.is_empty() {
            return Err(format!(
                "user not define param \"{}\" in dna \"{}\"",
                parameter, taxonomy_path
            ));
        }

        loop {
            let parameter_path = format!("{}.{}", current, parameter);
            if let Some(value) = Self::search_params(config, &parameter_path) {
                return Ok((current, value));
            }

            match current.rsplit_once('.') {
                Some((parent, _)) => current = parent.to_string(),
                None => break,
            }
        }

        Err(format!(
            "user not define param \"{}\" in dna \"{}\"",
            parameter, taxonomy_path
        ))
    }

    /// [AESTHETIC CORE - ABYSSAL JUMP RESOLVER]
    /// Resolve recursivamente ponteiros taxonômicos até encontrar o valor literal final.
    pub(crate) fn resolve_taxonomic_jump(config: &HierarchyConfig, value: &str) -> Vec<String> {
        // Se o valor parece um path (contém pontos), tentamos pular
        if value.contains('.') {
            if let Some(jumped_value) = Self::search_params(config, value) {
                return Self::resolve_taxonomic_jump(config, &jumped_value);
            }

            // Tentativa de salto por ID se o path falhar
            let discovery_paths = Self::find_paths_by_id(config, value);
            if let Some(p) = discovery_paths.get(0) {
                if let Some(jumped_value) = Self::search_params(config, p) {
                    return Self::resolve_taxonomic_jump(config, &jumped_value);
                }
            }
        }
        Self::parse_industrial_array(value)
    }

    /// [WHAT] Converte strings de TOML que representam vetores ["a", "b"] em listas reais.
    fn parse_industrial_array(input: &str) -> Vec<String> {
        let trimmed = input.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            trimmed[1..trimmed.len() - 1]
                .split(',')
                .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![trimmed.to_string()]
        }
    }

    fn search_concrete_value(config: &HierarchyConfig, path: &str) -> Option<String> {
        if let Some(node) = config.nodes.get(path) {
            let mut concrete: Vec<(&String, &String)> = node
                .properties
                .iter()
                .filter(|(key, _)| {
                    key.as_str() != "is_requirement" && !key.starts_with(Self::REQUIREMENT_PREFIX)
                })
                .collect();
            concrete.sort_by(|a, b| a.0.cmp(b.0));

            if concrete.len() == 1 {
                return Some(concrete[0].1.clone());
            }

            if concrete.len() > 1 {
                let mut result = String::new();
                for (key, value) in concrete {
                    result.push_str(&format!("{} = {}\n", key, value));
                }
                return Some(result);
            }
        }

        if let Some(idx) = path.rfind('.') {
            let parent_path = &path[..idx];
            let attr = &path[idx + 1..];
            if let Some(node) = config.nodes.get(parent_path) {
                if let Some(val) = node.properties.get(attr) {
                    return Some(val.clone());
                }
            }
        }

        None
    }

    fn find_property_value(config: &HierarchyConfig, property: &str) -> Option<(String, String)> {
        let mut matches: Vec<(&String, &String)> = config
            .nodes
            .iter()
            .filter_map(|(path, node)| node.properties.get(property).map(|value| (path, value)))
            .collect();
        matches.sort_by_key(|(path, _)| path.matches('.').count());
        matches
            .first()
            .map(|(path, value)| (format!("{}.{}", path, property), (*value).clone()))
    }

    /// [AESTHETIC CORE - TRIPLE SEARCH ORCHESTRATOR]
    /// Centraliza a interrogação abissal em 3 estágios: Identidade -> Requisição -> Resolução.
    pub(crate) fn search_functionalities(
        config: &HierarchyConfig,
        master_id: &str,
    ) -> Vec<(String, Vec<String>)> {
        let mut results = Vec::new();

        // [STAGE 1: IDENTITY] - "Quem sou eu?"
        let identity_paths = Self::find_paths_by_id(config, master_id);

        for geode_path in identity_paths {
            // [STAGE 2: REQUIREMENT] - "Do que eu preciso?"
            for question_id in Self::find_requirements(config, &geode_path) {
                // [STAGE 3: RESOLUTION] - "Qual a resposta (valor =)?"
                let local_requirement_path = format!("{}.{}", geode_path, question_id);
                let answer = Self::search_params(config, &local_requirement_path)
                    .map(|value| (local_requirement_path, value))
                    .or_else(|| Self::resolve_parameter_binding(config, &question_id));

                if let Some((_, answer_val)) = answer {
                    let physical_paths = Self::resolve_taxonomic_jump(config, &answer_val);
                    results.push((question_id, physical_paths));
                } else {
                    results.push((question_id, Vec::new()));
                }
            }
        }
        results
    }
}

pub(crate) struct AgnosticScanner;

impl AgnosticScanner {
    pub(crate) fn load_from_folder(dir_path: &str) -> HierarchyConfig {
        let mut master_config = HierarchyConfig::default();
        let paths = Self::collect_toml_paths_recursively(std::path::Path::new(dir_path));
        for path in paths {
            if let Ok(content) = fs::read_to_string(&path) {
                let current = Self::parse_brute(&content);
                HierarchyResolver::merge_config(&mut master_config, current);
            }
        }
        master_config
    }

    fn collect_toml_paths_recursively(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        Self::collect_toml_paths_worker(root, &mut paths);
        paths.sort();
        paths
    }

    fn collect_toml_paths_worker(root: &std::path::Path, paths: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    Self::collect_toml_paths_worker(&path, paths);
                } else if path.extension().map_or(false, |ext| ext == "toml") {
                    paths.push(path);
                }
            }
        }
    }

    pub(crate) fn parse_brute(input: &str) -> HierarchyConfig {
        let mut nodes: HashMap<String, DynamicNode> = HashMap::new();
        let mut current_node_path = String::new();
        for line in input.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let clean_line = trimmed.split('#').next().unwrap_or("").trim();
            if clean_line.is_empty() {
                continue;
            }

            if clean_line.starts_with('[') && clean_line.ends_with(']') {
                current_node_path = clean_line[1..clean_line.len() - 1].to_string();
                nodes
                    .entry(current_node_path.clone())
                    .or_insert_with(DynamicNode::default);
            } else if !current_node_path.is_empty() {
                if clean_line.contains('=') {
                    let parts: Vec<&str> = clean_line.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        let full_key = parts[0].trim();
                        let val_raw = parts[1].trim();
                        let val = val_raw.trim_matches('"').to_string();
                        if let Some(last_dot_idx) = full_key.rfind('.') {
                            let path_ext = &full_key[..last_dot_idx];
                            let real_key = &full_key[last_dot_idx + 1..];
                            let target_node_path = format!("{}.{}", current_node_path, path_ext);
                            let node = nodes
                                .entry(target_node_path)
                                .or_insert_with(DynamicNode::default);
                            node.properties.insert(real_key.to_string(), val);
                        } else {
                            if let Some(node) = nodes.get_mut(&current_node_path) {
                                node.properties.insert(full_key.to_string(), val);
                            }
                        }
                    }
                } else {
                    let node = nodes
                        .entry(current_node_path.clone())
                        .or_insert_with(DynamicNode::default);
                    let requirement_key =
                        format!("{}{}", HierarchyResolver::REQUIREMENT_PREFIX, clean_line);
                    node.properties.insert(requirement_key, "true".to_string());
                }
            }
        }
        HierarchyConfig { nodes }
    }
}
