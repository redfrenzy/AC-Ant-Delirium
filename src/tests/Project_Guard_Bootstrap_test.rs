#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use crate::structs::Project_Guard_Bootstrap_structcore::local_ignore_content;
    use crate::structs::{
        materialize_project_guard, ProjectGuardBootstrapError, ProjectGuardBootstrapReceipt,
        ProjectStateRoot,
    };

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture(label: &str) -> (PathBuf, ProjectStateRoot) {
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let invocation = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("fixture clock")
            .as_nanos();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/project-guard-bootstrap-tests/{label}-{}-{invocation}-{sequence}",
            std::process::id(),
        ));
        let project = root.join("project");
        fs::create_dir_all(&project).expect("project fixture");
        let state_root =
            ProjectStateRoot::with_default_config(root.join("protected"), project.clone(), project);
        (root, state_root)
    }

    #[test]
    fn creates_the_exact_local_projection_ignore_without_git() {
        let (root, state_root) = fixture("create");

        let first = materialize_project_guard(&state_root).expect("first bootstrap");
        let second = materialize_project_guard(&state_root).expect("idempotent bootstrap");

        assert_eq!(
            first,
            ProjectGuardBootstrapReceipt {
                guard_directory_created: true,
                local_ignore_created: true,
            }
        );
        assert_eq!(
            second,
            ProjectGuardBootstrapReceipt {
                guard_directory_created: false,
                local_ignore_created: false,
            }
        );
        assert_eq!(
            fs::read(state_root.project_guard_dir().join(".gitignore")).unwrap(),
            local_ignore_content()
        );
        assert!(!state_root.todo_projection_path().exists());
        assert!(!state_root.governance_error_projection_path().exists());
        assert!(!state_root.project_root().join(".git").exists());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn does_not_inspect_or_modify_project_git_metadata() {
        let (root, state_root) = fixture("opaque-git-metadata");
        let marker = state_root.project_root().join(".git");
        fs::write(&marker, b"unreadable and deliberately not parsed").unwrap();

        let receipt = materialize_project_guard(&state_root).expect("filesystem-only bootstrap");

        assert!(receipt.guard_directory_created);
        assert!(receipt.local_ignore_created);
        assert_eq!(
            fs::read(marker).unwrap(),
            b"unreadable and deliberately not parsed"
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn preserves_an_existing_local_ignore() {
        let (root, state_root) = fixture("existing-local");
        fs::create_dir(state_root.project_guard_dir()).unwrap();
        let existing = b"# project-owned decision\n/custom.json\n";
        fs::write(state_root.project_guard_dir().join(".gitignore"), existing).unwrap();

        let receipt = materialize_project_guard(&state_root).expect("preserve local ignore");

        assert!(!receipt.guard_directory_created);
        assert!(!receipt.local_ignore_created);
        assert_eq!(
            fs::read(state_root.project_guard_dir().join(".gitignore")).unwrap(),
            existing
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_guard_or_local_ignore_without_touching_the_target() {
        let (root, state_root) = fixture("symlinks");
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
        let (root, state_root) = fixture("concurrent");
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
