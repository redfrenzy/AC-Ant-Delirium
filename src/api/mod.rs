use std::collections::BTreeSet;
use std::env;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use crate::rules;
use crate::rules::Rule_Docs_unit;
use crate::structs::HierarchyConfig;

const DEFAULT_RULES_RELATIVE_PATH: &str = "ant_delirium_config/type_1/default_rules.toml";

#[derive(Debug, Clone)]
pub struct InteractiveApi {
    hierarchy: HierarchyConfig,
    project_root: PathBuf,
    hierarchy_hint_path: String,
    binary_name: String,
    info_overrides: Vec<(String, String)>,
}

impl InteractiveApi {
    pub fn new(
        hierarchy: HierarchyConfig,
        project_root: PathBuf,
        hierarchy_hint_path: String,
        binary_name: String,
    ) -> Self {
        Self {
            hierarchy,
            project_root,
            hierarchy_hint_path,
            binary_name,
            info_overrides: Vec::new(),
        }
    }

    pub fn with_info_overrides(mut self, overrides: Vec<(String, String)>) -> Self {
        self.info_overrides = overrides;
        self
    }

    pub fn render(&self, query: Option<&str>) -> String {
        let root = self.primary_root();
        match query.map(str::trim).filter(|value| !value.is_empty()) {
            None => self.render_welcome(&root),
            Some(path) if path == root => self.render_root_overview(&root),
            Some(path) if path.ends_with(".help") => {
                let base = path.trim_end_matches(".help");
                self.render_help(base)
            }
            Some(path) if path.starts_with(&format!("{}.rule.", root)) => {
                let rule_id = &path[format!("{}.rule.", root).len()..];
                self.render_rule_detail(&root, rule_id)
            }
            Some(path) => self.render_info_path(path),
        }
    }

    fn render_welcome(&self, root: &str) -> String {
        format!(
            "Welcome to Aesthetic Core Ant-Delirium.\n\nThe project was created based on the idea that AIs tend to lose context and often generate code with excessive indentation or practices that make human review difficult.\n\nWe believe that well-structured code improves human understanding and, with standardized contextual information, reduces bug detection time and enhances collaboration between humans and AIs.\n\nFor more information, use:\n{} --api {}\n{} --installation\n",
            self.binary_name, root, self.binary_name
        )
    }

    fn render_root_overview(&self, root: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Aesthetic Core Ant-Delirium");
        out.push('\n');
        let _ = writeln!(
            out,
            "Project root: {}",
            self.display_path(&self.project_root)
        );
        let _ = writeln!(out, "Active hierarchy root: {}", root);
        let _ = writeln!(
            out,
            "Interactive help: {} --api {}.help",
            self.binary_name, root
        );
        let _ = writeln!(
            out,
            "Installation guide: {} --installation",
            self.binary_name
        );
        let _ = writeln!(
            out,
            "Rule reference: {} --api {}.rule.<rule_name>",
            self.binary_name, root
        );
        out.push('\n');
        out.push_str("Global technical notes:\n");
        out.push_str("- The active taxonomy is loaded from the current project configuration.\n");
        out.push_str("- Shadow-context artifacts are the primary machine-readable source for deep diagnostics.\n");
        out.push_str(
            "- Interactive API responses are generated in English for human and AI consumption.\n",
        );
        out.push('\n');
        out.push_str("Info block:\n");
        out.push_str(&self.render_info_value(root));
        out
    }

    fn render_help(&self, path: &str) -> String {
        let root = self.primary_root();
        let mut out = String::new();
        let _ = writeln!(out, "Available commands under {}", path);
        out.push('\n');

        let descendants = self.descendant_paths(path);
        if descendants.is_empty() {
            let _ = writeln!(out, "No child paths found under {}.", path);
        } else {
            for descendant in descendants {
                let info_state = if self.path_has_info(&descendant) {
                    "info: configured"
                } else {
                    "info: missing"
                };
                let _ = writeln!(
                    out,
                    "- {} --api {} [{}]",
                    self.binary_name, descendant, info_state
                );
            }
        }

        out.push('\n');
        out.push_str("Rule commands:\n");
        for rule_id in self.rule_ids() {
            let _ = writeln!(
                out,
                "- {} --api {}.rule.{}",
                self.binary_name, root, rule_id
            );
        }
        out
    }

