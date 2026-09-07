use crate::parser::CodeScanner;
use crate::structs::{
    AuditConfig, AuditContext, AuditSourceFile, AuditViolation, GovernanceAxiom, ProjectContext,
    ProjectFileContext,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Default)]
pub(crate) struct PreparedProject {
    pub context: ProjectContext,
    pub path_index_millis: u128,
    pub file_read_millis: u128,
    pub taxonomy_millis: u128,
}

/// [INTERNAL VACUUM]
/// Executa um único axioma contra sua jurisdição resolvida.
#[allow(dead_code)]
pub fn execute_axiom(axiom: &dyn GovernanceAxiom, config: &AuditConfig) -> Vec<AuditViolation> {
    let mut axiom_violations = Vec::new();
    let jurisdiction = axiom.resolve_jurisdiction(&config.hierarchy);
    let tag_separator =
        crate::engine::Swarm_Engine_unit::SwarmEngine::resolve_tag_separator(config);
    let prepared = prepare_project_context(config, &jurisdiction, &tag_separator);

    // Se não houver jurisdição, o axioma não tem nada a patrulhar neste cluster.
    if prepared.context.files.is_empty() {
        return axiom_violations;
    }

    for file in prepared.context.files.iter() {
        let mut local_violations = Vec::new();
        let functions = CodeScanner::scan_blocks(&file.content);

        {
            let mut ctx = AuditContext {
                config,
                project: &prepared.context,
                violations: &mut local_violations,
                file_path: &file.path,
                source: &file.content,
                functions: &functions,
                detected_dna: file.detected_dna.clone(),
                taxonomy_path: file.taxonomy_path.clone(),
            };

            // [EXECUTE] Disparamos a patrulha brutal
            axiom.execute(&mut ctx);
        }

        // [COLLECT] Agregamos as violações deste contexto
        axiom_violations.extend(local_violations);
    }

    axiom_violations
}

pub(crate) fn prepare_project_context(
    config: &AuditConfig,
    audited_paths: &[String],
    tag_separator: &str,
) -> PreparedProject {
    let mut context = ProjectContext::default();

    let path_index_start = Instant::now();
    for path in collect_known_paths(config, audited_paths) {
        context.register_known_path(path);
    }
    let path_index_millis = path_index_start.elapsed().as_millis();

    let mut file_read_millis = 0u128;
    let mut taxonomy_millis = 0u128;

    for file_path in audited_paths {
        context.register_known_path(file_path);
        let physical_path = config.resolve_project_path(file_path);
        let read_start = Instant::now();
        let content = match fs::read_to_string(&physical_path) {
            Ok(content) => {
                file_read_millis += read_start.elapsed().as_millis();
                content
            }
            Err(_) => continue,
        };

        let taxonomy_start = Instant::now();
        let (detected_dna, taxonomy_path) = resolve_file_taxonomy(config, file_path, tag_separator);
        taxonomy_millis += taxonomy_start.elapsed().as_millis();

        context.insert_loaded_file(ProjectFileContext::new(
            file_path.clone(),
            content,
            detected_dna,
            taxonomy_path,
        ));
    }

    PreparedProject {
        context,
        path_index_millis,
        file_read_millis,
        taxonomy_millis,
    }
}

pub(crate) fn prepare_source_context(
    config: &AuditConfig,
    sources: &[AuditSourceFile],
    tag_separator: &str,
) -> PreparedProject {
    let path_index_start = Instant::now();
    let mut context = ProjectContext::default();
    for source in sources {
        context.register_known_path(&source.path);
    }
    let path_index_millis = path_index_start.elapsed().as_millis();
    let taxonomy_start = Instant::now();
    for source in sources {
        let (detected_dna, taxonomy_path) =
            resolve_file_taxonomy(config, &source.path, tag_separator);
        context.insert_loaded_file(ProjectFileContext::new(
            source.path.clone(),
            source.content.clone(),
            detected_dna,
            taxonomy_path,
        ));
    }
    let taxonomy_millis = taxonomy_start.elapsed().as_millis();
    PreparedProject {
        context,
        path_index_millis,
        file_read_millis: 0,
        taxonomy_millis,
    }
}

