use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("test clock after epoch")
        .as_nanos();
    let parent =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/protected-git-backup-tests");
    fs::create_dir_all(&parent).expect("backup fixture parent");
    let root = parent.join(format!("{label}-{}-{sequence}-{nonce}", std::process::id(),));
    fs::create_dir(&root).expect("unique backup fixture root");
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

fn executable_entry(path: &str, contents: &[u8]) -> BackupSnapshotEntry {
    BackupSnapshotEntry {
        relative_path: path.to_string(),
        executable: true,
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
    assert_eq!(snapshot["contract_version"], SNAPSHOT_CONTRACT_VERSION);
    assert_eq!(snapshot["source_policy"], SOURCE_SNAPSHOT_POLICY);
    assert_eq!(snapshot["transition"], "advance");
    assert_eq!(snapshot["source_fingerprint"], advance.source_fingerprint);
    assert_eq!(snapshot["observed_compilation_fingerprint"], "a".repeat(64));
    assert_eq!(snapshot["transition_id"], "rev_00000000000000000000000000");
    assert!(tree.lookup_entry_by_path("state").unwrap().is_none());
    assert!(tree.lookup_entry_by_path("capabilities").unwrap().is_none());
}

#[test]
fn recovers_the_exact_baseline_binding_from_the_private_history() {
    let root = unique_root("recover-baseline");
    let backup = ProtectedGitBackup::initialize(&root, current_uid()).unwrap();
    let expected_metadata = BackupCommitMetadata {
        task_id: Some(TASK_ID.to_string()),
        task_revision: Some(1),
        ..metadata(BackupTransition::Baseline, 1)
    };
    let expected_receipt = backup
        .commit_snapshot(
            &expected_metadata,
            vec![entry("src/lib.rs", b"pub fn baseline() {}\n")],
        )
        .unwrap();

    let recovered = backup.recover_baseline_checkpoint().unwrap().unwrap();
    assert_eq!(recovered.metadata, expected_metadata);
    assert_eq!(recovered.receipt, expected_receipt);

    fs::remove_dir_all(root).unwrap();
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
        "vendor/dependency/.git/config",
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
    fs::create_dir_all(workspace.join("AC_Ant-Delirium_props/plan")).unwrap();
    fs::create_dir_all(workspace.join("src/.git")).unwrap();
    fs::create_dir_all(workspace.join("src/target")).unwrap();
    fs::write(workspace.join("Cargo.toml"), b"[package]\n").unwrap();
    fs::write(workspace.join(".env"), b"PROJECT_SETTING=checked-in\n").unwrap();
    fs::write(workspace.join("src/lib.rs"), b"pub fn descriptor() {}\n").unwrap();
    fs::write(workspace.join("src/nested/data.bin"), b"data").unwrap();
    fs::write(workspace.join("src/target/source.txt"), b"nested-source").unwrap();
    fs::write(workspace.join("src/.git/config"), b"nested-git-secret").unwrap();
    fs::write(workspace.join("target/debug/secret"), b"target-secret").unwrap();
    fs::write(workspace.join("guard/private"), b"guard-secret").unwrap();
    fs::write(workspace.join(".git/config"), b"git-secret").unwrap();
    fs::write(workspace.join("plugins/private/state"), b"plugin-secret").unwrap();
    fs::write(
        workspace.join("AC_Ant-Delirium_props/plan/private.json"),
        b"plan-secret",
    )
    .unwrap();

    let backup = ProtectedGitBackup::initialize(&storage, current_uid()).unwrap();
    let receipt = backup
        .commit_snapshot_from_descriptor(
            &metadata(BackupTransition::Baseline, 0),
            &open_directory(&workspace),
        )
        .unwrap();
    assert_eq!(receipt.entry_count, 5);

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
    assert!(tree.lookup_entry_by_path("source/.env").unwrap().is_some());
    assert!(tree
        .lookup_entry_by_path("source/src/target/source.txt")
        .unwrap()
        .is_some());
    assert!(tree
        .lookup_entry_by_path("source/src/.git")
        .unwrap()
        .is_none());
    for excluded in [
        "target",
        "guard",
        ".git",
        "plugins",
        "AC_Ant-Delirium_props",
    ] {
        assert!(tree
            .lookup_entry_by_path(format!("source/{excluded}"))
            .unwrap()
            .is_none());
    }
}

#[test]
fn accepts_only_the_versioned_source_policy_and_legacy_v1_without_a_policy_field() {
    let base = serde_json::json!({
        "contract_version": LEGACY_SNAPSHOT_CONTRACT_VERSION,
        "transition": "baseline",
        "project_id": PROJECT_ID,
        "workspace_id": WORKSPACE_ID,
        "task_id": null,
        "task_revision": null,
        "state_revision": 0,
        "occurred_at": "unix:1788000000",
        "source_fingerprint": "a".repeat(64),
        "observed_compilation_fingerprint": "a".repeat(64),
        "transition_id": null,
        "entry_count": 1,
        "byte_count": 1
    });
    let legacy: StoredSnapshotMetadata = serde_json::from_value(base.clone()).unwrap();
    assert!(valid_snapshot_contract(&legacy));

    let mut current = base.clone();
    current["contract_version"] = SNAPSHOT_CONTRACT_VERSION.into();
    current["source_policy"] = SOURCE_SNAPSHOT_POLICY.into();
    let current: StoredSnapshotMetadata = serde_json::from_value(current).unwrap();
    assert!(valid_snapshot_contract(&current));

    let mut missing_policy = base.clone();
    missing_policy["contract_version"] = SNAPSHOT_CONTRACT_VERSION.into();
    let missing_policy: StoredSnapshotMetadata = serde_json::from_value(missing_policy).unwrap();
    assert!(!valid_snapshot_contract(&missing_policy));

    let mut changed_policy = base;
    changed_policy["contract_version"] = SNAPSHOT_CONTRACT_VERSION.into();
    changed_policy["source_policy"] = "include-everything".into();
    let changed_policy: StoredSnapshotMetadata = serde_json::from_value(changed_policy).unwrap();
    assert!(!valid_snapshot_contract(&changed_policy));
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

#[test]
fn review_and_advance_reuse_the_protected_source_without_a_workspace_descriptor() {
    let storage = unique_root("metadata-only-transitions");
    let backup = ProtectedGitBackup::initialize(&storage, current_uid()).unwrap();
    let baseline = backup
        .commit_snapshot(
            &metadata(BackupTransition::Baseline, 0),
            vec![entry("src/lib.rs", b"pub fn value() -> u8 { 1 }\n")],
        )
        .unwrap();
    let submission = backup
        .commit_snapshot(
            &metadata(BackupTransition::Submission, 1),
            vec![entry("src/lib.rs", b"pub fn value() -> u8 { 2 }\n")],
        )
        .unwrap();
    let mut newer_submission_metadata = metadata(BackupTransition::Submission, 1);
    newer_submission_metadata.transition_id = Some("sub_11111111111111111111111111".to_string());
    let newer_submission = backup
        .commit_snapshot(
            &newer_submission_metadata,
            vec![entry("src/lib.rs", b"pub fn value() -> u8 { 3 }\n")],
        )
        .unwrap();
    let review_metadata = metadata(BackupTransition::Review, 1);
    let review = backup
        .commit_snapshot_from_transition(
            &review_metadata,
            BackupTransition::Submission,
            "sub_00000000000000000000000000",
        )
        .unwrap();
    let advance_metadata = metadata(BackupTransition::Advance, 2);
    let advance = backup
        .commit_snapshot_from_transition(
            &advance_metadata,
            BackupTransition::Review,
            "rev_00000000000000000000000000",
        )
        .unwrap();

    assert_eq!(
        review.parent_commit_id.as_deref(),
        Some(newer_submission.commit_id.as_str())
    );
    assert_eq!(
        advance.parent_commit_id.as_deref(),
        Some(review.commit_id.as_str())
    );
    assert_ne!(submission.source_fingerprint, baseline.source_fingerprint);
    assert_ne!(
        newer_submission.source_fingerprint,
        submission.source_fingerprint
    );
    assert_eq!(review.source_fingerprint, submission.source_fingerprint);
    assert_eq!(advance.source_fingerprint, submission.source_fingerprint);
    assert_eq!(
        backup
            .commit_snapshot_from_transition(
                &review_metadata,
                BackupTransition::Submission,
                "sub_00000000000000000000000000",
            )
            .unwrap(),
        review,
    );
    assert_eq!(
        backup
            .commit_snapshot_from_transition(
                &advance_metadata,
                BackupTransition::Review,
                "rev_00000000000000000000000000",
            )
            .unwrap(),
        advance,
    );
    let recovered = backup.recover_checkpoint_chain().unwrap();
    assert_eq!(recovered.len(), 5);
    assert_eq!(recovered[0].receipt.commit_id, baseline.commit_id);
    assert_eq!(recovered[1].receipt.commit_id, submission.commit_id);
    assert_eq!(recovered[2].receipt.commit_id, newer_submission.commit_id);
    assert_eq!(recovered[3].receipt.commit_id, review.commit_id);
    assert_eq!(recovered[4].receipt.commit_id, advance.commit_id);
    assert_eq!(recovered[4].metadata.transition, BackupTransition::Advance);

    let repository = gix::open(backup.repository_path()).unwrap();
    let source_at = |commit_id: &str| {
        repository
            .find_object(gix::ObjectId::from_hex(commit_id.as_bytes()).unwrap())
            .unwrap()
            .into_commit()
            .tree()
            .unwrap()
            .lookup_entry_by_path("source")
            .unwrap()
            .unwrap()
            .oid()
            .to_string()
    };
    assert_eq!(
        source_at(&review.commit_id),
        source_at(&submission.commit_id)
    );
    assert_eq!(
        source_at(&advance.commit_id),
        source_at(&submission.commit_id)
    );
}

#[test]
fn derives_a_bounded_source_context_only_from_private_commit_trees() {
    let storage = unique_root("private-source-context");
    let backup = ProtectedGitBackup::initialize(&storage, current_uid()).unwrap();
    backup
        .commit_snapshot(
            &metadata(BackupTransition::Baseline, 0),
            vec![
                entry("src/deleted.rs", b"deleted\n"),
                entry("src/mode.sh", b"#!/bin/sh\n"),
                entry("src/modified.rs", b"before\n"),
                entry("src/unchanged.rs", b"same\n"),
            ],
        )
        .unwrap();
    backup
        .commit_snapshot(
            &metadata(BackupTransition::Submission, 1),
            vec![
                entry("src/added.rs", b"added\n"),
                executable_entry("src/mode.sh", b"#!/bin/sh\n"),
                entry("src/modified.rs", b"after\n"),
                entry("src/unchanged.rs", b"same\n"),
            ],
        )
        .unwrap();

    let context = backup
        .source_context_for_transition(
            BackupTransition::Submission,
            "sub_00000000000000000000000000",
        )
        .unwrap();
    assert_eq!(context.contract_version, "protected-source-context-v1");
    assert_eq!(context.changed_file_count, 4);
    assert!(!context.changes_truncated);
    assert_eq!(
        context.changes,
        vec![
            ProtectedSourceChange {
                relative_path: "src/added.rs".to_string(),
                kind: ProtectedSourceChangeKind::Added,
            },
            ProtectedSourceChange {
                relative_path: "src/deleted.rs".to_string(),
                kind: ProtectedSourceChangeKind::Deleted,
            },
            ProtectedSourceChange {
                relative_path: "src/mode.sh".to_string(),
                kind: ProtectedSourceChangeKind::ModeChanged,
            },
            ProtectedSourceChange {
                relative_path: "src/modified.rs".to_string(),
                kind: ProtectedSourceChangeKind::Modified,
            },
        ]
    );
    let encoded = serde_json::to_string(&context).unwrap();
    assert!(!encoded.contains(storage.to_string_lossy().as_ref()));
    assert!(!encoded.contains("repository.git"));
}

#[test]
fn compares_a_submission_with_its_own_workspace_when_histories_are_interleaved() {
    let storage = unique_root("interleaved-source-context");
    let backup = ProtectedGitBackup::initialize(&storage, current_uid()).unwrap();
    backup
        .commit_snapshot(
            &metadata(BackupTransition::Baseline, 0),
            vec![entry("src/lib.rs", b"workspace-one\n")],
        )
        .unwrap();
    let mut other_baseline = metadata(BackupTransition::Baseline, 0);
    other_baseline.workspace_id = "wsp_11111111111111111111111111".to_string();
    backup
        .commit_snapshot(
            &other_baseline,
            vec![entry("src/lib.rs", b"workspace-two\n")],
        )
        .unwrap();
    backup
        .commit_snapshot(
            &metadata(BackupTransition::Submission, 1),
            vec![entry("src/lib.rs", b"workspace-one\n")],
        )
        .unwrap();

    let context = backup
        .source_context_for_transition(
            BackupTransition::Submission,
            "sub_00000000000000000000000000",
        )
        .unwrap();
    assert_eq!(context.workspace_id, WORKSPACE_ID);
    assert_eq!(context.changed_file_count, 0);
    assert!(context.changes.is_empty());
}

#[test]
fn bounds_the_rendered_change_list_without_losing_the_total_count() {
    let storage = unique_root("bounded-source-context");
    let backup = ProtectedGitBackup::initialize(&storage, current_uid()).unwrap();
    backup
        .commit_snapshot(
            &metadata(BackupTransition::Baseline, 0),
            vec![entry("Cargo.toml", b"[package]\n")],
        )
        .unwrap();
    let mut submitted = vec![entry("Cargo.toml", b"[package]\n")];
    for ordinal in 0..=MAX_SOURCE_CONTEXT_CHANGES {
        submitted.push(entry(
            &format!("src/generated-{ordinal:04}.rs"),
            b"source\n",
        ));
    }
    backup
        .commit_snapshot(&metadata(BackupTransition::Submission, 1), submitted)
        .unwrap();

    let context = backup
        .source_context_for_transition(
            BackupTransition::Submission,
            "sub_00000000000000000000000000",
        )
        .unwrap();
    assert_eq!(
        context.changed_file_count,
        (MAX_SOURCE_CONTEXT_CHANGES + 1) as u64
    );
    assert_eq!(context.changes.len(), MAX_SOURCE_CONTEXT_CHANGES);
    assert!(context.changes_truncated);
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