    fn render_rule_detail(&self, root: &str, rule_id: &str) -> String {
        match Rule_Docs_unit::find_rule_doc(rule_id) {
            Some(doc) => {
                let mut out = String::new();
                let _ = writeln!(out, "Rule: {}.rule.{}", root, doc.id);
                let _ = writeln!(out, "Name: {}", doc.name);
                let _ = writeln!(out, "Category: {}", doc.category);
                out.push('\n');
                let _ = writeln!(out, "Summary:\n{}", doc.summary);
                out.push('\n');
                let _ = writeln!(out, "Detection Model:\n{}", doc.detection_model);
                out.push('\n');
                let _ = writeln!(out, "Failure Semantics:\n{}", doc.failure_semantics);
                out.push('\n');
                let _ = writeln!(out, "Remediation:\n{}", doc.remediation);
                if let Some(example) = doc.example {
                    out.push('\n');
                    let _ = writeln!(out, "Example:\n{}", example);
                }
                out
            }
            None => {
                let mut out = String::new();
                let _ = writeln!(out, "Rule not found: {}", rule_id);
                out.push('\n');
                out.push_str("Available rules:\n");
                for available in self.rule_ids() {
                    let _ = writeln!(out, "- {}.rule.{}", root, available);
                }
                out
            }
        }
    }

    fn render_info_path(&self, path: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Path: {}", path);
        out.push('\n');
        out.push_str(&self.render_info_value(path));
        out
    }

    fn render_info_value(&self, path: &str) -> String {
        let direct_value = self
            .hierarchy
            .search_params(path)
            .map(|value| value.trim().to_string());
        let override_body = self
            .info_overrides
            .iter()
            .find(|(override_path, _)| override_path == path)
            .map(|(_, body)| body.trim().to_string());
        let api_doc_body =
            Rule_Docs_unit::find_api_doc(path).map(|doc| doc.body.trim().to_string());
        let user_info_body = self
            .hierarchy
            .search_params(&format!("{}.info", path))
            .map(|info| info.trim().to_string());

        if let Some(body) = override_body.or(api_doc_body) {
            let mut out = String::new();
            out.push_str(&body);
            out.push('\n');
            if let Some(user_info) = user_info_body {
                out.push('\n');
                out.push_str("User notes:\n");
                out.push_str(&user_info);
                out.push('\n');
            }
            return out;
        }

        if let Some(value) = direct_value {
            return format!("{}\n", value);
        }

        match user_info_body {
            Some(info) => format!("{}\n", info.trim()),
            None => format!(
                "info not found, you can define it in {}\n\nExample:\n[{}]\ninfo = \"Define custom information here (string in English)\"\n",
                self.hierarchy_hint_path, path
            ),
        }
    }

    fn primary_root(&self) -> String {
        for preferred_root in ["hierarchy", "rules", "regras"] {
            if self.has_semantic_root(preferred_root) {
                return preferred_root.to_string();
            }
        }

        let mut roots: Vec<String> = self
            .hierarchy
            .nodes
            .keys()
            .filter(|key| !key.is_empty() && !key.starts_with("runtime."))
            .filter_map(|key| key.split('.').next().map(str::to_string))
            .collect();
        roots.sort();
        roots.dedup();
        roots
            .into_iter()
            .next()
            .unwrap_or_else(|| "hierarchy".to_string())
    }

    fn has_semantic_root(&self, root: &str) -> bool {
        [
            "structural",
            "identity",
            "pedagogical",
            "sanitization",
            "dependency_governance",
            "files",
        ]
        .iter()
        .any(|branch| {
            self.hierarchy
                .nodes
                .contains_key(&format!("{}.{}", root, branch))
        })
    }

    fn descendant_paths(&self, path: &str) -> Vec<String> {
        let mut descendants = BTreeSet::new();
        let prefix = format!("{}.", path);

        for key in self.hierarchy.nodes.keys() {
            if key == path || key.starts_with("runtime.") {
                continue;
            }
            if key.starts_with(&prefix) {
                descendants.insert(key.clone());
            }
        }

        descendants.into_iter().collect()
    }

    fn path_has_info(&self, path: &str) -> bool {
        self.info_overrides
            .iter()
            .any(|(override_path, _)| override_path == path)
            || Rule_Docs_unit::find_api_doc(path).is_some()
            || self
                .hierarchy
                .search_params(&format!("{}.info", path))
                .is_some()
    }

