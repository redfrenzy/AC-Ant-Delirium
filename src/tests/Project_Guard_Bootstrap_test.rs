#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

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
    fn creates_only_the_empty_public_projection_directory() {
        let (root, state_root) = fixture("create");
        let first = materialize_project_guard(&state_root).expect("first bootstrap");
        let second = materialize_project_guard(&state_root).expect("idempotent bootstrap");
        assert_eq!(
            first,
            ProjectGuardBootstrapReceipt {
                guard_directory_created: true,
                local_ignore_created: false,
            }
        );
        assert_eq!(
            second,
            ProjectGuardBootstrapReceipt {
                guard_directory_created: false,
                local_ignore_created: false,
            }
        );
        assert!(fs::read_dir(state_root.project_guard_dir())
            .unwrap()
            .next()
            .is_none());
        assert!(!state_root.project_root().join(".git").exists());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn does_not_inspect_or_modify_project_git_metadata_or_existing_files() {
        let (root, state_root) = fixture("opaque-git-metadata");
        let marker = state_root.project_root().join(".git");
        fs::write(&marker, b"unreadable and deliberately not parsed").unwrap();
        fs::create_dir(state_root.project_guard_dir()).unwrap();
        let existing = state_root.project_guard_dir().join("operator-note");
        fs::write(&existing, b"preserve").unwrap();

        let receipt = materialize_project_guard(&state_root).expect("filesystem-only bootstrap");
        assert!(!receipt.guard_directory_created);
        assert!(!receipt.local_ignore_created);
        assert_eq!(
            fs::read(marker).unwrap(),
            b"unreadable and deliberately not parsed"
        );
        assert_eq!(fs::read(existing).unwrap(), b"preserve");
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlinked_guard_without_touching_its_target() {
        let (root, state_root) = fixture("symlink");
        let external = root.join("external");
        fs::create_dir(&external).unwrap();
        symlink(&external, state_root.project_guard_dir()).unwrap();
        assert_eq!(
            materialize_project_guard(&state_root),
            Err(ProjectGuardBootstrapError::InvalidGuardDirectory)
        );
        assert!(fs::read_dir(&external).unwrap().next().is_none());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[test]
    fn concurrent_bootstrap_materializes_one_empty_directory() {
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
                .filter(|receipt| receipt.guard_directory_created)
                .count(),
            1
        );
        assert!(fs::read_dir(state_root.project_guard_dir())
            .unwrap()
            .next()
            .is_none());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
