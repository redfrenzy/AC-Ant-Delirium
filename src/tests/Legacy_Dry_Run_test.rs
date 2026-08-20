use super::*;

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::legacy_normalization::{normalize_staged_migration, prepare_source_normalization};
use ac_ant_delirium::structs::StorageProject;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn unique_root(label: &str) -> PathBuf {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/legacy-dry-run-tests/{label}-{}-{now}-{sequence}",
        std::process::id(),
    ));
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    root
}

fn create_source(root: &Path) -> PathBuf {
    let source = root.join("origin");
    fs::create_dir(&source).unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
    for directory in ["global", "plan", "problems"] {
        fs::create_dir(source.join(directory)).unwrap();
        fs::set_permissions(source.join(directory), fs::Permissions::from_mode(0o700)).unwrap();
    }
    let files: [(&str, &[u8]); 4] = [
        (
            "global/transition_task.json",
            br#"{"transition_tasks":[{"transition_task_id":"x"}]}"#,
        ),
        (
            "governance_history.json",
            br#"{"entries":[{"id":"legacy-event"}]}"#,
        ),
        (
            "plan/one.json",
            br#"{"plan_id":"1.0","tasks":[{"step_id":"1.1","implemented":false}]}"#,
        ),
        (
            "problems/one.json",
            br#"{"category":"test","step_id":"1.1"}"#,
        ),
    ];
    for (relative, bytes) in files {
        fs::write(source.join(relative), bytes).unwrap();
        fs::set_permissions(source.join(relative), fs::Permissions::from_mode(0o600)).unwrap();
    }
    source
}

fn fixture_inventory(source: &Path) -> (Vec<u8>, String, Vec<u8>) {
    let uid = fs::metadata(source).unwrap().uid();
    let gid = fs::metadata(source).unwrap().gid();
    let paths = [
        "global",
        "global/transition_task.json",
        "governance_history.json",
        "plan",
        "plan/one.json",
        "problems",
        "problems/one.json",
    ];
    let mut entries = Vec::new();
    let mut stream = Vec::new();
    for path in paths {
        let metadata = fs::metadata(source.join(path)).unwrap();
        if metadata.is_dir() {
            entries.push(serde_json::json!({
                "path": path, "type": "directory", "mode": "700",
                "uid": uid, "gid": gid, "size": metadata.len(), "sha256": null,
                "link": {"kind": "none", "target": null, "target_sha256": null}
            }));
        } else {
            let bytes = fs::read(source.join(path)).unwrap();
            stream.extend_from_slice(&bytes);
            entries.push(serde_json::json!({
                "path": path, "type": "regular", "mode": "600",
                "uid": uid, "gid": gid, "size": bytes.len(),
                "sha256": lowercase_hex(&sha256(&bytes)),
                "link": {"kind": "none", "target": null, "target_sha256": null}
            }));
        }
    }
    let payload = serde_json::json!({
        "source_kind": "legacy-plugin",
        "source_locator": "operator-private",
        "root": {"type": "directory", "mode": "700", "uid": uid, "gid": gid},
        "validation": {"json_files": 4, "valid_json_files": 4, "invalid_json_files": 0},
        "counts": {"entries": 7, "regular_files": 4, "directories": 3, "symlinks": 0, "other": 0},
        "mode_histogram": [{"mode": "600", "count": 4}, {"mode": "700", "count": 3}],
        "ownership_histogram": [{"uid": uid, "gid": gid, "count": 7}],
        "semantic_counts": {
            "plans": 1, "tasks": 1, "submissions": 0, "reviews": 0,
            "history_events": 1, "problems": 1, "transitions": 1,
            "json_schemas": 0, "generated_artifacts": 0, "task_evidence": 0
        },
        "git": {"repository": false, "head": null, "branch": null, "worktree_clean": null, "status": []},
        "entries": entries
    });
    let mut canonical = serde_json::to_vec(&payload).unwrap();
    canonical.push(b'\n');
    let digest = lowercase_hex(&sha256(&canonical));
    let inventory = serde_json::to_vec_pretty(&serde_json::json!({
        "contract_version": "legacy-source-inventory-v1",
        "captured_at": "2026-08-20T00:00:00Z",
        "inventory_sha256": digest,
        "consistency": "two-identical-read-only-passes",
        "payload": payload
    }))
    .unwrap();
    (inventory, digest, stream)
}

fn normalized_fixture(root: &Path) -> (PathBuf, ProjectDatabase, LegacyDryRunRequest, PathBuf) {
    let source = create_source(root);
    let (inventory, digest, stream) = fixture_inventory(&source);
    let staging_parent = root.join("migration-staging");
    fs::create_dir(&staging_parent).unwrap();
    fs::set_permissions(&staging_parent, fs::Permissions::from_mode(0o700)).unwrap();
    let stage = crate::legacy_staging::stage_legacy_stream(
        &staging_parent,
        &inventory,
        &digest,
        &mut Cursor::new(stream),
    )
    .unwrap();
    let normalization =
        prepare_source_normalization(&source, &inventory, &digest, &stage.migration_id).unwrap();
    let receipt = normalize_staged_migration(&staging_parent, &normalization).unwrap();
    let registry_root = root.join("registry");
    fs::create_dir(&registry_root).unwrap();
    fs::set_permissions(&registry_root, fs::Permissions::from_mode(0o700)).unwrap();
    let registry = ProjectDatabase::initialize_at(&registry_root).unwrap();
    let request = dry_run_request(
        stage.migration_id.clone(),
        receipt.source_snapshot_sha256,
        receipt.staging_snapshot_sha256,
        receipt.normalized_snapshot_sha256,
        receipt.semantic_snapshot_sha256,
    );
    let migration_root = staging_parent.join(stage.migration_id);
    (staging_parent, registry, request, migration_root)
}

