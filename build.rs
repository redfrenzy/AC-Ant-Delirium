use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// [AESTHETIC CORE - TAXONOMY COMPILER]
/// Transmuta arquivos TOML em código Rust estático para performance Elite.
fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let config_dir = Path::new(&manifest_dir).join("ant_delirium_config");
    let dest_path = Path::new(&out_dir).join("generated_static_hierarchy.rs");

    emit_build_metadata(&manifest_dir);

    run_governance_build_hook(&manifest_dir);

    let mut generated_code = String::new();
    generated_code.push_str("/// [GENERATED CODE] - Não edite manualmente.\n");
    generated_code.push_str(
        "pub fn load_embedded_hierarchy(hierarchy: &mut crate::structs::HierarchyConfig) {\n",
    );

    if config_dir.exists() {
        let selected_profile = select_profile_folder(&config_dir);
        println!(
            "cargo:warning=[ac_ant_delirium] selected config profile: {}",
            selected_profile.display()
        );
        for path in collect_toml_files(&selected_profile) {
            if let Ok(content) = fs::read_to_string(&path) {
                compile_toml_to_rust(&content, &mut generated_code);
            }
        }
    }

    generated_code.push_str("}\n");
    fs::write(&dest_path, generated_code).expect("Falha ao escrever taxonomia estática");

    // Re-run if configs change
    println!("cargo:rerun-if-changed=ant_delirium_config");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ANT_DELIRIUM_BIN");
    println!("cargo:rerun-if-env-changed=ANT_DELIRIUM_CONFIG");
    println!("cargo:rerun-if-env-changed=ANT_DELIRIUM_PROJECT_ROOT");
    println!("cargo:rerun-if-env-changed=ANT_DELIRIUM_ENFORCE_BUILD_HOOK");
    println!("cargo:rerun-if-env-changed=ANT_DELIRIUM_SKIP_BUILD_HOOK");
}

fn emit_build_metadata(manifest_dir: &str) {
    println!(
        "cargo:rustc-env=ANT_DELIRIUM_BUILD_GIT_HASH={}",
        git_hash(manifest_dir)
    );
    println!(
        "cargo:rustc-env=ANT_DELIRIUM_BUILD_UNIX={}",
        build_unix_timestamp()
    );
}

fn run_governance_build_hook(manifest_dir: &str) {
    if env_flag("ANT_DELIRIUM_SKIP_BUILD_HOOK") {
        println!("cargo:warning=[ac_ant_delirium] governance build hook skipped by ANT_DELIRIUM_SKIP_BUILD_HOOK");
        return;
    }

    let manifest_path = Path::new(manifest_dir);
    let project_root = env::var("ANT_DELIRIUM_PROJECT_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| manifest_path.to_path_buf());
    let plugin_path = env::var("ANT_DELIRIUM_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| manifest_path.join("../plugins/ac_ant_delirium"));
    let enforce_hook = env_flag("ANT_DELIRIUM_ENFORCE_BUILD_HOOK");

    if !plugin_path.is_file() {
        let msg = format!(
            "[ac_ant_delirium] governance binary not found at {}. Build hook stays advisory until an external ant_delirium binary is installed. See docs/PROJECT_INSTALLATION.md.",
            plugin_path.display()
        );
        if enforce_hook {
            panic!("{}", msg);
        }
        println!("cargo:warning={}", msg);
        return;
    }

    let mut command = Command::new(&plugin_path);
    command.current_dir(&project_root);
    command.arg("--project-root").arg(&project_root);

    if let Ok(config_path) = env::var("ANT_DELIRIUM_CONFIG") {
        if !config_path.trim().is_empty() {
            command.arg("--config").arg(config_path);
        }
    }

    let status = command.status().unwrap_or_else(|err| {
        panic!(
            "[ac_ant_delirium] failed to execute governance build hook: {}",
            err
        )
    });

    if !status.success() {
        let message = format!(
            "[ac_ant_delirium] governance violations detected during build hook. Inspect the terminal output or run the API guide with: {} --api hierarchy.installation",
            plugin_path.display()
        );
        if enforce_hook {
            panic!("{}", message);
        }
        println!("cargo:warning={}", message);
    }
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

fn git_hash(manifest_dir: &str) -> String {
    Command::new("git")
        .arg("-C")
        .arg(manifest_dir)
        .arg("rev-parse")
        .arg("--short")
        .arg("HEAD")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn build_unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn collect_toml_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "toml") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn select_profile_folder(config_root: &Path) -> std::path::PathBuf {
    let type_1 = config_root.join("type_1");
    if type_1.is_dir() {
        return type_1;
    }
    config_root.to_path_buf()
}

fn compile_toml_to_rust(content: &str, output: &mut String) {
    let mut current_node = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // [IDENTIDADE] Seção de Mapeamento (Ignorada no Bake-in, pois a descoberta é dinâmica)
        if trimmed == "[mappings]" {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_node = trimmed[1..trimmed.len() - 1].to_string();
            output.push_str(&format!("    hierarchy.nodes.entry(\"{}\".to_string()).or_insert_with(crate::structs::DynamicNode::default);\n", current_node));
        } else if !current_node.is_empty() {
            if trimmed.contains('=') {
                let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
                let key = parts[0].trim();
                let val = parts[1]
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .replace("\"", "\\\"");

                if let Some(dot_idx) = key.rfind('.') {
                    let sub_node = &key[..dot_idx];
                    let real_key = &key[dot_idx + 1..];
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
