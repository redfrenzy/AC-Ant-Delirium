use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::fd::OwnedFd;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const PROJECT_ID: &str = "prj_00000000000000000000000000";
const WORKSPACE_ID: &str = "wsp_00000000000000000000000000";
const TASK_ID: &str = "tsk_00000000000000000000000000";

fn unique_root(label: &str) -> PathBuf {
    let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/protected-git-backup-tests/{label}-{}-{sequence}",
        std::process::id(),
    ));
    fs::create_dir_all(&root).expect("backup fixture root");
    root
}

fn current_uid() -> u32 {
    #[cfg(unix)]
    {
        fs::metadata(".").unwrap().uid()
    }
    #[cfg(not(unix))]
    {
        0
    }
}

fn metadata(transition: BackupTransition, state_revision: i64) -> BackupCommitMetadata {
    BackupCommitMetadata {
        transition,
        project_id: PROJECT_ID.to_string(),
        workspace_id: WORKSPACE_ID.to_string(),
        task_id: (transition != BackupTransition::Baseline).then(|| TASK_ID.to_string()),
        task_revision: (transition != BackupTransition::Baseline).then_some(1),
        state_revision,
        occurred_at_unix: 1_788_000_000 + state_revision,
        observed_compilation_fingerprint: Some("a".repeat(64)),
        transition_id: match transition {
            BackupTransition::Baseline => None,
            BackupTransition::Submission => Some("sub_00000000000000000000000000".to_string()),
            BackupTransition::Review | BackupTransition::Advance => {
                Some("rev_00000000000000000000000000".to_string())
            }
        },
    }
}

fn entry(path: &str, contents: &[u8]) -> BackupSnapshotEntry {
    BackupSnapshotEntry {
        relative_path: path.to_string(),
        executable: false,
        contents: contents.to_vec(),
    }
}

