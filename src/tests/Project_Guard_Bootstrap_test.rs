#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use crate::structs::Project_Guard_Bootstrap_structcore::local_ignore_content;
    use crate::structs::{
        materialize_project_guard, ProjectGuardBootstrapError, ProjectGuardBootstrapReceipt,
        ProjectStateRoot,
    };

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture(label: &str, git: bool) -> (PathBuf, ProjectStateRoot) {
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/project-guard-bootstrap-tests/{label}-{}-{sequence}",
            std::process::id(),
        ));
        let project = root.join("project");
        fs::create_dir_all(&project).expect("project fixture");
        if git {
            let status = Command::new("git")
                .arg("init")
                .arg("--quiet")
                .arg(&project)
                .status()
                .expect("git init fixture");
            assert!(status.success());
        }
        let state_root =
            ProjectStateRoot::with_default_config(root.join("protected"), project.clone(), project);
        (root, state_root)
    }

    fn git_status(project: &Path) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(project)
            .arg("status")
            .arg("--porcelain")
            .arg("--untracked-files=all")
            .output()
            .expect("git status fixture");
        assert!(output.status.success());
        String::from_utf8(output.stdout).expect("UTF-8 git status")
    }

    #[test]
    fn creates_the_exact_self_contained_ignore_once_without_generating_projections() {
        let (root, state_root) = fixture("create", true);

        let first = materialize_project_guard(&state_root).expect("first bootstrap");
        let second = materialize_project_guard(&state_root).expect("idempotent bootstrap");

        assert!(first.guard_directory_created);
        assert!(first.git_repository);
        assert!(first.projections_ignored);
        assert!(first.local_ignore_created);
        assert_eq!(
            second,
            ProjectGuardBootstrapReceipt {
                guard_directory_created: false,
                git_repository: true,
                projections_ignored: true,
                local_ignore_created: false,
            }
        );
        assert_eq!(
            fs::read(state_root.project_guard_dir().join(".gitignore")).unwrap(),
            local_ignore_content()
        );
        assert!(!state_root.todo_projection_path().exists());
        assert!(!state_root.governance_error_projection_path().exists());
        assert!(!state_root.project_root().join(".gitignore").exists());
        assert!(git_status(state_root.project_root()).is_empty());

        fs::write(
            state_root.project_guard_dir().join("unexpected.txt"),
            b"preserve and expose",
        )
        .unwrap();
        assert_eq!(
            git_status(state_root.project_root()),
            "?? guard/unexpected.txt\n"
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn preserves_existing_root_rules_and_does_not_create_a_second_ignore() {
        let (root, state_root) = fixture("root-rules", true);
        fs::write(
            state_root.project_root().join(".gitignore"),
            b"/guard/to-do_list.json\n/guard/governance_error.json\n",
        )
        .unwrap();

        let receipt = materialize_project_guard(&state_root).expect("existing rules");

        assert!(receipt.guard_directory_created);
        assert!(receipt.git_repository);
        assert!(receipt.projections_ignored);
        assert!(!receipt.local_ignore_created);
        assert!(!state_root.project_guard_dir().join(".gitignore").exists());
        assert_eq!(
            fs::read(state_root.project_root().join(".gitignore")).unwrap(),
            b"/guard/to-do_list.json\n/guard/governance_error.json\n"
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn preserves_an_existing_local_ignore_even_when_the_project_chose_other_rules() {
        let (root, state_root) = fixture("existing-local", true);
        fs::create_dir(state_root.project_guard_dir()).unwrap();
        let existing = b"# project-owned decision\n/custom.json\n";
        fs::write(state_root.project_guard_dir().join(".gitignore"), existing).unwrap();

        let receipt = materialize_project_guard(&state_root).expect("preserve local ignore");

        assert!(!receipt.guard_directory_created);
        assert!(receipt.git_repository);
        assert!(!receipt.projections_ignored);
        assert!(!receipt.local_ignore_created);
        assert_eq!(
            fs::read(state_root.project_guard_dir().join(".gitignore")).unwrap(),
            existing
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn a_non_git_project_gets_only_an_empty_guard_directory() {
        let (root, state_root) = fixture("non-git", false);

        let receipt = materialize_project_guard(&state_root).expect("non-git bootstrap");

        assert!(receipt.guard_directory_created);
        assert!(!receipt.git_repository);
        assert!(!receipt.projections_ignored);
        assert!(!receipt.local_ignore_created);
        assert!(fs::read_dir(state_root.project_guard_dir())
            .unwrap()
            .next()
            .is_none());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn refuses_a_tracked_projection_without_changing_the_index_or_ignore_files() {
        let (root, state_root) = fixture("tracked", true);
        fs::create_dir(state_root.project_guard_dir()).unwrap();
        fs::write(state_root.todo_projection_path(), b"tracked projection").unwrap();
        let add = Command::new("git")
            .arg("-C")
            .arg(state_root.project_root())
            .arg("add")
            .arg("guard/to-do_list.json")
            .status()
            .expect("stage tracked fixture");
        assert!(add.success());
        let status_before = git_status(state_root.project_root());

        assert_eq!(
            materialize_project_guard(&state_root),
            Err(ProjectGuardBootstrapError::TrackedProjection)
        );
        assert_eq!(git_status(state_root.project_root()), status_before);
        assert_eq!(
            fs::read(state_root.todo_projection_path()).unwrap(),
            b"tracked projection"
        );
        assert!(!state_root.project_guard_dir().join(".gitignore").exists());
        assert!(!state_root.project_root().join(".gitignore").exists());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_guard_or_local_ignore_without_touching_the_target() {
        let (root, state_root) = fixture("symlinks", true);
        let external = root.join("external");
        fs::create_dir(&external).unwrap();
        symlink(&external, state_root.project_guard_dir()).unwrap();
        assert_eq!(
            materialize_project_guard(&state_root),
            Err(ProjectGuardBootstrapError::InvalidGuardDirectory)
        );
        assert!(fs::read_dir(&external).unwrap().next().is_none());
        fs::remove_file(state_root.project_guard_dir()).unwrap();

        fs::create_dir(state_root.project_guard_dir()).unwrap();
        let external_file = external.join("ignore-target");
        fs::write(&external_file, b"do not touch").unwrap();
        symlink(
            &external_file,
            state_root.project_guard_dir().join(".gitignore"),
        )
        .unwrap();
        assert_eq!(
            materialize_project_guard(&state_root),
            Err(ProjectGuardBootstrapError::InvalidLocalIgnore)
        );
        assert_eq!(fs::read(external_file).unwrap(), b"do not touch");
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn concurrent_bootstrap_creates_one_complete_ignore_without_overwrite() {
        let (root, state_root) = fixture("concurrent", true);
        let mut workers = Vec::new();
        for _ in 0..8 {
            let state_root = state_root.clone();
            workers.push(std::thread::spawn(move || {
                materialize_project_guard(&state_root).expect("concurrent bootstrap")
            }));
        }
        let receipts = workers
            .into_iter()
            .map(|worker| worker.join().expect("worker join"))
            .collect::<Vec<_>>();

        assert_eq!(
            receipts
                .iter()
                .filter(|receipt| receipt.local_ignore_created)
                .count(),
            1
        );
        assert!(receipts.iter().all(|receipt| receipt.projections_ignored));
        assert_eq!(
            fs::read(state_root.project_guard_dir().join(".gitignore")).unwrap(),
            local_ignore_content()
        );
        let names = fs::read_dir(state_root.project_guard_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![".gitignore"]);
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
