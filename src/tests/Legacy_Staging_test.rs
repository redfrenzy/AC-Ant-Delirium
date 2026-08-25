use super::*;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::structs::{
    CapabilityActor, CapabilityAuthorizationOutcome, CapabilityAuthorizationRequest,
    CapabilityChannel, CapabilityOperation, NewCapabilityGrant, NewCapabilityRevocation,
};

use crate::service_authority::VerifiedServiceAuthority;
use crate::service_storage::ProtectedServiceStorage;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const GRANT_ID: &str = "cap_00000000000000000000000000";
const REQUEST_ID: &str = "req_00000000000000000000000000";

fn unique_root(label: &str) -> PathBuf {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/legacy-staging-tests/{label}-{}-{nonce}-{sequence}",
        std::process::id(),
    ));
    fs::create_dir_all(&root).expect("staging fixture root");
    root
}

fn authority() -> VerifiedServiceAuthority {
    VerifiedServiceAuthority::for_current_test_process().expect("Linux effective uid")
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_secs() as i64
}

fn authorize_operator(
    storage: &ProtectedServiceStorage,
    uid: u32,
) -> ac_ant_delirium::structs::AuthorizedCapability {
    let secret = [7_u8; 32];
    let nonce = [9_u8; 32];
    let now = unix_now();
    storage
        .registry()
        .store_capability_grant(&NewCapabilityGrant {
            grant_id: GRANT_ID.to_string(),
            secret_hash: sha256(&secret),
            subject_uid: uid,
            actor: CapabilityActor::Operator,
            channel: CapabilityChannel::Administrative,
            operation: CapabilityOperation::StateAdmin,
            project_id: None,
            workspace_id: None,
            task_id: None,
            not_before_unix: now - 60,
            expires_at_unix: now + 600,
            revocation_epoch: 0,
            granted_at: "2026-08-20T00:00:00Z".to_string(),
        })
        .expect("operator capability");
    let outcome = storage
        .registry()
        .authorize_capability(
            uid,
            CapabilityChannel::Administrative,
            &nonce,
            now,
            &CapabilityAuthorizationRequest {
                grant_id: GRANT_ID.to_string(),
                presented_secret: secret,
                operation: CapabilityOperation::StateAdmin,
                project_id: None,
                workspace_id: None,
                task_id: None,
                presented_connection_nonce: nonce,
                request_id: REQUEST_ID.to_string(),
                request_fingerprint: [10_u8; 32],
            },
        )
        .expect("authorized operator capability");
    match outcome {
        CapabilityAuthorizationOutcome::Fresh(capability) => capability,
        _ => panic!("fresh operator capability expected"),
    }
}

fn fixture_inventory(uid: u32, gid: u32) -> (Vec<u8>, String, Vec<u8>) {
    let history = br#"{"entries":[]}"#.to_vec();
    let plan = br#"{"tasks":[]}"#.to_vec();
    let entries = serde_json::json!([
        {
            "path": "governance_history.json",
            "type": "regular",
            "mode": "600",
            "uid": uid,
            "gid": gid,
            "size": history.len(),
            "sha256": lowercase_hex(&sha256(&history)),
            "link": {"kind": "none", "target": null, "target_sha256": null}
        },
        {
            "path": "plan",
            "type": "directory",
            "mode": "700",
            "uid": uid,
            "gid": gid,
            "size": 0,
            "sha256": null,
            "link": {"kind": "none", "target": null, "target_sha256": null}
        },
        {
            "path": "plan/[0.0]_Fixture.json",
            "type": "regular",
            "mode": "600",
            "uid": uid,
            "gid": gid,
            "size": plan.len(),
            "sha256": lowercase_hex(&sha256(&plan)),
            "link": {"kind": "none", "target": null, "target_sha256": null}
        }
    ]);
    let payload = serde_json::json!({
        "source_kind": "legacy-plugin",
        "source_locator": "operator-private",
        "root": {"type": "directory", "mode": "700", "uid": uid, "gid": gid},
        "validation": {"json_files": 2, "valid_json_files": 2, "invalid_json_files": 0},
        "counts": {"entries": 3, "regular_files": 2, "directories": 1, "symlinks": 0, "other": 0},
        "mode_histogram": [{"mode": "600", "count": 2}, {"mode": "700", "count": 1}],
        "ownership_histogram": [{"uid": uid, "gid": gid, "count": 3}],
        "semantic_counts": {
            "plans": 1,
            "tasks": 0,
            "submissions": 0,
            "reviews": 0,
            "history_events": 0,
            "problems": 0,
            "transitions": 0,
            "json_schemas": 0,
            "generated_artifacts": 0,
            "task_evidence": 0
        },
        "git": {"repository": false, "head": null, "branch": null, "worktree_clean": null, "status": []},
        "entries": entries
    });
    let mut canonical_payload = serde_json::to_vec(&payload).unwrap();
    canonical_payload.push(b'\n');
    let digest = lowercase_hex(&sha256(&canonical_payload));
    let document = serde_json::to_vec_pretty(&serde_json::json!({
        "contract_version": "legacy-source-inventory-v1",
        "captured_at": "2026-08-20T00:00:00Z",
        "inventory_sha256": digest,
        "consistency": "two-identical-read-only-passes",
        "payload": payload
    }))
    .unwrap();
    let mut stream = history;
    stream.extend_from_slice(&plan);
    (document, digest, stream)
}

