use std::fs;
use std::path::Path;
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
            let (detected_dna, taxonomy_path) = resolve_file_taxonomy(config, &file_path);
            
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

pub(crate) fn resolve_file_taxonomy(config: &AuditConfig, file_path: &str) -> (String, String) {
    let scope_root = resolve_scope_root(config);
    let tag_separator = config.hierarchy.resolve_parameter("tag_separator")
        .unwrap_or_else(|| "_".to_string());
    let stem = Path::new(file_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    let detected_dna = match stem.rsplit_once(&tag_separator) {
        Some((_, suffix)) if !suffix.is_empty() => suffix.to_string(),
        _ => String::new(),
    };

    let taxonomy_path = if detected_dna.is_empty() {
        scope_root
    } else {
        find_taxonomy_path_for_dna(config, &scope_root, &detected_dna)
            .unwrap_or(scope_root)
    };

    (detected_dna, taxonomy_path)
}

fn resolve_scope_root(config: &AuditConfig) -> String {
    let jurisdiction_paths: Vec<Vec<&str>> = config.hierarchy.nodes.iter()
        .filter(|(_, node)| node.properties.contains_key("jurisdiction"))
        .map(|(path, _)| path.split('.').collect())
        .collect();

    if jurisdiction_paths.is_empty() {
        return String::new();
    }

    let mut prefix = jurisdiction_paths[0].clone();
    for path in jurisdiction_paths.iter().skip(1) {
        let common_len = prefix.iter()
            .zip(path.iter())
            .take_while(|(left, right)| left == right)
            .count();
        prefix.truncate(common_len);
    }

    prefix.join(".")
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

    let mut candidates: Vec<String> = config.hierarchy.nodes.keys()
        .filter(|path| path.rsplit('.').next() == Some(detected_dna))
        .filter(|path| scope_root.is_empty() || *path == scope_root || path.starts_with(&scope_prefix))
        .cloned()
        .collect();
    candidates.sort_by_key(|path| path.matches('.').count());
    candidates.into_iter().next()
}
