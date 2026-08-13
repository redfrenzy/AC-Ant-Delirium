use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_PLUGIN_BINARY_NAME: &str = "ac_ant_delirium";
const DEFAULT_BINARY_DISPLAY_NAME: &str = "ac_ant_delirium";
const DEFAULT_CONFIG_DIR_NAME: &str = "ant_delirium_config";

#[allow(dead_code)]
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
    let plugin_bin = plugin_dir.join("bin").join(DEFAULT_PLUGIN_BINARY_NAME);
    let config_dir = plugin_dir.join(DEFAULT_CONFIG_DIR_NAME);

    println!("cargo:rerun-if-changed={}", plugin_bin.display());
    println!("cargo:rerun-if-changed={}", config_dir.display());

    if !plugin_bin.is_file() {
        panic!(
            "[{}:bootstrap.binary_missing] governance binary not found at the resolved plugin location",
            DEFAULT_BINARY_DISPLAY_NAME
        );
    }

    let status = governance_command(&plugin_bin, &repository_root, &config_dir)
        .status()
        .unwrap_or_else(|err| {
            panic!(
                "[{}:bootstrap.execution_failed] failed to execute governance binary: {}",
                DEFAULT_BINARY_DISPLAY_NAME, err
            )
        });

    if !status.success() {
        panic!(
            "[{}:governance.failed] governance violations detected; inspect project-local diagnostics",
            DEFAULT_BINARY_DISPLAY_NAME
        );
    }
}

fn governance_command(plugin_bin: &Path, repository_root: &Path, config_dir: &Path) -> Command {
    let mut command = Command::new(plugin_bin);
    let hierarchy_path = config_dir.join("default_hierarchy.toml");
    let agent_command = read_toml_string(
        &hierarchy_path,
        "hierarchy.installation",
        "agent_command",
    )
    .unwrap_or_else(|| "./guard".to_string());
    command
        .current_dir(repository_root)
        .env("ANT_DELIRIUM_BINARY_DISPLAY", agent_command)
        .arg("--project-root")
        .arg(repository_root)
        .arg("--config")
        .arg(config_dir);
    command
}

fn find_repository_root(start: &Path) -> PathBuf {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return current;
        }
        if !current.pop() {
            panic!(
                "[{}:bootstrap.repository_missing] unable to locate repository root",
                DEFAULT_BINARY_DISPLAY_NAME
            );
        }
    }
}

fn resolve_plugin_dir(canonical_project_root: &Path) -> PathBuf {
    let expected_repository = canonicalize_or_keep(canonical_project_root);
    let mut matches = Vec::new();

    for plugins_root in candidate_plugin_roots(canonical_project_root) {
        let Ok(entries) = fs::read_dir(plugins_root) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }
            let hierarchy_path = plugin_dir
                .join(DEFAULT_CONFIG_DIR_NAME)
                .join("default_hierarchy.toml");
            if hierarchy_repository_matches(&hierarchy_path, &expected_repository) {
                let active = read_toml_string(
                    &hierarchy_path,
                    "hierarchy.installation",
                    "active",
                )
                .map(|value| value.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
                matches.push((plugin_dir, active));
            }
        }
    }

    matches.sort_by(|left, right| left.0.cmp(&right.0));
    matches.dedup_by(|left, right| left.0 == right.0);
    let active_matches: Vec<PathBuf> = matches
        .iter()
        .filter(|(_, active)| *active)
        .map(|(path, _)| path.clone())
        .collect();
    if active_matches.len() == 1 {
        return active_matches[0].clone();
    }
    if active_matches.len() > 1 {
        panic!(
            "[{}:bootstrap.plugin_ambiguous] {} active governance plugins declare this repository",
            DEFAULT_BINARY_DISPLAY_NAME,
            active_matches.len()
        );
    }

    match matches.len() {
        1 => matches.remove(0).0,
        0 => panic!(
            "[{}:bootstrap.plugin_missing] no governance plugin declares this repository in hierarchy.repository",
            DEFAULT_BINARY_DISPLAY_NAME
        ),
        count => panic!(
            "[{}:bootstrap.plugin_ambiguous] {} governance plugins declare this repository",
            DEFAULT_BINARY_DISPLAY_NAME, count
        ),
    }
}

fn candidate_plugin_roots(repository_root: &Path) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    roots.insert(repository_root.join("plugins"));

    let Some(parent) = repository_root.parent() else {
        return roots.into_iter().collect();
    };
    roots.insert(parent.join("plugins"));

    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.filter_map(Result::ok) {
            let container = entry.path();
            if container.is_dir() {
                roots.insert(container.join("plugins"));
            }
        }
    }

    roots.into_iter().collect()
}

fn hierarchy_repository_matches(hierarchy_path: &Path, expected_repository: &Path) -> bool {
    let Some(configured_repository) = read_toml_string(hierarchy_path, "hierarchy", "repository")
    else {
        return false;
    };
    canonicalize_or_keep(Path::new(&configured_repository)) == expected_repository
}