#[test]
fn creates_an_exact_private_bare_repository_without_discovery() {
    let root = unique_root("exact-root");
    let unrelated = gix::init(&root).expect("unrelated parent repository");
    let backup = ProtectedGitBackup::initialize(&root, current_uid()).expect("private backup");

    assert!(backup.repository_path().join("HEAD").is_file());
    assert!(gix::open(backup.repository_path()).unwrap().is_bare());
    assert!(unrelated
        .try_find_reference(SNAPSHOT_REFERENCE)
        .unwrap()
        .is_none());
    #[cfg(unix)]
    {
        assert_eq!(
            fs::metadata(root.join(BACKUP_DIRECTORY_NAME))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(backup.repository_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}

#[test]
fn commits_only_the_validated_source_tree_and_opens_a_history_chain() {
    let root = unique_root("history");
    let backup = ProtectedGitBackup::initialize(&root, current_uid()).unwrap();
    let baseline = backup
        .commit_snapshot(
            &metadata(BackupTransition::Baseline, 0),
            vec![
                entry("Cargo.toml", b"[package]\n"),
                entry("src/lib.rs", b"pub fn first() {}\n"),
            ],
        )
        .unwrap();
    let advance = backup
        .commit_snapshot(
            &metadata(BackupTransition::Advance, 1),
            vec![
                entry("Cargo.toml", b"[package]\n"),
                entry("src/lib.rs", b"pub fn second() {}\n"),
            ],
        )
        .unwrap();

    assert_eq!(baseline.parent_commit_id, None);
    assert_eq!(
        advance.parent_commit_id.as_deref(),
        Some(baseline.commit_id.as_str())
    );
    assert_ne!(advance.source_fingerprint, baseline.source_fingerprint);
    assert_eq!(advance.entry_count, 2);

    let repository = gix::open(backup.repository_path()).unwrap();
    let commit = repository
        .find_reference(SNAPSHOT_REFERENCE)
        .unwrap()
        .peel_to_commit()
        .unwrap();
    assert_eq!(commit.id().to_string(), advance.commit_id);
    assert_eq!(commit.parent_ids().count(), 1);
    let tree = commit.tree().unwrap();
    assert!(tree
        .lookup_entry_by_path("source/Cargo.toml")
        .unwrap()
        .is_some());
    let source = tree
        .lookup_entry_by_path("source/src/lib.rs")
        .unwrap()
        .unwrap()
        .object()
        .unwrap()
        .into_blob()
        .data
        .clone();
    assert_eq!(source, b"pub fn second() {}\n");
    let snapshot = tree
        .lookup_entry_by_path("snapshot.json")
        .unwrap()
        .unwrap()
        .object()
        .unwrap()
        .into_blob()
        .data
        .clone();
    let snapshot: serde_json::Value = serde_json::from_slice(&snapshot).unwrap();
    assert_eq!(snapshot["contract_version"], "protected-git-snapshot-v1");
    assert_eq!(snapshot["transition"], "advance");
    assert_eq!(snapshot["source_fingerprint"], advance.source_fingerprint);
    assert_eq!(snapshot["observed_compilation_fingerprint"], "a".repeat(64));
    assert_eq!(snapshot["transition_id"], "rev_00000000000000000000000000");
    assert!(tree.lookup_entry_by_path("state").unwrap().is_none());
    assert!(tree.lookup_entry_by_path("capabilities").unwrap().is_none());
}

#[test]
fn rejects_paths_outside_the_closed_source_snapshot() {
    let root = unique_root("invalid-entry");
    let backup = ProtectedGitBackup::initialize(&root, current_uid()).unwrap();
    for invalid in [
        "/etc/passwd",
        "../secret",
        "src/../../secret",
        ".git/config",
        "target/debug/token",
        "guard/to-do_list.json",
        "AC_Ant-Delirium_props/plan/private.json",
    ] {
        assert_eq!(
            backup.commit_snapshot(
                &metadata(BackupTransition::Baseline, 0),
                vec![entry(invalid, b"secret")],
            ),
            Err(ProtectedGitBackupError::InvalidEntry),
            "path must be rejected: {invalid}",
        );
    }
    assert!(gix::open(backup.repository_path())
        .unwrap()
        .try_find_reference(SNAPSHOT_REFERENCE)
        .unwrap()
        .is_none());
}

#[test]
fn rejects_duplicate_entries_and_metadata_without_task_scope() {
    let root = unique_root("duplicates");
    let backup = ProtectedGitBackup::initialize(&root, current_uid()).unwrap();
    assert_eq!(
        backup.commit_snapshot(
            &metadata(BackupTransition::Baseline, 0),
            vec![entry("src/lib.rs", b"one"), entry("src/lib.rs", b"two")],
        ),
        Err(ProtectedGitBackupError::DuplicateEntry)
    );
    let mut invalid_metadata = metadata(BackupTransition::Advance, 1);
    invalid_metadata.task_id = None;
    invalid_metadata.task_revision = None;
    assert_eq!(
        backup.commit_snapshot(&invalid_metadata, vec![entry("src/lib.rs", b"one")]),
        Err(ProtectedGitBackupError::InvalidMetadata)
    );
}

#[cfg(unix)]
#[test]
fn rejects_a_symlinked_repository_root() {
    let root = unique_root("symlink");
    let outside = unique_root("symlink-outside");
    let backup_root = root.join(BACKUP_DIRECTORY_NAME);
    fs::create_dir(&backup_root).unwrap();
    symlink(&outside, backup_root.join(REPOSITORY_DIRECTORY_NAME)).unwrap();
    assert!(matches!(
        ProtectedGitBackup::initialize(&root, current_uid()),
        Err(ProtectedGitBackupError::InvalidRoot)
    ));
}

#[cfg(unix)]
fn open_directory(path: &Path) -> OwnedFd {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .unwrap()
        .into()
}

#[cfg(unix)]
#[test]
fn snapshots_a_project_descriptor_without_reading_excluded_roots() {
    let storage = unique_root("descriptor-storage");
    let workspace = unique_root("descriptor-workspace");
    fs::create_dir_all(workspace.join("src/nested")).unwrap();
    fs::create_dir_all(workspace.join("target/debug")).unwrap();
    fs::create_dir_all(workspace.join("guard")).unwrap();
    fs::create_dir_all(workspace.join(".git")).unwrap();
    fs::create_dir_all(workspace.join("plugins/private")).unwrap();
    fs::write(workspace.join("Cargo.toml"), b"[package]\n").unwrap();
    fs::write(workspace.join("src/lib.rs"), b"pub fn descriptor() {}\n").unwrap();
    fs::write(workspace.join("src/nested/data.bin"), b"data").unwrap();
    fs::write(workspace.join("target/debug/secret"), b"target-secret").unwrap();
    fs::write(workspace.join("guard/private"), b"guard-secret").unwrap();
    fs::write(workspace.join(".git/config"), b"git-secret").unwrap();
    fs::write(workspace.join("plugins/private/state"), b"plugin-secret").unwrap();

    let backup = ProtectedGitBackup::initialize(&storage, current_uid()).unwrap();
    let receipt = backup
        .commit_snapshot_from_descriptor(
            &metadata(BackupTransition::Baseline, 0),
            &open_directory(&workspace),
        )
        .unwrap();
    assert_eq!(receipt.entry_count, 3);

    let repository = gix::open(backup.repository_path()).unwrap();
    let tree = repository
        .find_reference(SNAPSHOT_REFERENCE)
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .tree()
        .unwrap();
    assert!(tree
        .lookup_entry_by_path("source/Cargo.toml")
        .unwrap()
        .is_some());
    assert!(tree
        .lookup_entry_by_path("source/src/lib.rs")
        .unwrap()
        .is_some());
    assert!(tree
        .lookup_entry_by_path("source/src/nested/data.bin")
        .unwrap()
        .is_some());
    for excluded in ["target", "guard", ".git", "plugins"] {
        assert!(tree
            .lookup_entry_by_path(format!("source/{excluded}"))
            .unwrap()
            .is_none());
    }
}

#[cfg(unix)]
#[test]
fn one_workspace_descriptor_can_produce_baseline_and_submission_snapshots() {
    let storage = unique_root("descriptor-reuse-storage");
    let workspace = unique_root("descriptor-reuse-workspace");
    fs::create_dir_all(workspace.join("src")).unwrap();
    fs::write(workspace.join("Cargo.toml"), b"[package]\n").unwrap();
    fs::write(
        workspace.join("src/lib.rs"),
        b"pub fn value() -> u8 { 1 }\n",
    )
    .unwrap();
    let descriptor = open_directory(&workspace);
    let backup = ProtectedGitBackup::initialize(&storage, current_uid()).unwrap();

    let baseline = backup
        .commit_snapshot_from_descriptor(&metadata(BackupTransition::Baseline, 0), &descriptor)
        .unwrap();
    fs::write(
        workspace.join("src/lib.rs"),
        b"pub fn value() -> u8 { 2 }\n",
    )
    .unwrap();
    let submission = backup
        .commit_snapshot_from_descriptor(&metadata(BackupTransition::Submission, 1), &descriptor)
        .unwrap();
    let replay = backup
        .commit_snapshot_from_descriptor(&metadata(BackupTransition::Submission, 1), &descriptor)
        .unwrap();

    assert_eq!(
        submission.parent_commit_id.as_deref(),
        Some(baseline.commit_id.as_str())
    );
    assert_ne!(submission.source_fingerprint, baseline.source_fingerprint);
    assert_eq!(replay, submission);

    fs::write(
        workspace.join("src/lib.rs"),
        b"pub fn value() -> u8 { 3 }\n",
    )
    .unwrap();
    assert_eq!(
        backup.commit_snapshot_from_descriptor(
            &metadata(BackupTransition::Submission, 1),
            &descriptor,
        ),
        Err(ProtectedGitBackupError::Commit),
    );
    let repository = gix::open(backup.repository_path()).unwrap();
    assert_eq!(
        repository
            .find_reference(SNAPSHOT_REFERENCE)
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string(),
        submission.commit_id,
    );
}

#[cfg(unix)]
#[test]
fn rejects_a_symlink_inside_the_snapshot_descriptor() {
    let storage = unique_root("descriptor-symlink-storage");
    let workspace = unique_root("descriptor-symlink-workspace");
    let outside = unique_root("descriptor-symlink-outside");
    fs::write(workspace.join("Cargo.toml"), b"[package]\n").unwrap();
    fs::write(outside.join("secret"), b"outside").unwrap();
    symlink(outside.join("secret"), workspace.join("linked-secret")).unwrap();

    let backup = ProtectedGitBackup::initialize(&storage, current_uid()).unwrap();
    assert_eq!(
        backup.commit_snapshot_from_descriptor(
            &metadata(BackupTransition::Baseline, 0),
            &open_directory(&workspace),
        ),
        Err(ProtectedGitBackupError::InvalidEntry)
    );
    assert!(gix::open(backup.repository_path())
        .unwrap()
        .try_find_reference(SNAPSHOT_REFERENCE)
        .unwrap()
        .is_none());
}
