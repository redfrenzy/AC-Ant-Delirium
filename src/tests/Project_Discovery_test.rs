#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::structs::{CargoProjectKind, ProjectDiscovery};

    const DISCOVERY_SOURCE: &str = include_str!("../structs/Project_Discovery_structcore.rs");

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after Unix epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("{label}_{}_{}", std::process::id(), nonce));
        fs::create_dir_all(&root).expect("temporary discovery root should be created");
        root
    }

    fn write_manifest(root: &Path, source: &str) -> PathBuf {
        fs::create_dir_all(root).expect("manifest root should be created");
        let manifest = root.join("Cargo.toml");
        fs::write(&manifest, source).expect("Cargo manifest should be written");
        manifest
    }

    fn package_manifest(name: &str) -> String {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
    }

    #[test]
    fn discovers_single_package_from_a_nested_directory() {
        let root = unique_temp_dir("project_discovery_package");
        let package = root.join("package");
        let nested = package.join("src/nested");
        write_manifest(&package, &package_manifest("single-package"));
        fs::create_dir_all(&nested).expect("nested package directory should be created");

        let discovered =
            ProjectDiscovery::discover(&nested, None).expect("package should be discovered");

        assert_eq!(discovered.project_root, fs::canonicalize(&package).unwrap());
        assert_eq!(discovered.cargo_kind, Some(CargoProjectKind::Package));
        assert_eq!(discovered.cargo_workspace_root, None);
        assert!(discovered.git_checkout.is_none());

        fs::remove_dir_all(root).expect("temporary package fixture should be removed");
    }

    #[test]
    fn selects_the_containing_virtual_workspace_for_a_member() {
        let root = unique_temp_dir("project_discovery_virtual_workspace");
        let member = root.join("crates/member");
        write_manifest(
            &root,
            "[workspace]\nmembers = [\"crates/member\"]\nresolver = \"2\"\n",
        );
        let member_manifest = write_manifest(&member, &package_manifest("member"));
        fs::create_dir_all(member.join("src")).expect("member source directory should exist");

        let discovered = ProjectDiscovery::discover(&member.join("src"), None)
            .expect("workspace member should be discovered");

        let canonical_root = fs::canonicalize(&root).unwrap();
        assert_eq!(discovered.project_root, canonical_root);
        assert_eq!(
            discovered.selected_manifest_path,
            Some(fs::canonicalize(member_manifest).unwrap())
        );
        assert_eq!(
            discovered.cargo_workspace_root,
            Some(fs::canonicalize(&root).unwrap())
        );
        assert_eq!(discovered.cargo_kind, Some(CargoProjectKind::Workspace));

        fs::remove_dir_all(root).expect("temporary workspace fixture should be removed");
    }

    #[test]
    fn supports_a_workspace_with_a_root_package() {
        let root = unique_temp_dir("project_discovery_root_package");
        write_manifest(
            &root,
            "[package]\nname = \"root-package\"\nversion = \"0.1.0\"\n\n[workspace]\nmembers = []\n",
        );
        fs::create_dir_all(root.join("src")).expect("root package source should exist");

        let discovered = ProjectDiscovery::discover(&root.join("src"), None)
            .expect("root package workspace should be discovered");

        assert_eq!(discovered.project_root, fs::canonicalize(&root).unwrap());
        assert_eq!(discovered.cargo_kind, Some(CargoProjectKind::Workspace));

        fs::remove_dir_all(root).expect("temporary root package fixture should be removed");
    }

    #[test]
    fn explicit_manifest_takes_precedence_over_the_current_directory() {
        let root = unique_temp_dir("project_discovery_explicit_manifest");
        let current = root.join("current");
        let selected = root.join("selected");
        write_manifest(&current, &package_manifest("current"));
        let selected_manifest = write_manifest(&selected, &package_manifest("selected"));

        let discovered = ProjectDiscovery::discover(&current, Some(&selected_manifest))
            .expect("explicit manifest should be selected");

        assert_eq!(
            discovered.project_root,
            fs::canonicalize(&selected).unwrap()
        );
        assert_eq!(
            discovered.selected_manifest_path,
            Some(fs::canonicalize(selected_manifest).unwrap())
        );

        fs::remove_dir_all(root).expect("temporary explicit fixture should be removed");
    }

    #[test]
    fn resolves_package_workspace_without_invoking_cargo() {
        let root = unique_temp_dir("project_discovery_package_workspace");
        let workspace = root.join("workspace");
        let member = workspace.join("member");
        write_manifest(&workspace, "[workspace]\nmembers = [\"member\"]\n");
        write_manifest(
            &member,
            "[package]\nname = \"member\"\nversion = \"0.1.0\"\nworkspace = \"..\"\n",
        );

        let discovered = ProjectDiscovery::discover(&member, None)
            .expect("package.workspace should resolve its workspace");

        assert_eq!(
            discovered.cargo_workspace_root,
            Some(fs::canonicalize(&workspace).unwrap())
        );
        assert!(!DISCOVERY_SOURCE.contains("Command::new"));
        assert!(!DISCOVERY_SOURCE.contains("cargo metadata"));
        assert!(!DISCOVERY_SOURCE.contains("std::process"));

        fs::remove_dir_all(root).expect("temporary package.workspace fixture should be removed");
    }

    #[test]
    fn reads_linked_worktree_common_dir_from_git_metadata() {
        let root = unique_temp_dir("project_discovery_worktree");
        let main = root.join("main");
        let worktree = root.join("feature");
        let main_git = main.join(".git");
        let worktree_git_dir = main_git.join("worktrees/feature");
        write_manifest(&main, &package_manifest("project"));
        write_manifest(&worktree, &package_manifest("project"));
        fs::create_dir_all(&worktree_git_dir).expect("linked worktree Git dir should be created");
        fs::create_dir_all(main_git.join("objects"))
            .expect("main Git objects directory should be created");
        fs::write(main_git.join("HEAD"), "ref: refs/heads/main\n")
            .expect("main Git HEAD should be written");
        fs::write(worktree_git_dir.join("HEAD"), "ref: refs/heads/feature\n")
            .expect("worktree Git HEAD should be written");
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", worktree_git_dir.to_string_lossy()),
        )
        .expect("worktree .git marker should be written");
        fs::write(worktree_git_dir.join("commondir"), "../..\n")
            .expect("worktree commondir marker should be written");

        let main_discovery =
            ProjectDiscovery::discover(&main, None).expect("main checkout should be discovered");
        let worktree_discovery = ProjectDiscovery::discover(&worktree, None)
            .expect("linked worktree should be discovered");
        let git = worktree_discovery
            .git_checkout
            .as_ref()
            .expect("linked worktree Git metadata should exist");

        assert!(git.is_linked_worktree);
        assert_eq!(git.common_dir, fs::canonicalize(&main_git).unwrap());
        assert!(worktree_discovery.shares_git_common_dir_with(&main_discovery));

        fs::remove_dir_all(root).expect("temporary worktree fixture should be removed");
    }

    #[test]
    fn preserves_non_git_legacy_governance_roots() {
        let root = unique_temp_dir("project_discovery_legacy_non_git");
        let nested = root.join("src/nested");
        fs::create_dir_all(root.join("ant_delirium_config/type_1"))
            .expect("legacy config marker should be created");
        fs::create_dir_all(&nested).expect("legacy nested directory should be created");

        let discovered = ProjectDiscovery::discover(&nested, None)
            .expect("legacy non-Git project should be discovered");

        assert_eq!(discovered.project_root, fs::canonicalize(&root).unwrap());
        assert_eq!(discovered.cargo_kind, None);
        assert!(discovered.git_checkout.is_none());

        fs::remove_dir_all(root).expect("temporary legacy fixture should be removed");
    }
}
