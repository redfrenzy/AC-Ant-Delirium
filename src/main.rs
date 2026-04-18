#![allow(non_snake_case)]

use std::env;
use std::fs;
use ac_ant_delirium::structs::{HierarchyConfig, AuditConfig};
use ac_ant_delirium::visualizer::TaxonomyVisualizer;
use ac_ant_delirium::engine::Swarm_Engine_unit::SwarmEngine;

/// [WHAT] Entry point focado na revelação ativa de um manifesto e auditoria de governança.
fn main() {
    let args: Vec<String> = env::args().collect();
    
    // [HYBRID LOAD] - Tenta carregar configuração externa ou recorre à estática embutida
    let hierarchy = if args.len() >= 3 && args[1] == "--api" {
        let config_path = &args[2];
        let is_dir = fs::metadata(config_path).map(|m| m.is_dir()).unwrap_or(false);
        if is_dir {
            HierarchyConfig::load_from_folder(config_path)
        } else {
            let input = fs::read_to_string(config_path).unwrap_or_else(|_| "".to_string());
            HierarchyConfig::parse_brute(&input)
        }
    } else {
        println!("[AESTHETIC CORE] Utilizando taxonomia estática embutida (Bake-in).");
        HierarchyConfig::load_static()
    };
    
    // [COMMAND: --search]
    if args.len() >= 5 && args[3] == "--search" {
        let path = &args[4];
        if let Some(result) = hierarchy.search_params(path) {
            println!("\n[AQ - SEARCH RESULT]\nPath: {}\n---------------------------------------------\n{}\n---------------------------------------------", path, result);
        } else {
            println!("\n[!] Path or Parameter not found: {}", path);
        }
        return;
    }

    let config = AuditConfig {
        hierarchy,
        ..Default::default()
    };

    // [1] Visualização Taxonômica
    println!("\nAnt Delirium Agnostic Discovery Tree:\n");
    println!("{}", TaxonomyVisualizer::generate_tree(&config.hierarchy));

    // [2] Governança Ativa (Swarm Engine)
    println!("\n[AESTHETIC CORE - GOVERNANCE REPORT]");
    let violations = SwarmEngine::patrol(&config);
    
    if violations.is_empty() {
        println!("\n[SUCCESS] No governance violations detected. Your cluster is in the Elite state.");
    } else {
        println!("\n[!] DETECTED {} VIOLATIONS IN GOVERNANCE:", violations.len());
        for (i, v) in violations.iter().enumerate() {
            println!("{:02}. [{}] in {}\n    Message: {}", i + 1, v.rule_id, v.file_path, v.message);
        }
    }
    
    println!("\n[FINISH] Audit complete.");
}
