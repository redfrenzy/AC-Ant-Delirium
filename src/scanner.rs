use std::path::Path;
use std::fs;
use super::config::AuditConfig;
use crate::restricted_tokens::resolve::elite_resolve_or;

/// [WHAT] High-fidelity recursive scanning system designed for the Aesthetic Core ecosystem to identify and process all source files within a geode.
/// [WHY] To ensure that governance audits, architectural analysis, and metadata generation reach every level of the source tree, maintaining total visibility.
/// [WHERE] Infrastructure layer for file scanning and tree traversal located in scanner.rs, serving as the entry point for all industrial code audits.
/// [USAGE] Invocado pelo motor de auditoria para realizar uma varredura profunda e sistemática da árvore de arquivos do projeto, filtrando componentes conforme as diretrizes de governança.
pub struct RecursiveScanner;

impl RecursiveScanner {
    /// [WHAT] Initiates a deep recursive scan starting from a root directory to identify candidate source files for governance processing.
    /// [WHY] To centralize file orchestration and respect the global exclusion authority from the delirium.toml configuration, ensuring a sanitized scan.
    /// [WHERE] Public entry point for the recursive scanner system in scanner.rs, acting as the primary interface for any tree-based audit tasks.
    /// [USAGE] RecursiveScanner::scan(Path::new("src"), &config, |path, source| { /* process each discovered file with its full source content */ });
    pub fn scan<F>(root: &Path, config: &AuditConfig, mut callback: F) 
    where F: FnMut(&Path, &str) 
    {
        let _ = Self::scan_recursive(root, root, config, &mut callback);
    }

    /// [WHAT] Internal directory traversal algorithm with multi-layer governance filtering to protect against deep recursion in unauthorized zones.
    /// [WHY] To maintain cognitive nesting complexity at industrial levels while accurately processing complex and massive file trees without logic leakage.
    /// [WHERE] Private recursive implementation layer in scanner.rs, providing the low-level logic for the Aesthetic Core file discovery engine.
    fn scan_recursive<F>(root: &Path, dir: &Path, config: &AuditConfig, callback: &mut F) -> std::io::Result<()>
    where F: FnMut(&Path, &str)
    {
        // 1. Validação de Autoridade: Se o diretório atual está ignorado, interrompe a recursão.
        if is_ignored(root, dir, config) { return Ok(()); }

        let entries = fs::read_dir(dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if is_ignored(root, &path, config) { continue; }

            Self::dispatch_entry(root, &path, config, callback);
        }
        Ok(())
    }

    /// [WHAT] Dispatches a file system entry to the appropriate handler based on its type.
    /// [WHY] To maintain cognitive nesting under control and avoid the 'Cognitive Abyss' in recursive scanning.
    /// [WHERE] Tactical dispatcher in scanner.rs.
    /// [USAGE] Utilizado pelo loop de varredura para rotear caminhos para funções de recursão ou processamento de arquivos individuais de forma eficiente.
    fn dispatch_entry<F>(root: &Path, path: &Path, config: &AuditConfig, callback: &mut F)
    where F: FnMut(&Path, &str)
    {
        if path.is_dir() {
            let _ = Self::scan_recursive(root, path, config, callback);
            return;
        }
        Self::process_file(path, callback);
    }

    /// [WHAT] Processes an individual file entry discovered during the scan to verify its extension and submit its content to the auditing callback.
    /// [WHY] To isolate the file-reading logic and extension filtering from the recursive traversal orchestration, maintaining a clean architectural separation.
    /// [WHERE] File processing utility in scanner.rs, acting as the final bridge between the filesystem and the auditing engine's internal logic.
    fn process_file<F>(path: &Path, callback: &mut F) 
    where F: FnMut(&Path, &str)
    {
        let ext = path.extension().and_then(|s| s.to_str());
        let name = path.file_name().and_then(|s| s.to_str());

        if ext != Some("rs") && name != Some("Cargo.toml") { return; }
        if let Ok(source) = fs::read_to_string(path) {
            callback(path, &source);
        }
    }
}

/// [WHAT] Authority predicate for path exclusion that determines if a file or directory should be skipped based on the industrial governance configuration.
/// [WHY] To ensure that artifact directories, quarantine zones, and ignored paths are strictly respected, optimizing audit performance and accuracy in Elite geodes.
/// [WHERE] Selective filtering layer in scanner.rs, serving as the gatekeeper for the recursive scanner to prevent illegal logic infiltration from unmanaged zones.
/// [USAGE] Retorna true se o componente do arquivo ou diretório deve ser evitado pelo scanner recursivo para garantir que apenas código governado seja processado.
fn is_ignored(root: &Path, path: &Path, config: &AuditConfig) -> bool {
    let relative_path = match path.strip_prefix(root) {
        Ok(p) => p,
        Err(_) => path,
    };
    let path_str = elite_resolve_or(relative_path.to_str(), "");
    // [ELITE NORMALIZATION] Remove prefixes and trailing separators for robust matching.
    let normalized_path = path_str.trim_matches(|c| c == '.' || c == '/');
    
    for ignore_path in &config.architecture.global_excludes {
        if match_path_governance(normalized_path, ignore_path) {
            return true;
        }
    }
    false
}

/// [WHAT] Performs a governance-compliant path match between a candidate file and an excluded architectural zone.
/// [WHY] To keep nesting levels flat while ensuring exact or root-based path authority during the physical scan.
/// [WHERE] Path normalization utility in scanner.rs.
/// [USAGE] Invocada pelo gatekeeper (is_ignored) para realizar a comparação física entre o caminho de um arquivo e as diretrizes de exclusão global, garantindo que colchetes, barras e outros delimitadores de sistema de arquivos sejam tratados de forma determinística e segura.
fn match_path_governance(path: &str, ignore: &str) -> bool {
    let normalized_ignore = ignore.trim_matches(|c| c == '.' || c == '/');
    path == normalized_ignore || path.starts_with(&(normalized_ignore.to_string() + "/"))
}
