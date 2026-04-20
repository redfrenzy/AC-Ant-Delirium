use std::env;
use std::fs;
use ac_ant_delirium::Logs::GovernanceLogs;
use ac_ant_delirium::restricted_tokens::out::elite_out;
use ac_ant_delirium::structs::{HierarchyConfig, AuditConfig};
use ac_ant_delirium::visualizer::TaxonomyVisualizer;
use ac_ant_delirium::engine::Manifest_Engine_unit::ManifestEngine;
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
        elite_out("[AESTHETIC CORE] Utilizando taxonomia estática embutida (Bake-in).");
        HierarchyConfig::load_static()
    };
    
    // [COMMAND: --search]
    if args.len() >= 5 && args[3] == "--search" {
        let path = &args[4];
        if let Some(result) = hierarchy.search_params(path) {
            elite_out(&format!("\n[AQ - SEARCH RESULT]\nPath: {}\n---------------------------------------------\n{}\n---------------------------------------------", path, result));
        } else {
            elite_out(&format!("\n[!] Path or Parameter not found: {}", path));
        }
        return;
    }

    let config = AuditConfig {
        hierarchy,
        ..Default::default()
    };

    // [1] Visualização Taxonômica
    elite_out("\nAnt Delirium Agnostic Discovery Tree:\n");
    elite_out(&TaxonomyVisualizer::generate_tree(&config.hierarchy));

    // [2] Governança Ativa (Swarm Engine)
    elite_out("\n[AESTHETIC CORE - GOVERNANCE REPORT]");
    let audit_result = SwarmEngine::audit(&config);
    match ManifestEngine::generate(&config) {
        Ok(paths) => {
            elite_out(&format!("[MANIFEST] {} arquivo(s) de manifesto tecnico gerados.", paths.len()));
        }
        Err(error) => {
            elite_out(&format!("[MANIFEST][WARN] Falha ao gerar manifestos tecnicos: {}", error));
        }
    }
    let artifact_paths = GovernanceLogs::write_artifacts(
        &audit_result,
        &config,
        GovernanceLogs::DEFAULT_OUTPUT_ROOT,
    ).ok();

    GovernanceLogs::emit_terminal_report(&audit_result, &config, artifact_paths.as_ref());
    elite_out("\n[FINISH] Audit complete.");
}