#[test]
fn protected_service_stages_exact_bytes_and_path_free_manifests() {
    let root = unique_root("exact-copy");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let capability = authorize_operator(&storage, authority.effective_uid());
    let (inventory, digest, stream) =
        fixture_inventory(authority.effective_uid(), authority.effective_uid());

    let receipt = storage
        .stage_legacy_stream(
            &authority,
            &capability,
            &inventory,
            &digest,
            &mut Cursor::new(stream),
        )
        .expect("protected staging");
    let staged = root.join("migration-staging").join(&receipt.migration_id);
    assert_eq!(
        fs::read(staged.join("source/governance_history.json")).unwrap(),
        br#"{"entries":[]}"#
    );
    assert_eq!(
        fs::read(staged.join("source/plan/[0.0]_Fixture.json")).unwrap(),
        br#"{"tasks":[]}"#
    );
    assert_eq!(receipt.entries, 3);
    assert_eq!(receipt.regular_files, 2);
    assert_eq!(receipt.directories, 1);
    assert_eq!(receipt.state, "staged");
    let manifest = fs::read(staged.join("migration-manifest-v1.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(manifest["state"], "staged");
    assert_eq!(manifest["source_snapshot_sha256"], digest);
    assert!(!String::from_utf8_lossy(&serde_json::to_vec(&manifest).unwrap()).contains("/home/"));
    assert_eq!(
        fs::metadata(&staged).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(staged.join("source/governance_history.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn staging_rejects_wrong_digest_corrupt_stream_and_non_admin_capability() {
    let root = unique_root("rejections");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let capability = authorize_operator(&storage, authority.effective_uid());
    let (inventory, digest, mut stream) =
        fixture_inventory(authority.effective_uid(), authority.effective_uid());

    assert!(storage
        .stage_legacy_stream(
            &authority,
            &capability,
            &inventory,
            &"0".repeat(64),
            &mut Cursor::new(stream.clone()),
        )
        .is_err());
    stream[0] ^= 1;
    assert!(storage
        .stage_legacy_stream(
            &authority,
            &capability,
            &inventory,
            &digest,
            &mut Cursor::new(stream),
        )
        .is_err());

    storage
        .registry()
        .revoke_capability(&NewCapabilityRevocation {
            revocation_id: "rev_00000000000000000000000000".to_string(),
            grant_id: GRANT_ID.to_string(),
            revocation_epoch: 0,
            revoked_at: "2026-08-20T00:01:00Z".to_string(),
            reason: "test revocation".to_string(),
        })
        .unwrap();
    assert!(storage
        .stage_legacy_stream(
            &authority,
            &capability,
            &inventory,
            &digest,
            &mut Cursor::new(Vec::<u8>::new()),
        )
        .is_err());
}

#[test]
fn canonical_payload_digest_matches_the_t82_evidence() {
    let document: Value = serde_json::from_slice(include_bytes!(
        "../../docs/evidence/autobrowser-legacy-source-inventory-v1.json"
    ))
    .unwrap();
    let expected = document["inventory_sha256"].as_str().unwrap();
    let mut canonical_payload = serde_json::to_vec(&document["payload"]).unwrap();
    canonical_payload.push(b'\n');
    assert_eq!(lowercase_hex(&sha256(&canonical_payload)), expected);
}

#[cfg(unix)]
#[test]
fn source_descriptor_walk_rejects_a_symlinked_ancestor() {
    use std::os::unix::fs::symlink;

    let root = unique_root("symlinked-ancestor");
    let source = root.join("source");
    let outside = root.join("outside");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("task.json"), b"{}").unwrap();
    symlink(&outside, source.join("plan")).unwrap();
    let directory = File::open(&source).unwrap();

    assert!(matches!(
        open_source_entry_beneath(&directory, Path::new("plan/task.json"), false),
        Err(LegacyStagingError::SourceMismatch)
    ));
}

#[cfg(unix)]
#[test]
fn inherited_source_descriptor_survives_locator_removal() {
    let root = unique_root("descriptor-only-source");
    let source = root.join("source-locator-not-shared");
    fs::create_dir(&source).unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(source.join("plan")).unwrap();
    fs::set_permissions(source.join("plan"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(source.join("governance_history.json"), br#"{"entries":[]}"#).unwrap();
    fs::write(source.join("plan/[0.0]_Fixture.json"), br#"{"tasks":[]}"#).unwrap();
    fs::set_permissions(
        source.join("governance_history.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fs::set_permissions(
        source.join("plan/[0.0]_Fixture.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();

    let directory = File::open(&source).unwrap();
    fs::rename(&source, root.join("operator-private-source")).unwrap();
    let uid = directory.metadata().unwrap().uid();
    let gid = directory.metadata().unwrap().gid();
    let (inventory, _, expected_stream) = fixture_inventory(uid, gid);
    let mut inventory_document: Value = serde_json::from_slice(&inventory).unwrap();
    inventory_document["payload"]["entries"][1]["size"] = Value::from(
        root.join("operator-private-source/plan")
            .metadata()
            .unwrap()
            .len(),
    );
    let mut canonical_payload = serde_json::to_vec(&inventory_document["payload"]).unwrap();
    canonical_payload.push(b'\n');
    let digest = lowercase_hex(&sha256(&canonical_payload));
    inventory_document["inventory_sha256"] = Value::from(digest.clone());
    let inventory = serde_json::to_vec_pretty(&inventory_document).unwrap();
    let mut stream = Vec::new();
    write_validated_source_stream_from_directory(&directory, &inventory, &digest, &mut stream)
        .unwrap();
    assert_eq!(stream, expected_stream);
}

#[test]
#[ignore = "requires operator-provided T8.2 source and inventory locators"]
fn stages_the_real_t82_snapshot_through_protected_storage() {
    let source =
        PathBuf::from(std::env::var_os("ANTDELIRIUM_T83_SOURCE_ROOT").expect("source root"));
    let inventory_path =
        PathBuf::from(std::env::var_os("ANTDELIRIUM_T83_INVENTORY").expect("inventory path"));
    let inventory = fs::read(inventory_path).expect("inventory bytes");
    let document: Value = serde_json::from_slice(&inventory).expect("inventory JSON");
    let digest = document["inventory_sha256"].as_str().unwrap().to_string();
    let source_directory = File::open(&source).expect("source directory descriptor");
    let mut stream = Vec::new();
    write_validated_source_stream_from_directory(
        &source_directory,
        &inventory,
        &digest,
        &mut stream,
    )
    .expect("descriptor-bound source still matches T8.2");

    let root = unique_root("real-autobrowser-copy");
    let authority = authority();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let capability = authorize_operator(&storage, authority.effective_uid());
    let receipt = storage
        .stage_legacy_stream(
            &authority,
            &capability,
            &inventory,
            &digest,
            &mut Cursor::new(stream),
        )
        .expect("real protected staging");
    println!("{}", serde_json::to_string(&receipt).unwrap());
}