    fn rule_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = rules::get_full_axioms()
            .into_iter()
            .map(|axiom| axiom.id().to_string())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    fn display_path<P: AsRef<Path>>(&self, path: P) -> String {
        let path = path.as_ref();
        path.strip_prefix(&self.project_root)
            .ok()
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
    }
}

pub fn default_rules_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_RULES_RELATIVE_PATH)
}

#[cfg(test)]
mod tests {
    use super::InteractiveApi;
    use crate::structs::HierarchyConfig;
    use std::path::PathBuf;

    fn sample_api() -> InteractiveApi {
        let hierarchy = HierarchyConfig::parse_brute(
            r#"
[hierarchy]
info = "Root project info."

[hierarchy.files]
info = "Files branch info."

[hierarchy.files.unit]
info = "Unit branch info."

[hierarchy.files.struct.documentation.what]
info = "Use this parameter for description what this function make."
min = 64

[hierarchy.files.unit.unitcore]
kind = "internal"

[hierarchy.structural]
enabled = true
"#,
        );

        InteractiveApi::new(
            hierarchy,
            PathBuf::from("/workspace/project"),
            "ant_delirium_config/type_1/default_hierarchy.toml".to_string(),
            "delirium_cli".to_string(),
        )
    }

    #[test]
    fn welcome_uses_dynamic_root_hint() {
        let output = sample_api().render(None);
        assert!(output.contains("Welcome to Aesthetic Core Ant-Delirium."));
        assert!(output.contains("delirium_cli --api hierarchy"));
    }

    #[test]
    fn root_overview_appends_info_block() {
        let output = sample_api().render(Some("hierarchy"));
        assert!(output.contains("Active hierarchy root: hierarchy"));
        assert!(output.contains("Root project info."));
        assert!(output.contains("hierarchy.help"));
    }

    #[test]
    fn info_fallback_points_to_hierarchy_file() {
        let output = sample_api().render(Some("hierarchy.structural"));
        assert!(output.contains("info not found"));
        assert!(output.contains("ant_delirium_config/type_1/default_hierarchy.toml"));
    }

    #[test]
    fn taxonomy_tag_info_path_returns_user_defined_text() {
        let output = sample_api().render(Some("hierarchy.files.struct.documentation.what.info"));
        assert!(output.contains("Use this parameter for description what this function make."));
    }

    #[test]
    fn installation_prefers_internal_api_doc() {
        let output = sample_api().render(Some("hierarchy.installation"));
        assert!(output.contains("project-scoped installation node"));
        assert!(!output.contains("info not found"));
    }

    #[test]
    fn installation_appends_user_notes_after_internal_doc() {
        let hierarchy = HierarchyConfig::parse_brute(
            r#"
[hierarchy.installation]
info = "User defined installation note."
"#,
        );
        let api = InteractiveApi::new(
            hierarchy,
            PathBuf::from("/workspace/project"),
            "ant_delirium_config/type_1/default_hierarchy.toml".to_string(),
            "delirium_cli".to_string(),
        );

        let output = api.render(Some("hierarchy.installation"));
        assert!(output.contains("project-scoped installation node"));
        assert!(output.contains("User notes:"));
        assert!(output.contains("User defined installation note."));
    }

    #[test]
    fn help_lists_child_paths_and_rule_commands() {
        let output = sample_api().render(Some("hierarchy.help"));
        assert!(output.contains("delirium_cli --api hierarchy.files [info: configured]"));
        assert!(output.contains("delirium_cli --api hierarchy.files.unit [info: configured]"));
        assert!(output.contains("delirium_cli --api hierarchy.files.unit.unitcore [info: missing]"));
        assert!(output.contains("delirium_cli --api hierarchy.rule.max_nesting"));
    }

    #[test]
    fn rule_detail_returns_rule_documentation() {
        let output = sample_api().render(Some("hierarchy.rule.md_pairing"));
        assert!(output.contains("Rule: hierarchy.rule.md_pairing"));
        assert!(output.contains("Markdown Pairing"));
        assert!(output.contains("Detection Model:"));
        assert!(output.contains("[WHERE]"));
    }

    #[test]
    fn rule_detail_exposes_technical_sections() {
        let output = sample_api().render(Some("hierarchy.rule.max_nesting"));
        assert!(output.contains("Summary:"));
        assert!(output.contains("Detection Model:"));
        assert!(output.contains("Failure Semantics:"));
        assert!(output.contains("Remediation:"));
        assert!(output.contains("d_max > L"));
    }
}