pub(crate) fn resolve_file_taxonomy(
    config: &AuditConfig,
    file_path: &str,
    tag_separator: &str,
) -> (String, String) {
    let scope_root = resolve_scope_root(config);
    let stem = Path::new(file_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    let (detected_dna, taxonomy_path) =
        match find_taxonomy_for_stem(config, &scope_root, stem, tag_separator) {
            Some((dna, path)) => (dna, path),
            None => {
                let detected_dna = match stem.rsplit_once(tag_separator) {
                    Some((_, suffix)) if !suffix.is_empty() => suffix.to_string(),
                    _ => String::new(),
                };
                let taxonomy_path = if detected_dna.is_empty() {
                    scope_root.clone()
                } else {
                    find_taxonomy_path_for_dna(config, &scope_root, &detected_dna)
                        .unwrap_or(scope_root.clone())
                };
                (detected_dna, taxonomy_path)
            }
        };

    (detected_dna, taxonomy_path)
}

fn resolve_scope_root(config: &AuditConfig) -> String {
    if let Some(preferred) = resolve_file_dna_scope_root(config) {
        return preferred;
    }

    let mut branch_children: HashMap<String, HashSet<String>> = HashMap::new();

    for (path, node) in config.hierarchy.nodes.iter() {
        let has_concrete_properties = node
            .properties
            .keys()
            .any(|key| key.as_str() != "is_requirement" && !key.starts_with("requirement::"));
        if !has_concrete_properties {
            continue;
        }

        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() < 2 {
            continue;
        }

        for idx in 1..parts.len() {
            let prefix = parts[..idx].join(".");
            let child = parts[idx].to_string();
            branch_children.entry(prefix).or_default().insert(child);
        }
    }

    branch_children
        .into_iter()
        .filter(|(_, children)| children.len() > 1)
        .max_by(|(left_path, left_children), (right_path, right_children)| {
            let left_score = (
                left_path.matches('.').count(),
                left_children.len(),
                left_path.len(),
            );
            let right_score = (
                right_path.matches('.').count(),
                right_children.len(),
                right_path.len(),
            );
            left_score.cmp(&right_score)
        })
        .map(|(path, _)| path)
        .unwrap_or_default()
}

fn resolve_file_dna_scope_root(config: &AuditConfig) -> Option<String> {
    let mut branch_children: HashMap<String, HashSet<String>> = HashMap::new();

    for (path, node) in config.hierarchy.nodes.iter() {
        let looks_like_file_dna = node.properties.contains_key("type_logic")
            || node.properties.contains_key("jurisdiction");
        if !looks_like_file_dna {
            continue;
        }

        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() < 2 {
            continue;
        }

        for idx in 1..parts.len() {
            let prefix = parts[..idx].join(".");
            let child = parts[idx].to_string();
            branch_children.entry(prefix).or_default().insert(child);
        }
    }

    branch_children
        .into_iter()
        .filter(|(_, children)| children.len() > 1)
        .max_by(|(left_path, left_children), (right_path, right_children)| {
            let left_score = (
                left_path.matches('.').count(),
                left_children.len(),
                left_path.len(),
            );
            let right_score = (
                right_path.matches('.').count(),
                right_children.len(),
                right_path.len(),
            );
            left_score.cmp(&right_score)
        })
        .map(|(path, _)| path)
}

fn find_taxonomy_path_for_dna(
    config: &AuditConfig,
    scope_root: &str,
    detected_dna: &str,
) -> Option<String> {
    let scope_prefix = if scope_root.is_empty() {
        String::new()
    } else {
        format!("{}.", scope_root)
    };

    let mut candidates: Vec<String> = config
        .hierarchy
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.properties.contains_key("type_logic")
                || node.properties.contains_key("jurisdiction")
                || node.properties.contains_key("manifest")
        })
        .filter(|(path, _)| {
            path.rsplit('.')
                .next()
                .map(|segment| segment == detected_dna)
                .unwrap_or(false)
        })
        .map(|(path, _)| path.clone())
        .filter(|path| {
            scope_root.is_empty() || *path == scope_root || path.starts_with(&scope_prefix)
        })
        .collect();
    candidates.sort_by_key(|path| path.matches('.').count());
    candidates.into_iter().next()
}

fn find_taxonomy_for_stem(
    config: &AuditConfig,
    scope_root: &str,
    stem: &str,
    tag_separator: &str,
) -> Option<(String, String)> {
    if stem.is_empty() {
        return None;
    }

    let scope_prefix = if scope_root.is_empty() {
        String::new()
    } else {
        format!("{}.", scope_root)
    };

    let mut candidates: Vec<(String, String)> = config
        .hierarchy
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.properties.contains_key("type_logic")
                || node.properties.contains_key("jurisdiction")
                || node.properties.contains_key("manifest")
        })
        .map(|(path, _)| {
            let dna = path.rsplit('.').next().unwrap_or_default().to_string();
            (path.clone(), dna)
        })
        .filter(|(path, dna)| {
            !dna.is_empty()
                && (scope_root.is_empty() || *path == scope_root || path.starts_with(&scope_prefix))
                && (stem == dna || stem.ends_with(&format!("{}{}", tag_separator, dna)))
        })
        .collect();

    // Prefer the most specific DNA token (longest segment) and then deeper taxonomy path.
    candidates.sort_by(|(left_path, left_dna), (right_path, right_dna)| {
        right_dna.len().cmp(&left_dna.len()).then_with(|| {
            right_path
                .matches('.')
                .count()
                .cmp(&left_path.matches('.').count())
        })
    });

    candidates.into_iter().next().map(|(path, dna)| (dna, path))
}

fn collect_known_paths(config: &AuditConfig, audited_paths: &[String]) -> HashSet<String> {
    let mut known_paths = HashSet::new();
    let mut parent_dirs = HashSet::new();

    for audited_path in audited_paths {
        let resolved = config.resolve_project_path(audited_path);
        let parent = resolved
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| config.project_root_path());
        parent_dirs.insert(parent);
        known_paths.insert(audited_path.clone());
    }

    for parent_dir in parent_dirs {
        if let Ok(entries) = fs::read_dir(&parent_dir) {
            for entry in entries.filter_map(|entry| entry.ok()) {
                let entry_path = entry.path();
                if should_index_known_path(&entry_path) {
                    known_paths.insert(config.display_path(&entry_path));
                }
            }
        }
    }

    known_paths
}

fn should_index_known_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("rs" | "md" | "json" | "toml")
    )
}
