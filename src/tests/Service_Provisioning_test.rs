use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ant_delirium::libres::Rusqlite_unit::AC_params;
use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::structs::{
    CapabilityActor, CapabilityChannel, CapabilityOperation, NewCapabilityGrant, ProjectDatabase,
};
use serde_json::json;

use super::{apply_manifest, InitialProvisioningError};
#[cfg(target_os = "linux")]
use super::{
    parse_posix_acl, require_exact_systemd_acl, PosixAclEntry, ACL_EXECUTE, ACL_GROUP_OBJ,
    ACL_MASK, ACL_OTHER, ACL_READ, ACL_UNDEFINED_ID, ACL_USER, ACL_USER_OBJ,
    POSIX_ACL_XATTR_VERSION,
};
use crate::limited_protocol::encode_base64url;

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fixture_root(label: &str) -> PathBuf {
    let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/service-provisioning-tests/{label}-{}-{nanos}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("project-local provisioning fixture");
    root
}

fn database(label: &str) -> ProjectDatabase {
    ProjectDatabase::initialize_at(&fixture_root(label)).expect("provisioning registry")
}

fn manifest(state_token: &[u8; 32], installation_token: &[u8; 32]) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "contract_version": "initial-capability-provisioning-v1",
        "provisioning_id": "prv_00000000000000000000000000",
        "issued_at_unix": 1_000,
        "expires_at_unix": 1_600,
        "grants": [
            {
                "capability_id": "cap_00000000000000000000000000",
                "token": encode_base64url(state_token),
                "subject_uid": 1000,
                "actor": "operator",
                "channel": "administrative",
                "operation": "state.admin",
                "project_id": null,
                "workspace_id": null,
                "task_id": null,
                "expires_at_unix": 86_400,
                "revocation_epoch": 0
            },
            {
                "capability_id": "cap_11111111111111111111111111",
                "token": encode_base64url(installation_token),
                "subject_uid": 1000,
                "actor": "operator",
                "channel": "administrative",
                "operation": "installation.admin",
                "project_id": null,
                "workspace_id": null,
                "task_id": null,
                "expires_at_unix": 86_400,
                "revocation_epoch": 0
            }
        ]
    }))
    .expect("manifest JSON")
}

#[test]
fn applies_initial_grants_once_and_persists_only_token_hashes() {
    let database = database("apply-once");
    let state_token = [7_u8; 32];
    let installation_token = [8_u8; 32];
    let bytes = manifest(&state_token, &installation_token);
    let expected_manifest_sha256 = sha256(&bytes);

    apply_manifest(&database, bytes.clone(), 1_100).expect("first provisioning");
    let receipt = database
        .initial_capability_provisioning_receipt()
        .expect("receipt query")
        .expect("receipt");
    assert_eq!(receipt.provisioning_id, "prv_00000000000000000000000000");
    assert_eq!(receipt.manifest_sha256, expected_manifest_sha256);
    assert_eq!(receipt.applied_at_unix, 1_100);
    assert_eq!(receipt.grant_count, 2);

    database
        .with_immediate_transaction(|transaction| {
            let count: i64 =
                transaction.query_row("SELECT count(*) FROM capability_grants", [], |row| {
                    row.get(0)
                })?;
            let state_hash: Vec<u8> = transaction.query_row(
                "SELECT secret_hash FROM capability_grants WHERE grant_id = ?1",
                AC_params!["cap_00000000000000000000000000"],
                |row| row.get(0),
            )?;
            assert_eq!(count, 2);
            assert_eq!(state_hash, sha256(&state_token));
            assert_ne!(state_hash, state_token);
            Ok(())
        })
        .unwrap();

    assert_eq!(
        apply_manifest(&database, bytes, 1_100),
        Err(InitialProvisioningError::Storage)
    );
}

