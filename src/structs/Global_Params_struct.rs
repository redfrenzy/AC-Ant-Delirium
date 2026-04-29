use std::collections::HashMap;
use std::path::{Path, PathBuf};
/// [AESTHETIC CORE - GLOBAL DATA SOVEREIGNTY]
/// Todas as definições de structs industriais residem aqui.

// --- ESTRUTURAS DE AUDITORIA ---

#[derive(Default, Debug, Clone)]
pub struct AuditViolation {
    pub file_path: String,
    pub line: usize,
    pub rule_id: String,
    pub message: String,
    pub snippet: Option<String>,
    pub quick_fix: Option<String>,
    pub detected_dna: String,
    pub taxonomy_path: String,
}

#[derive(Default, Debug, Clone)]
pub struct AuditResult {
    pub violations: Vec<AuditViolation>,
    pub logs: Vec<String>,
}

// --- ESTRUTURAS DE CONFIGURAÇÃO ---

#[derive(Default, Debug)]
pub struct AuditConfig {
    pub hierarchy: HierarchyConfig,
    pub architecture: ArchitectureConfig,
    pub jurisdiction: JurisdictionConfig,
    pub rules: RulesConfig,
    pub shadow: ShadowConfig,
    pub debug: DebugConfig,
    pub info: InfoConfig,
    pub runtime: RuntimeConfig,
}

#[derive(Default, Debug, Clone)]
pub struct ArchitectureConfig {
    pub global_excludes: Vec<String>,
}
#[derive(Default, Debug, Clone)]
pub struct JurisdictionConfig {
    pub exclusive_to: HashMap<String, Vec<String>>,
}
#[derive(Default, Debug, Clone)]
pub struct RulesConfig {
    pub structural: StructuralAxiom,
    pub pedagogical: PedagogicalAxiom,
    pub identity: IdentityAxiom,
    pub sanitization: SanitizationAxiom,
}

#[derive(Default, Debug, Clone)]
pub struct StructuralAxiom {
    pub max_nesting: usize,
    pub max_params: usize,
    pub logical_density: usize,
}

#[derive(Default, Debug, Clone)]
pub struct PedagogicalAxiom {
    pub md_pairing: bool,
    pub md_min_chars: usize,
    pub doc_fields_min: HashMap<String, usize>,
}

#[derive(Default, Debug, Clone)]
pub struct IdentityAxiom {
    pub dna_suffixes: Vec<String>,
    pub tag_separator: String,
    pub casing: String,
    pub jurisdiction_recursion: bool,
}

#[derive(Default, Debug, Clone)]
pub struct SanitizationAxiom {
    pub restricted_tokens: HashMap<String, String>,
    pub allowed_crates: Vec<String>,
    pub purify_strings: bool,
}
#[derive(Default, Debug, Clone)]
pub struct ShadowConfig {
    pub output: String,
}
#[derive(Default, Debug, Clone)]
pub struct DebugConfig {}
#[derive(Default, Debug, Clone)]
pub struct InfoConfig {
    pub tools: Vec<String>,
    pub terminal_commands: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GovernanceMode {
    #[default]
    Strict,
    Creative,
}

#[derive(Default, Debug, Clone)]
pub struct RuntimeConfig {
    pub project_root: String,
    pub mode: GovernanceMode,
}

// --- ESTRUTURAS DO MOTOR (ENGINE) ---

/// [WHAT] Nó dinâmico que armazena propriedades sem esquema fixo.
#[derive(Debug, Default, Clone)]
pub struct DynamicNode {
    pub properties: HashMap<String, String>,
}

/// [WHAT] Container agnóstico da taxonomia industrial.
#[derive(Debug, Default, Clone)]
pub struct HierarchyConfig {
    pub nodes: HashMap<String, DynamicNode>,
}

#[derive(Debug, Default, Clone)]
pub struct SearchFiles {
    pub files: Vec<PathBuf>,
}
/// [AESTHETIC CORE - GOVERNANCE CONTRACT]
/// A lei fundamental que rege todos os Axiomas do sistema.
pub trait GovernanceAxiom: Send + Sync {
    /// [ATOMIC ID] O identificador único do axioma (Ex: "complexity").
    fn id(&self) -> &str;

    /// [LITERAL NAME] Nome legível para humanos e logs industriais.
    fn name(&self) -> &str;

    /// [DISCOVERY] Utiliza o Search Engine para localizar os arquivos sob sua jurisdição.
    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String>;

    /// [PATROL] Executa a lógica de auditoria ativa no contexto de um arquivo.
    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>);
}

impl AuditConfig {
    pub fn project_root_path(&self) -> PathBuf {
        if self.runtime.project_root.trim().is_empty() {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        } else {
            PathBuf::from(&self.runtime.project_root)
        }
    }

    pub fn resolve_project_path(&self, path: &str) -> PathBuf {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.project_root_path().join(candidate)
        }
    }

    pub fn display_path<P: AsRef<Path>>(&self, path: P) -> String {
        let path = path.as_ref();
        let root = self.project_root_path();

        path.strip_prefix(&root)
            .ok()
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
    }

    pub fn is_creative_mode(&self) -> bool {
        self.runtime.mode == GovernanceMode::Creative
    }
}
