#[cfg(test)]
mod tests {
    use crate::engine::Swarm_Engine_unit::SwarmEngine;
    use crate::structs::{AuditConfig, HierarchyConfig, RuntimeConfig};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("{}_{}", label, nanos));
        fs::create_dir_all(&dir).expect("temporary directory should be created");
        dir
    }

    fn config_for(project_root: &std::path::Path, hierarchy_raw: &str) -> AuditConfig {
        AuditConfig {
            hierarchy: HierarchyConfig::parse_brute(hierarchy_raw),
            runtime: RuntimeConfig {
                project_root: project_root.to_string_lossy().to_string(),
                ..RuntimeConfig::default()
            },
            ..AuditConfig::default()
        }
    }

    #[test]
    fn super_block_aborts_when_build_guard_is_not_first_statement() {
        let workspace = unique_temp_dir("ac_ant_delirium_super_block_abort");
        fs::create_dir_all(workspace.join("src")).expect("src dir should exist");
        fs::create_dir_all(workspace.join("AC_Ant-Delirium_props"))
            .expect("guard dir should exist");
        fs::write(workspace.join("src/lib.rs"), "pub fn sample() {}\n")
            .expect("fixture source should be written");
        fs::write(
            workspace.join("build.rs"),
            "#[path = \"AC_Ant-Delirium_props/ant_delirium_build_guard.rs\"]\nmod ant_delirium_build_guard;\nfn main() {\n    println!(\"late guard\");\n    ant_delirium_build_guard::run(&[\"src\"]);\n}\n",
        )
        .expect("build.rs should be written");
        fs::write(
            workspace.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"),
            "pub fn run(_watch_paths: &[&str]) {}\n",
        )
        .expect("guard file should be written");

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(workspace.join("build.rs"))
                .expect("build.rs metadata should exist")
                .permissions();
            perms.set_mode(0o444);
            fs::set_permissions(workspace.join("build.rs"), perms)
                .expect("build.rs should become readonly");

            let mut perms =
                fs::metadata(workspace.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"))
                    .expect("guard metadata should exist")
                    .permissions();
            perms.set_mode(0o444);
            fs::set_permissions(
                workspace.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"),
                perms,
            )
            .expect("guard file should become readonly");
        }

        let hierarchy = r#"
[hierarchy.engine.search_file]
target.core

[hierarchy.target]
core = ["src"]

[hierarchy.super_block]
enabled = true
abort_on_violation = true
guard_path = "AC_Ant-Delirium_props/ant_delirium_build_guard.rs"
guard_call = "ant_delirium_build_guard::run("
guard_symbol = "ant_delirium_build_guard"
guard_owner = "any"
guard_readonly = true
enforced_files = ["build.rs", "AC_Ant-Delirium_props/ant_delirium_build_guard.rs"]
enforced_owner = "any"
enforced_readonly = true
watch_files = []
"#;

        let result = SwarmEngine::audit(&config_for(&workspace, hierarchy));
        assert!(result.violations.iter().any(|violation| violation
            .rule_id
            .contains(".super_block.enforced.guard_order.")));
        assert!(result
            .logs
            .iter()
            .any(|line| line.contains("Preflight abortado")));
    }

    #[test]
    fn super_block_keeps_scanning_when_only_watch_files_are_missing() {
        let workspace = unique_temp_dir("ac_ant_delirium_super_block_watch");
        fs::create_dir_all(workspace.join("src")).expect("src dir should exist");
        fs::create_dir_all(workspace.join("AC_Ant-Delirium_props"))
            .expect("guard dir should exist");
        fs::write(workspace.join("src/lib.rs"), "pub fn sample() {}\n")
            .expect("fixture source should be written");
        fs::write(
            workspace.join("build.rs"),
            "#[path = \"AC_Ant-Delirium_props/ant_delirium_build_guard.rs\"]\nmod ant_delirium_build_guard;\nfn main() {\n    ant_delirium_build_guard::run(&[\"src\"]);\n}\n",
        )
        .expect("build.rs should be written");
        fs::write(
            workspace.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"),
            "pub fn run(_watch_paths: &[&str]) {}\n",
        )
        .expect("guard file should be written");

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(workspace.join("build.rs"))
                .expect("build.rs metadata should exist")
                .permissions();
            perms.set_mode(0o444);
            fs::set_permissions(workspace.join("build.rs"), perms)
                .expect("build.rs should become readonly");

            let mut perms =
                fs::metadata(workspace.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"))
                    .expect("guard metadata should exist")
                    .permissions();
            perms.set_mode(0o444);
            fs::set_permissions(
                workspace.join("AC_Ant-Delirium_props/ant_delirium_build_guard.rs"),
                perms,
            )
            .expect("guard file should become readonly");
        }

        let hierarchy = r#"
[hierarchy.engine.search_file]
target.core

[hierarchy.target]
core = ["src"]

[hierarchy.super_block]
enabled = true
abort_on_violation = true
guard_path = "AC_Ant-Delirium_props/ant_delirium_build_guard.rs"
guard_call = "ant_delirium_build_guard::run("
guard_symbol = "ant_delirium_build_guard"
guard_owner = "any"
guard_readonly = true
enforced_files = ["build.rs", "AC_Ant-Delirium_props/ant_delirium_build_guard.rs"]
enforced_owner = "any"
enforced_readonly = true
watch_files = ["governance.json"]
"#;

        let result = SwarmEngine::audit(&config_for(&workspace, hierarchy));
        assert!(result
            .violations
            .iter()
            .any(|violation| violation.rule_id.contains(".super_block.watch.missing.")));
        assert!(!result
            .logs
            .iter()
            .any(|line| line.contains("Preflight abortado")));
        assert!(result
            .logs
            .iter()
            .any(|line| line.contains("Patrulha encerrada")));
    }
}