fn read_toml_string(path: &Path, section: &str, key: &str) -> Option<String> {
    let input = fs::read_to_string(path).ok()?;
    let expected_section = format!("[{}]", section);
    let mut in_section = false;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed == expected_section;
            continue;
        }
        if !in_section || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((candidate_key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        if candidate_key.trim() != key {
            continue;
        }
        let value = raw_value.trim();
        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            return Some(value[1..value.len() - 1].replace("\\\"", "\""));
        }
        let scalar = value.split('#').next().unwrap_or("").trim();
        if !scalar.is_empty() {
            return Some(scalar.to_string());
        }
    }
    None
}

fn canonicalize_or_keep(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn resolve_canonical_project_root(repository_root: &Path) -> PathBuf {
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(repository_root)
        .arg("rev-parse")
        .arg("--git-common-dir")
        .output()
    else {
        return repository_root.to_path_buf();
    };
    if !output.status.success() {
        return repository_root.to_path_buf();
    }

    let common_dir_raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if common_dir_raw.is_empty() {
        return repository_root.to_path_buf();
    }
    let common_dir = PathBuf::from(common_dir_raw);
    let common_dir = if common_dir.is_absolute() {
        common_dir
    } else {
        repository_root.join(common_dir)
    };
    common_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repository_root.to_path_buf())
}

#[allow(dead_code)]
fn main() {
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repository_root = find_repository_root(&current_dir);
    let canonical_project_root = resolve_canonical_project_root(&repository_root);
    let plugin_dir = resolve_plugin_dir(&canonical_project_root);
    let plugin_bin = plugin_dir.join("bin").join(DEFAULT_PLUGIN_BINARY_NAME);
    let config_dir = plugin_dir.join(DEFAULT_CONFIG_DIR_NAME);

    if !plugin_bin.is_file() {
        eprintln!(
            "[{}:bootstrap.binary_missing] governance binary not found at the resolved plugin location",
            DEFAULT_BINARY_DISPLAY_NAME
        );
        std::process::exit(1);
    }

    let args: Vec<String> = env::args().skip(1).collect();
    let status = governance_command(&plugin_bin, &repository_root, &config_dir)
        .args(args)
        .status()
        .unwrap_or_else(|err| {
            panic!(
                "[{}:bootstrap.execution_failed] failed to execute governance API: {}",
                DEFAULT_BINARY_DISPLAY_NAME, err
            )
        });

    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = env::temp_dir().join(format!("{}_{}", label, nanos));
        fs::create_dir_all(&path).expect("temporary directory should exist");
        path
    }

    fn write_plugin(plugin_dir: &Path, repository: &Path) {
        let config_dir = plugin_dir.join(DEFAULT_CONFIG_DIR_NAME);
        fs::create_dir_all(&config_dir).expect("config directory should exist");
        fs::write(
            config_dir.join("default_hierarchy.toml"),
            format!(
                "[hierarchy]\nrepository = \"{}\"\n",
                repository.to_string_lossy().replace('\\', "/")
            ),
        )
        .expect("hierarchy should be written");
    }

    fn mark_plugin_active(plugin_dir: &Path) {
        let hierarchy_path = plugin_dir
            .join(DEFAULT_CONFIG_DIR_NAME)
            .join("default_hierarchy.toml");
        let mut hierarchy = fs::read_to_string(&hierarchy_path)
            .expect("hierarchy should exist before activation");
        hierarchy.push_str("\n[hierarchy.installation]\nactive = true\n");
        fs::write(hierarchy_path, hierarchy).expect("active contract should be written");
    }

    #[test]
    fn resolves_plugin_from_sibling_container_by_repository_contract() {
        let root = unique_temp_dir("ant_delirium_guard_external_plugin");
        let repository = root.join("AutoBrowser");
        let plugin = root
            .join("aesthetic_core_project")
            .join("plugins")
            .join("AutoBrowser");
        fs::create_dir_all(&repository).expect("repository should exist");
        write_plugin(&plugin, &repository);

        assert_eq!(resolve_plugin_dir(&repository), plugin);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_ambiguous_repository_claims() {
        let root = unique_temp_dir("ant_delirium_guard_ambiguous_plugin");
        let repository = root.join("AutoBrowser");
        fs::create_dir_all(&repository).expect("repository should exist");
        write_plugin(&root.join("one/plugins/AutoBrowser"), &repository);
        write_plugin(&root.join("two/plugins/AutoBrowser"), &repository);

        let result = std::panic::catch_unwind(|| resolve_plugin_dir(&repository));
        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn active_installation_disambiguates_repository_claims() {
        let root = unique_temp_dir("ant_delirium_guard_active_plugin");
        let repository = root.join("AutoBrowser");
        let inactive = root.join("plugins/AutoBrowser");
        let active = root.join("container/plugins/AutoBrowser");
        fs::create_dir_all(&repository).expect("repository should exist");
        write_plugin(&inactive, &repository);
        write_plugin(&active, &repository);
        mark_plugin_active(&active);

        assert_eq!(resolve_plugin_dir(&repository), active);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignores_plugin_for_another_repository() {
        let root = unique_temp_dir("ant_delirium_guard_wrong_repository");
        let repository = root.join("AutoBrowser");
        let other_repository = root.join("OtherProject");
        fs::create_dir_all(&repository).expect("repository should exist");
        fs::create_dir_all(&other_repository).expect("other repository should exist");
        write_plugin(
            &root.join("container/plugins/AutoBrowser"),
            &other_repository,
        );

        let result = std::panic::catch_unwind(|| resolve_plugin_dir(&repository));
        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }
}
