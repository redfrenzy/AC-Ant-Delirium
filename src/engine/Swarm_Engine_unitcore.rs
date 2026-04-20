use std::fs;
use std::path::Path;
use std::collections::{HashMap, HashSet};
use crate::structs::{AuditConfig, GovernanceAxiom, AuditViolation, AuditContext};
use crate::parser::CodeScanner;

/// [INTERNAL VACUUM]
/// Executa um único axioma contra sua jurisdição resolvida.
pub fn execute_axiom(axiom: &dyn GovernanceAxiom, config: &AuditConfig) -> Vec<AuditViolation> {
    let mut axiom_violations = Vec::new();
    let jurisdiction = axiom.resolve_jurisdiction(&config.hierarchy);
    
    // Se não houver jurisdição, o axioma não tem nada a patrulhar neste cluster.
    if jurisdiction.is_empty() {
        return axiom_violations;
    }

    for file_path in jurisdiction {
        // [READ] Tentamos ler o conteúdo do arquivo
        if let Ok(content) = fs::read_to_string(&file_path) {
            let mut local_violations = Vec::new();
            let functions = CodeScanner::scan_blocks(&content);
            let tag_separator = crate::engine::Swarm_Engine_unit::SwarmEngine::resolve_tag_separator(config);
            let (detected_dna, taxonomy_path) = resolve_file_taxonomy(config, &file_path, &tag_separator);
            
            {
                let mut ctx = AuditContext {
                    config,
                    violations: &mut local_violations,
                    file_path: &file_path,
                    source: &content,
                    functions: &functions,
                    detected_dna,
                    taxonomy_path,
                };
                
                // [EXECUTE] Disparamos a patrulha brutal
                axiom.execute(&mut ctx);
            }
            
            // [COLLECT] Agregamos as violações deste contexto
            axiom_violations.extend(local_violations);
        }
    }
    
    axiom_violations
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

    let (detected_dna, taxonomy_path) = match find_taxonomy_for_stem(config, &scope_root, stem, tag_separator) {
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
        let has_concrete_properties = node.properties.keys().any(|key| {
            key.as_str() != "is_requirement" && !key.starts_with("requirement::")
        });
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

    branch_children.into_iter()
        .filter(|(_, children)| children.len() > 1)
        .max_by(|(left_path, left_children), (right_path, right_children)| {
            let left_score = (left_path.matches('.').count(), left_children.len(), left_path.len());
            let right_score = (right_path.matches('.').count(), right_children.len(), right_path.len());
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

    branch_children.into_iter()
        .filter(|(_, children)| children.len() > 1)
        .max_by(|(left_path, left_children), (right_path, right_children)| {
            let left_score = (left_path.matches('.').count(), left_children.len(), left_path.len());
            let right_score = (right_path.matches('.').count(), right_children.len(), right_path.len());
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

    let mut candidates: Vec<String> = config.hierarchy.nodes.iter()
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
        .filter(|path| scope_root.is_empty() || *path == scope_root || path.starts_with(&scope_prefix))
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

    let mut candidates: Vec<(String, String)> = config.hierarchy.nodes.iter()
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
        right_dna
            .len()
            .cmp(&left_dna.len())
            .then_with(|| right_path.matches('.').count().cmp(&left_path.matches('.').count()))
    });

    candidates.into_iter().next().map(|(path, dna)| (dna, path))
}
