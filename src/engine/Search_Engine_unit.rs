// [AESTHETIC CORE - HIERARCHY ENGINE UNIT]
// Motor Abstrato para leitura e resolução de hierarquias taxonômicas.
// [SOVEREIGNTY] Interface híbrida: Suporta Bake-in estático e Override dinâmico.

pub(crate) use crate::structs::{HierarchyConfig, SearchFiles};

#[path = "Search_Engine_unitcore.rs"]
pub mod internal;

// [BAKE-IN] Inclui a taxonomia gerada em tempo de compilação pelo build.rs
include!(concat!(env!("OUT_DIR"), "/generated_static_hierarchy.rs"));

/// [AESTHETIC CORE - HIERARCHY MOTOR PROTOCOL]
impl HierarchyConfig {
    /// [WHAT] Carrega a hierarquia estática embutida no binário (Zero I/O).
    pub fn load_static() -> Self {
        let mut config = Self::default();
        load_embedded_hierarchy(&mut config);
        config
    }

    /// [WHAT] Lê todos os TOMLs de uma pasta e os funde em uma única configuração (Runtime I/O).
    pub fn load_from_folder(dir_path: &str) -> Self {
        internal::AgnosticScanner::load_from_folder(dir_path)
    }

    /// [WHAT] Lê e analisa um único TOML bruto.
    pub fn parse_brute(input: &str) -> Self {
        internal::AgnosticScanner::parse_brute(input)
    }
    /// [WHAT] Query Engine para busca de parâmetros ou nós.
    pub fn search_params(&self, path: &str) -> Option<String> {
        internal::HierarchyResolver::search_params(self, path)
    }

    /// [WHAT] Resolve um requisito atômico da rule para seu valor final.
    pub fn resolve_parameter(&self, id: &str) -> Option<String> {
        internal::HierarchyResolver::resolve_parameter(self, id)
    }

    /// [WHAT] Resolve um requisito atômico e devolve também o caminho concreto de onde o valor foi materializado.
    pub fn resolve_parameter_binding(&self, id: &str) -> Option<(String, String)> {
        internal::HierarchyResolver::resolve_parameter_binding(self, id)
    }

    /// [WHAT] Resolve um parâmetro a partir de um caminho taxonômico, subindo pelos ancestrais até o root.
    pub fn resolve_taxonomy_parameter(
        &self,
        taxonomy_path: &str,
        parameter: &str,
    ) -> Result<(String, String), String> {
        internal::HierarchyResolver::resolve_taxonomy_parameter(self, taxonomy_path, parameter)
    }

    /// [WHAT] Traduz um ID atômico (ex: search_file) para seu path taxonômico.
    pub fn resolve_id(&self, id: &str) -> String {
        internal::HierarchyResolver::resolve_id(self, id)
    }

    /// [WHAT] Retorna todos os caminhos da hierarquia que terminam com o identificador fornecido.
    pub fn find_paths_by_id(&self, id: &str) -> Vec<String> {
        internal::HierarchyResolver::find_paths_by_id(self, id)
    }

    /// [WHAT] Retorna os requirements anexados ao nó-identidade informado.
    pub fn find_requirements(&self, identity_path: &str) -> Vec<String> {
        internal::HierarchyResolver::find_requirements(self, identity_path)
    }

    /// [AESTHETIC CORE - ABYSSAL INTERROGATION]
    /// Interroga a hierarquia para descobrir funcionalidades e jurisdições físicas.
    pub fn search_functionalities(&self, id: &str) -> Vec<(String, Vec<String>)> {
        internal::HierarchyResolver::search_functionalities(self, id)
    }

    /// [WHAT] Anexa contexto efêmero de execução sem hardcode no manifesto persistido.
    pub fn attach_runtime_project_root(&mut self, project_root: &str) {
        let node = self
            .nodes
            .entry("runtime".to_string())
            .or_insert_with(crate::structs::DynamicNode::default);
        node.properties
            .insert("project_root".to_string(), project_root.to_string());
    }
}

impl SearchFiles {
    pub const ID: &str = "search_file";
    /// [AESTHETIC CORE - DISCOVERY SERVICE]
    /// Resolve a jurisdição física (lista de arquivos) para um ID atômico.
    pub fn resolve_jurisdiction(hierarchy: &HierarchyConfig, id: &str) -> Vec<String> {
        internal::resolve_jurisdiction(hierarchy, id)
    }
}
