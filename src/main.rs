use std::env;
use std::fs;
use std::path::Path;

/// [WHAT] Orchestrates the initialization and sequential execution of the Ant Delirium audit pipeline for the Aesthetic Core CLI application.
/// [WHY] To provide a robust terminal interface that enables structural and semantic verification of Rust geodes, serving as a gateway for architectural compliance.
/// [WHERE] Primary entry point for the CLI tool located in main.rs, acting as the orchestrator for both file and directory-level audits.
/// [USAGE] Invocado pelo sistema operacional para disparar a auditoria técnica de um geode, processando os argumentos de entrada e interpretando as configurações do manifesto. Esta fase de inicialização garante que o ambiente de execução e as permissões de arquivo estejam em conformidade antes do início do escaneamento recursivo.
fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        ac_ant_delirium::restricted_tokens::out::elite_out("Aesthetic Core - Ant Delirium Governance Engine");
        ac_ant_delirium::restricted_tokens::out::elite_out("Usage: ac_ant_delirium <file_or_directory>");
        return;
    }

    let target = &args[1];
    let config_path = "delirium.toml";
    
    let config_toml = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(_) => {
            ac_ant_delirium::restricted_tokens::out::elite_out("[WARN] delirium.toml not found. Using defaults.");
            "".to_string()
        }
    };

    let target_path = Path::new(target);
    let abs_target = ac_ant_delirium::restricted_tokens::resolve::elite_resolve_or(fs::canonicalize(target_path).ok(), target_path.to_path_buf());
    let target_str = ac_ant_delirium::restricted_tokens::resolve::elite_resolve_or(abs_target.to_str(), target);

    if abs_target.is_dir() {
        audit_directory(target_str, &config_toml);
        return;
    }
    
    audit_file(target_str, &config_toml);
}

/// [WHAT] Orchestrates the governance audit of an individual Rust source file provided via the CLI to check for P1 and P2 violations in a focused context.
/// [WHY] To provide a localized entry point for verifying single components without full workspace scans, enabling rapid iteration and feedback for developers.
/// [WHERE] Primary execution entry point for non-recursive auditing within the ac_ant_delirium CLI toolchain, providing file-level diagnostics to the terminal.
/// [USAGE] Invoked when the provided path argument identifies a valid regular file on disk, triggering a surgical audit and printing the associated compliance report. Esta função é otimizada para verificações rápidas durante o desenvolvimento ativo de arquivos individuais ou módulos de lógica isolada.
fn audit_file(path: &str, config_toml: &str) {
    let config = ac_ant_delirium::config::AuditConfig::parse_brute(config_toml);
    let source = ac_ant_delirium::restricted_tokens::resolve::elite_resolve_or(fs::read_to_string(path).ok(), String::new());
    let result = ac_ant_delirium::audit_code_to_result(&source, path, config_toml);
    
    ac_ant_delirium::restricted_tokens::out::elite_out(&ac_ant_delirium::reporter::format_visual_report(&result, &config));
}

/// [WHAT] Performs a bulk governance scan across an entire directory hierarchy to validate compliance for all functional units within an Aesthetic Core geode.
/// [WHY] To support the auditing of multi-module architectural trees, ensuring that the entire project state is synchronized with the established governance manifest.
/// [WHERE] Directory-level dispatcher within the CLI toolchain in main.rs, serving as the high-level orchestrator for full-workspace governance checks.
/// [USAGE] Invoked when the target path is identified as a directory during argument resolution, iniciando uma varredura recursiva e forçando um código de saída diferente de zero em caso de falha de conformidade ou erros estruturais graves detectados pelo motor.
fn audit_directory(dir: &str, config_toml: &str) {
    let config = ac_ant_delirium::config::AuditConfig::parse_brute(config_toml);
    let result = ac_ant_delirium::audit_geode_recursive(dir, config_toml);

    if !result.violations.is_empty() {
        // [ELITE EXPORT] Salva o Shadow Context (SARIF) global com propriedades de governança para consumo AI
        let shadow = result.to_shadow_context(&config.shadow.output, &config);
        let _ = std::fs::write(&config.shadow.output, shadow);
        
        ac_ant_delirium::restricted_tokens::out::elite_out(&ac_ant_delirium::reporter::format_visual_report(&result, &config));
        std::process::exit(1);
    }
}
