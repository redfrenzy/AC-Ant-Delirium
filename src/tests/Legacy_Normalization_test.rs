use super::*;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn unique_root(label: &str) -> PathBuf {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/legacy-normalization-tests/{label}-{}-{now}-{sequence}",
        std::process::id(),
    ));
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    root
}

fn fixture_inventory(source: &Path) -> (Vec<u8>, String, Vec<u8>) {
    let uid = fs::metadata(source).unwrap().uid();
    let gid = fs::metadata(source).unwrap().gid();
    let paths = [
        "binary.bin",
        "context.json",
        "notes.md",
        "plan",
        "plan/task.json",
    ];
    let mut entries = Vec::new();
    let mut stream = Vec::new();
    for path in paths {
        let full = source.join(path);
        let metadata = fs::metadata(&full).unwrap();
        if metadata.is_dir() {
            entries.push(serde_json::json!({
                "path": path, "type": "directory", "mode": "700",
                "uid": uid, "gid": gid, "size": metadata.len(), "sha256": null,
                "link": {"kind": "none", "target": null, "target_sha256": null}
            }));
        } else {
            let bytes = fs::read(&full).unwrap();
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
        "validation": {"json_files": 2, "valid_json_files": 2, "invalid_json_files": 0},
        "counts": {"entries": 5, "regular_files": 4, "directories": 1, "symlinks": 0, "other": 0},
        "mode_histogram": [{"mode": "600", "count": 4}, {"mode": "700", "count": 1}],
        "ownership_histogram": [{"uid": uid, "gid": gid, "count": 5}],
        "semantic_counts": {
            "plans": 1, "tasks": 1, "submissions": 0, "reviews": 0,
            "history_events": 0, "problems": 0, "transitions": 0,
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

fn create_source(root: &Path) -> PathBuf {
    let source = root.join("origin");
    fs::create_dir(&source).unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(source.join("plan")).unwrap();
    fs::set_permissions(source.join("plan"), fs::Permissions::from_mode(0o700)).unwrap();
    let files: [(&str, &[u8]); 4] = [
        ("binary.bin", b"\xff\x00\x80"),
        (
            "context.json",
            br#"{"repository_path":"/home/private/workspace","relative":"plan/task.json"}"#,
        ),
        (
            "notes.md",
            b"Use /tmp/private/work and keep plan/task.json relative.\n",
        ),
        (
            "plan/task.json",
            br#"{"prompt":"stay in /home/private/workspace for this task"}"#,
        ),
    ];
    for (path, bytes) in files {
        fs::write(source.join(path), bytes).unwrap();
        fs::set_permissions(source.join(path), fs::Permissions::from_mode(0o600)).unwrap();
    }
    source
}

#[test]
fn normalization_is_path_free_atomic_and_semantically_equal() {
    let root = unique_root("equivalent");
    let source = create_source(&root);
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
    let request = prepare_source_normalization(&source, &inventory, &digest, &stage.migration_id)
        .expect("validated source normalization plan");
    let receipt = normalize_staged_migration(&staging_parent, &request)
        .expect("protected staging normalization");

    assert_eq!(receipt.comparison, "equivalent");
    assert_eq!(receipt.state, "normalized");
    assert_eq!(receipt.entries_compared, 5);
    assert_eq!(receipt.regular_files_compared, 4);
    assert_eq!(receipt.json_documents_compared, 2);
    assert_eq!(receipt.physical_references, 2);
    assert_eq!(receipt.physical_occurrences, 3);
    let normalized_root = staging_parent.join(&stage.migration_id).join("normalized");
    let context = fs::read_to_string(normalized_root.join("source/context.json")).unwrap();
    assert!(!context.contains("/home/"));
    assert!(context.contains("ref_"));
    assert!(context.contains("plan/task.json"));
    assert_eq!(
        fs::read(source.join("binary.bin")).unwrap(),
        b"\xff\x00\x80"
    );
    assert_eq!(
        fs::read(
            staging_parent
                .join(&stage.migration_id)
                .join("source/context.json")
        )
        .unwrap(),
        br#"{"repository_path":"/home/private/workspace","relative":"plan/task.json"}"#
    );
    let bindings =
        fs::read_to_string(normalized_root.join("physical-reference-bindings-v1.json")).unwrap();
    assert!(!bindings.contains("/home/"));
    assert!(!bindings.contains("/tmp/"));
    assert_eq!(
        normalize_staged_migration(&staging_parent, &request).unwrap(),
        receipt
    );
    let retry =
        prepare_source_normalization(&source, &inventory, &digest, &stage.migration_id).unwrap();
    assert_ne!(retry.physical_references, request.physical_references);
    assert_eq!(
        normalize_staged_migration(&staging_parent, &retry).unwrap(),
        receipt
    );
    fs::write(normalized_root.join("source/notes.md"), b"tampered").unwrap();
    assert_eq!(
        normalize_staged_migration(&staging_parent, &retry),
        Err(LegacyNormalizationError::Destination)
    );
}

#[test]
fn normalization_rejects_missing_mapping_and_tampered_staging() {
    let root = unique_root("reject");
    let source = create_source(&root);
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
    let mut request =
        prepare_source_normalization(&source, &inventory, &digest, &stage.migration_id).unwrap();
    request.physical_references.pop();
    assert_eq!(
        normalize_staged_migration(&staging_parent, &request),
        Err(LegacyNormalizationError::Mapping)
    );

    let request =
        prepare_source_normalization(&source, &inventory, &digest, &stage.migration_id).unwrap();
    fs::write(
        staging_parent
            .join(&stage.migration_id)
            .join("source/notes.md"),
        b"tampered",
    )
    .unwrap();
    assert_eq!(
        normalize_staged_migration(&staging_parent, &request),
        Err(LegacyNormalizationError::Staging)
    );
}

#[test]
fn physical_path_recognition_is_closed_and_leaves_relative_references() {
    let bytes = b"/home/a/x C:\\Users\\a\\x ~/work PWD=/tmp/work BIN=/usr/bin/tool #!/bin/sh /proc/$$/fd/* 2>/dev/null relative/file https://example.test/media/x";
    let values = physical_path_spans(bytes)
        .into_iter()
        .map(|(start, end)| String::from_utf8(bytes[start..end].to_vec()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        [
            "/home/a/x",
            "C:\\Users\\a\\x",
            "~/work",
            "/tmp/work",
            "/usr/bin/tool",
            "/bin/sh",
            "/proc/",
            "/dev/null"
        ]
    );
}

#[test]
#[ignore = "requires the operator-provided T8.2 origin and T8.3 staging locators"]
fn normalizes_the_real_t83_staging_against_the_t82_origin() {
    let source =
        PathBuf::from(std::env::var_os("ANTDELIRIUM_T84_SOURCE_ROOT").expect("T8.4 source root"));
    let inventory =
        fs::read(std::env::var_os("ANTDELIRIUM_T84_INVENTORY").expect("T8.4 inventory")).unwrap();
    let document: Value = serde_json::from_slice(&inventory).unwrap();
    let digest = document["inventory_sha256"].as_str().unwrap();
    let migration_id = std::env::var("ANTDELIRIUM_T84_MIGRATION_ID").unwrap();
    let staging_parent = PathBuf::from(
        std::env::var_os("ANTDELIRIUM_T84_STAGING_PARENT").expect("T8.4 staging parent"),
    );
    let request = prepare_source_normalization(&source, &inventory, digest, &migration_id).unwrap();
    let receipt = normalize_staged_migration(&staging_parent, &request).unwrap();
    println!("{}", serde_json::to_string(&receipt).unwrap());
}