#[test]
fn invalid_second_token_rolls_back_without_receipt_or_partial_grant() {
    let database = database("atomic-invalid");
    let mut value: serde_json::Value =
        serde_json::from_slice(&manifest(&[7; 32], &[8; 32])).unwrap();
    value["grants"][1]["token"] = json!("not-a-token");

    assert_eq!(
        apply_manifest(&database, serde_json::to_vec(&value).unwrap(), 1_100),
        Err(InitialProvisioningError::Manifest)
    );
    assert!(database
        .initial_capability_provisioning_receipt()
        .unwrap()
        .is_none());
    database
        .with_immediate_transaction(|transaction| {
            let count: i64 =
                transaction.query_row("SELECT count(*) FROM capability_grants", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(count, 0);
            Ok(())
        })
        .unwrap();
}

#[test]
fn refuses_to_claim_a_registry_that_already_contains_any_grant() {
    let database = database("dirty-registry");
    database
        .store_capability_grant(&NewCapabilityGrant {
            grant_id: "cap_22222222222222222222222222".to_string(),
            secret_hash: sha256(&[9; 32]),
            subject_uid: 1000,
            actor: CapabilityActor::Operator,
            channel: CapabilityChannel::Administrative,
            operation: CapabilityOperation::StateAdmin,
            project_id: None,
            workspace_id: None,
            task_id: None,
            not_before_unix: 1_000,
            expires_at_unix: 86_400,
            revocation_epoch: 0,
            granted_at: "unix:1000".to_string(),
        })
        .expect("pre-existing grant");

    assert_eq!(
        apply_manifest(&database, manifest(&[7; 32], &[8; 32]), 1_100),
        Err(InitialProvisioningError::Storage)
    );
    assert!(database
        .initial_capability_provisioning_receipt()
        .unwrap()
        .is_none());
}

#[test]
fn rejects_an_expired_or_overlong_bootstrap_envelope() {
    let expired = database("expired");
    assert_eq!(
        apply_manifest(&expired, manifest(&[7; 32], &[8; 32]), 1_600),
        Err(InitialProvisioningError::Manifest)
    );

    let overlong = database("overlong");
    let mut value: serde_json::Value =
        serde_json::from_slice(&manifest(&[7; 32], &[8; 32])).unwrap();
    value["expires_at_unix"] = json!(2_000);
    assert_eq!(
        apply_manifest(&overlong, serde_json::to_vec(&value).unwrap(), 1_100),
        Err(InitialProvisioningError::Manifest)
    );
}

#[test]
fn provisioning_errors_have_nonsecret_operator_codes() {
    let cases = [
        (
            InitialProvisioningError::CredentialContract,
            "AD-PROVISION-CREDENTIAL-CONTRACT",
        ),
        (
            InitialProvisioningError::CredentialAccess,
            "AD-PROVISION-CREDENTIAL-ACCESS",
        ),
        (InitialProvisioningError::Manifest, "AD-PROVISION-MANIFEST"),
        (
            InitialProvisioningError::AlreadyProvisioned,
            "AD-PROVISION-REPLAY",
        ),
        (
            InitialProvisioningError::MissingInitialCredential,
            "AD-PROVISION-MISSING",
        ),
        (InitialProvisioningError::Storage, "AD-PROVISION-STORAGE"),
        (InitialProvisioningError::Clock, "AD-PROVISION-CLOCK"),
    ];
    for (error, expected) in cases {
        assert_eq!(error.diagnostic_code(), expected);
        assert!(!expected.contains('/'));
        assert!(!expected.contains("token"));
    }
}

#[cfg(target_os = "linux")]
fn acl_bytes(entries: &[(u16, u16, u32)]) -> Vec<u8> {
    let mut bytes = POSIX_ACL_XATTR_VERSION.to_le_bytes().to_vec();
    for (tag, permissions, id) in entries {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&permissions.to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    bytes
}

#[cfg(target_os = "linux")]
fn systemd_acl(service_uid: u32, directory: bool) -> Vec<PosixAclEntry> {
    let allowed = ACL_READ | if directory { ACL_EXECUTE } else { 0 };
    vec![
        PosixAclEntry {
            tag: ACL_USER_OBJ,
            permissions: allowed,
            id: ACL_UNDEFINED_ID,
        },
        PosixAclEntry {
            tag: ACL_USER,
            permissions: allowed,
            id: service_uid,
        },
        PosixAclEntry {
            tag: ACL_GROUP_OBJ,
            permissions: 0,
            id: ACL_UNDEFINED_ID,
        },
        PosixAclEntry {
            tag: ACL_MASK,
            permissions: allowed,
            id: ACL_UNDEFINED_ID,
        },
        PosixAclEntry {
            tag: ACL_OTHER,
            permissions: 0,
            id: ACL_UNDEFINED_ID,
        },
    ]
}

#[test]
#[cfg(target_os = "linux")]
fn accepts_only_the_exact_systemd_named_user_acl() {
    let service_uid = 982;
    let mut directory_acl = systemd_acl(service_uid, true);
    directory_acl.reverse();
    require_exact_systemd_acl(&directory_acl, service_uid, true).expect("systemd directory ACL");
    require_exact_systemd_acl(&systemd_acl(service_uid, false), service_uid, false)
        .expect("systemd file ACL");

    let mut extra_user = systemd_acl(service_uid, false);
    extra_user.push(PosixAclEntry {
        tag: ACL_USER,
        permissions: ACL_READ,
        id: 1000,
    });
    assert_eq!(
        require_exact_systemd_acl(&extra_user, service_uid, false),
        Err(InitialProvisioningError::CredentialContract)
    );

    let mut writable_mask = systemd_acl(service_uid, false);
    writable_mask
        .iter_mut()
        .find(|entry| entry.tag == ACL_MASK)
        .expect("ACL mask")
        .permissions |= 0o2;
    assert_eq!(
        require_exact_systemd_acl(&writable_mask, service_uid, false),
        Err(InitialProvisioningError::CredentialContract)
    );
}

#[test]
#[cfg(target_os = "linux")]
fn parses_linux_acl_xattrs_and_rejects_malformed_contracts() {
    let expected = systemd_acl(982, false);
    let wire = acl_bytes(
        &expected
            .iter()
            .map(|entry| (entry.tag, entry.permissions, entry.id))
            .collect::<Vec<_>>(),
    );
    assert_eq!(parse_posix_acl(&wire).unwrap(), expected);

    let mut wrong_version = wire.clone();
    wrong_version[0..4].copy_from_slice(&3_u32.to_le_bytes());
    assert_eq!(
        parse_posix_acl(&wrong_version),
        Err(InitialProvisioningError::CredentialContract)
    );
    assert_eq!(
        parse_posix_acl(&wire[..wire.len() - 1]),
        Err(InitialProvisioningError::CredentialContract)
    );
}
