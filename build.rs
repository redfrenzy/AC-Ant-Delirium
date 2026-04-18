use std::env;
use std::fs;
use std::path::Path;

/// [AESTHETIC CORE - TAXONOMY COMPILER]
/// Transmuta arquivos TOML em código Rust estático para performance Elite.
fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let config_dir = Path::new(&manifest_dir).join("ant_delirium_config");
    let dest_path = Path::new(&out_dir).join("generated_static_hierarchy.rs");

    let mut generated_code = String::new();
    generated_code.push_str("/// [GENERATED CODE] - Não edite manualmente.\n");
    generated_code.push_str("pub fn load_embedded_hierarchy(hierarchy: &mut crate::structs::HierarchyConfig) {\n");

    if config_dir.exists() {
        if let Ok(entries) = fs::read_dir(config_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "toml") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        compile_toml_to_rust(&content, &mut generated_code);
                    }
                }
            }
        }
    }

    generated_code.push_str("}\n");
    fs::write(&dest_path, generated_code).expect("Falha ao escrever taxonomia estática");

    // Re-run if configs change
    println!("cargo:rerun-if-changed=ant_delirium_config");
    println!("cargo:rerun-if-changed=build.rs");
}

fn compile_toml_to_rust(content: &str, output: &mut String) {
    let mut current_node = String::new();
    
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        
        // [IDENTIDADE] Seção de Mapeamento (Ignorada no Bake-in, pois a descoberta é dinâmica)
        if trimmed == "[mappings]" { continue; }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_node = trimmed[1..trimmed.len()-1].to_string();
            output.push_str(&format!("    hierarchy.nodes.entry(\"{}\".to_string()).or_insert_with(crate::structs::DynamicNode::default);\n", current_node));
        } else if !current_node.is_empty() {
            if trimmed.contains('=') {
                let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
                let key = parts[0].trim();
                let val = parts[1].trim().trim_matches('"').trim_matches('\'').replace("\"", "\\\"");
                
                if let Some(dot_idx) = key.rfind('.') {
                    let sub_node = &key[..dot_idx];
                    let real_key = &key[dot_idx+1..];
                    let full_path = format!("{}.{}", current_node, sub_node);
                    output.push_str(&format!("    hierarchy.nodes.entry(\"{}\".to_string()).or_insert_with(crate::structs::DynamicNode::default).properties.insert(\"{}\".to_string(), \"{}\".to_string());\n", full_path, real_key, val));
                } else {
                    output.push_str(&format!("    hierarchy.nodes.get_mut(\"{}\").map(|n| n.properties.insert(\"{}\".to_string(), \"{}\".to_string()));\n", current_node, key, val));
                }
            } else {
                // Registro de Requerimento (Stage 2)
                let full_path = format!("{}.{}", current_node, trimmed);
                output.push_str(&format!("    hierarchy.nodes.entry(\"{}\".to_string()).or_insert_with(crate::structs::DynamicNode::default).properties.insert(\"is_requirement\".to_string(), \"true\".to_string());\n", full_path));
            }
        }
    }
}
