use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn run(watch_paths: &[&str]) {
    println!("cargo:rerun-if-changed=build.rs");
    for watch_path in watch_paths {
        println!("cargo:rerun-if-changed={}", watch_path);
    }

    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR unavailable during build.rs"),
    );
    let repository_root = find_repository_root(&manifest_dir);
    let canonical_project_root = resolve_canonical_project_root(&repository_root);
    let plugin_dir = resolve_plugin_dir(&canonical_project_root);
    let plugin_bin = plugin_dir.join("ac_ant_delirium");
    let config_dir = plugin_dir.join("ant_delirium_config");

    println!("cargo:rerun-if-changed={}", plugin_bin.display());
    println!("cargo:rerun-if-changed={}", config_dir.display());

    if !plugin_bin.is_file() {
        panic!(
            "[ac_ant_delirium] plugin binary not found at {}. Run ./ac_ant_delirium config init from plugins/<project> before cargo check.",
            plugin_bin.display()
        );
    }

    let status = Command::new(&plugin_bin)
        .current_dir(&repository_root)
        .arg("--project-root")
        .arg(&repository_root)
        .arg("--config")
        .arg(&config_dir)
        .status()
        .unwrap_or_else(|err| {
            panic!(
                "[ac_ant_delirium] failed to execute project-local governance binary {}: {}",
                plugin_bin.display(),
                err
            )
        });

    if !status.success() {
        panic!(
            "[ac_ant_delirium] governance violations detected. Inspect the terminal diagnostics or run {} --api hierarchy.installation",
            plugin_bin.display()
        );
    }
}

fn find_repository_root(start: &Path) -> PathBuf {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return current;
        }
        if !current.pop() {
            panic!(
                "[ac_ant_delirium] unable to locate repository root from {}",
                start.display()
            );
        }
    }
}

fn resolve_plugin_dir(repository_root: &Path) -> PathBuf {
    let project_name = repository_root
        .file_name()
        .unwrap_or_else(|| panic!("[ac_ant_delirium] repository root has no terminal folder name"));
    repository_root
        .parent()
        .unwrap_or_else(|| panic!("[ac_ant_delirium] repository root has no parent folder"))
        .join("plugins")
        .join(project_name)
}

fn resolve_canonical_project_root(repository_root: &Path) -> PathBuf {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository_root)
        .arg("rev-parse")
        .arg("--git-common-dir")
        .output()
        .unwrap_or_else(|err| {
            panic!(
                "[ac_ant_delirium] failed to resolve git common dir from {}: {}",
                repository_root.display(),
                err
            )
        });

    if !output.status.success() {
        panic!(
            "[ac_ant_delirium] git rev-parse --git-common-dir failed for {}",
            repository_root.display()
        );
    }

    let common_dir_raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if common_dir_raw.is_empty() {
        panic!(
            "[ac_ant_delirium] git common dir was empty for {}",
            repository_root.display()
        );
    }

    let common_dir = PathBuf::from(&common_dir_raw);
    let common_dir = if common_dir.is_absolute() {
        common_dir
    } else {
        repository_root.join(common_dir)
    };

    common_dir
        .parent()
        .unwrap_or_else(|| {
            panic!(
                "[ac_ant_delirium] git common dir {} has no parent project root",
                common_dir.display()
            )
        })
        .to_path_buf()
}