#[test]
fn dry_run_is_atomic_idempotent_and_never_approves_or_imports() {
    let root = unique_root("blocked-report");
    let (staging_parent, registry, request, migration_root) = normalized_fixture(&root);
    let manifest_before = fs::read(migration_root.join("migration-manifest-v1.json")).unwrap();
    let report = dry_run_legacy_migration(&staging_parent, &registry, &request).unwrap();

    assert_eq!(report.source_counts.plans, 1);
    assert_eq!(report.source_counts.tasks, 1);
    assert_eq!(report.source_counts.history_events, 1);
    assert_eq!(report.projected_counts, LegacyDomainCounts::zero());
    assert_eq!(report.differences.len(), 6);
    assert_eq!(report.comparison, "differences_found");
    assert_eq!(report.cutover_readiness, "blocked");
    assert!(!report.approval_eligible);
    assert!(!report.approval_created);
    assert!(!report.cutover_performed);
    assert_eq!(report.manifest_state_after, "staged");
    assert!(!report.target_project_preexisting);
    assert_eq!(registry.registered_projects().unwrap(), Vec::new());
    assert_eq!(
        fs::read(migration_root.join("migration-manifest-v1.json")).unwrap(),
        manifest_before
    );
    assert!(!migration_root.join("dry-run.partial").exists());
    assert!(migration_root
        .join("dry-run/shadow/state/state.db")
        .is_file());
    assert_eq!(
        dry_run_legacy_migration(&staging_parent, &registry, &request).unwrap(),
        report
    );

    fs::write(
        migration_root.join("normalized/source/governance_history.json"),
        b"tampered",
    )
    .unwrap();
    assert_eq!(
        dry_run_legacy_migration(&staging_parent, &registry, &request),
        Err(LegacyDryRunError::Normalization)
    );
}

#[test]
fn dry_run_binds_all_four_artifact_digests() {
    let root = unique_root("digest-binding");
    let (staging_parent, registry, mut request, _) = normalized_fixture(&root);
    request.expected_semantic_snapshot_sha256 = "a".repeat(64);
    assert_eq!(
        dry_run_legacy_migration(&staging_parent, &registry, &request),
        Err(LegacyDryRunError::Normalization)
    );
}

#[test]
fn dry_run_reports_a_preexisting_canonical_project_as_a_blocker() {
    let root = unique_root("project-conflict");
    let (staging_parent, registry, request, migration_root) = normalized_fixture(&root);
    let receipt: Value = serde_json::from_slice(
        &fs::read(migration_root.join("normalized/normalization-receipt-v1.json")).unwrap(),
    )
    .unwrap();
    registry
        .ensure_project(&StorageProject {
            project_id: receipt["project_id"].as_str().unwrap().to_string(),
            project_key: "preexisting-project".to_string(),
            created_at: "2026-08-20T00:00:00Z".to_string(),
        })
        .unwrap();

    let report = dry_run_legacy_migration(&staging_parent, &registry, &request).unwrap();
    assert!(report.target_project_preexisting);
    assert_eq!(
        report.differences[0].code,
        "target_project_identity_conflict"
    );
    assert!(!report
        .differences
        .iter()
        .any(|difference| difference.code == "workspace_identity_mapping_missing"));
    assert!(!report.approval_eligible);
}

#[test]
#[ignore = "requires the operator-provided T8.4 normalized staging locator"]
fn dry_runs_the_real_t84_autobrowser_generation() {
    let staging_parent = PathBuf::from(
        std::env::var_os("ANTDELIRIUM_T85_STAGING_PARENT").expect("T8.5 staging parent"),
    );
    let registry_root = PathBuf::from(
        std::env::var_os("ANTDELIRIUM_T85_REGISTRY_ROOT").expect("T8.5 registry root"),
    );
    fs::create_dir_all(&registry_root).unwrap();
    fs::set_permissions(&registry_root, fs::Permissions::from_mode(0o700)).unwrap();
    let registry = ProjectDatabase::initialize_at(&registry_root).unwrap();
    let request = dry_run_request(
        std::env::var("ANTDELIRIUM_T85_MIGRATION_ID").unwrap(),
        std::env::var("ANTDELIRIUM_T85_SOURCE_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T85_STAGING_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T85_NORMALIZED_SHA256").unwrap(),
        std::env::var("ANTDELIRIUM_T85_SEMANTIC_SHA256").unwrap(),
    );
    let report = dry_run_legacy_migration(&staging_parent, &registry, &request).unwrap();
    println!("{}", serde_json::to_string(&report).unwrap());
}
