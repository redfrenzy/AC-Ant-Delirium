use crate::structs::{GovernanceAxiom, HierarchyConfig, SearchFiles};
use std::fs;
use std::path::{Path, PathBuf};

#[path = "Allowed_Crates_Axiom_unitcore.rs"]
mod Allowed_Crates_Axiom_unitcore;

pub struct AllowedCratesAxiom;

impl Default for AllowedCratesAxiom {
    fn default() -> Self {
        Self {}
    }
}

impl GovernanceAxiom for AllowedCratesAxiom {
    fn id(&self) -> &str {
        "allowed_crates"
    }
    fn name(&self) -> &str {
        "AllowedCratesAxiom Sanitization Auditor"
    }

    fn resolve_jurisdiction(&self, hierarchy: &HierarchyConfig) -> Vec<String> {
        let mut files = SearchFiles::resolve_jurisdiction(hierarchy, SearchFiles::ID);
        files.extend(resolve_project_manifests(hierarchy));
        files.sort();
        files.dedup();
        files
    }

    fn execute(&self, ctx: &mut crate::structs::AuditContext<'_>) {
        Allowed_Crates_Axiom_unitcore::patrol(ctx);
    }
}

fn resolve_project_manifests(hierarchy: &HierarchyConfig) -> Vec<String> {
    let root = runtime_project_root(hierarchy);
    let mut out = Vec::new();
    collect_cargo_toml(&root, &root, &mut out);
    out.sort();
    out.dedup();
    out
}

fn runtime_project_root(hierarchy: &HierarchyConfig) -> PathBuf {
    if let Some(path) = hierarchy.resolve_parameter("runtime.project_root") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn collect_cargo_toml(current: &Path, root: &Path, out: &mut Vec<String>) {
    if !current.starts_with(root) || !current.exists() {
        return;
    }
    if should_skip_dir(current, root) {
        return;
    }
    if current.is_file() {
        if current.file_name().and_then(|v| v.to_str()) == Some("Cargo.toml") {
            out.push(relativize_for_display(current, root));
        }
        return;
    }
    if !current.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.filter_map(|value| value.ok()) {
        let path = entry.path();
        if !path.starts_with(root) {
            continue;
        }
        collect_cargo_toml(&path, root, out);
    }
}

fn should_skip_dir(path: &Path, root: &Path) -> bool {
    let rel = path
        .strip_prefix(root)
        .ok()
        .map(|v| v.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    if rel.is_empty() {
        return false;
    }
    rel == "target"
        || rel.starts_with("target/")
        || rel == ".git"
        || rel.starts_with(".git/")
        || rel == "AC_Ant-Delirium_props"
        || rel.starts_with("AC_Ant-Delirium_props/")
        || rel == "src/Logs"
        || rel.starts_with("src/Logs/")
}

fn relativize_for_display(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
}
