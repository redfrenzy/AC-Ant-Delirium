use crate::structs::HierarchyConfig;
pub use crate::structs::TaxonomyVisualizer;

impl TaxonomyVisualizer {
    /// [WHAT] Gera uma árvore visual completa (Brute Tree).
    pub fn generate_tree(config: &HierarchyConfig) -> String {
        let mut out = String::new();
        out.push_str("Ant Delirium Agnostic Discovery Tree (The Brute Truth):\n\n");

        let mut keys: Vec<&String> = config.nodes.keys().collect();
        keys.sort();

        for path in keys {
            let depth = path.split('.').count();
            let indent = "  ".repeat(depth.saturating_sub(1));
            let prefix = if path == "root" { "▸" } else { "└─" };

            out.push_str(&format!("{} {} {}\n", indent, prefix, path));

            // Atributos locais do nó
            let node = &config.nodes[path];
            let mut prop_keys: Vec<&String> = node.properties.keys().collect();
            prop_keys.sort();

            let attr_indent = "  ".repeat(depth);
            for pk in prop_keys {
                out.push_str(&format!(
                    "{} [P] {} = {}\n",
                    attr_indent, pk, node.properties[pk]
                ));
            }
        }
        out
    }

    /// [WHAT] Gera a visualização detalhada de um único nó utilizando o Query Engine.
    pub fn generate_node_view(config: &HierarchyConfig, path: &str) -> Option<String> {
        if path == "hierarchy.map" {
            return Some(Self::generate_tree(config));
        }

        // [SOVEREIGNTY] Consulta direta ao motor agnóstico
        match config.search_params(path) {
            Some(result) => {
                let mut out = format!("Agnostic Node View: {}\n", path);
                out.push_str("============================\n\n");
                out.push_str(&result);
                Some(out)
            }
            None => None,
        }
    }
}
