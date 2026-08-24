use super::*;

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::fd::IntoRawFd;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[cfg(unix)]
use ac_ant_delirium::structs::{
    CapabilityActor, CapabilityAuthorizationOutcome, CapabilityAuthorizationRequest,
    CapabilityChannel, CapabilityOperation, NewCapabilityGrant,
};

#[cfg(unix)]
use crate::service_authentication::ConnectionAuthentication;
#[cfg(unix)]
use crate::service_authority::VerifiedServiceAuthority;
#[cfg(unix)]
use crate::service_storage::ProtectedServiceStorage;

#[cfg(unix)]
const ADMIN_SOCKET_CHILD: &str = "ANTDELIRIUM_ADMIN_SOCKET_TEST_CHILD";

#[cfg(unix)]
fn enter_isolated_socket_test(label: &str, exact_test_name: &str) -> bool {
    if std::env::var_os(ADMIN_SOCKET_CHILD).is_some_and(|value| value == label) {
        return true;
    }
    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--exact")
        .arg(exact_test_name)
        .arg("--nocapture")
        .env(ADMIN_SOCKET_CHILD, label)
        .output()
        .expect("isolated administrative socket test");
    assert!(
        output.status.success(),
        "isolated administrative socket test failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

#[cfg(unix)]
fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(unix)]
#[test]
fn administrative_handshake_negotiates_only_ada1() {
    if !enter_isolated_socket_test(
        "handshake",
        "administrative_protocol::administrative_protocol_tests::administrative_handshake_negotiates_only_ada1",
    ) {
        return;
    }
    let (mut client, mut server) = UnixStream::pair().unwrap();
    let nonce = [9_u8; 32];
    let server_thread = std::thread::spawn(move || {
        answer_administrative_hello(&mut server, &nonce).unwrap();
    });
    let negotiated = negotiate_client(&mut client).unwrap();
    assert_eq!(decode_base64url(&negotiated).unwrap(), nonce);
    server_thread.join().unwrap();
}

#[cfg(unix)]
#[test]
fn administrative_framing_rejects_limited_channel_magic() {
    if !enter_isolated_socket_test(
        "framing",
        "administrative_protocol::administrative_protocol_tests::administrative_framing_rejects_limited_channel_magic",
    ) {
        return;
    }
    let (mut client, mut server) = UnixStream::pair().unwrap();
    write_all_before(
        &mut client,
        b"ADL1\0\0\0\x02{}",
        Instant::now() + ADMIN_TIMEOUT,
    )
    .unwrap();
    assert_eq!(
        read_frame(&mut server),
        Err(AdministrativeProtocolError::Framing)
    );
}

#[test]
fn migration_request_binds_connection_operation_and_inventory_digest() {
    let nonce = [4_u8; 32];
    let digest = "a".repeat(64);
    let mut request = AdministrativeRequest {
        message_type: "request".to_string(),
        wire_version: ADMIN_WIRE_VERSION.to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        connection_nonce: encode_base64url(&nonce),
        operation: "migration.stage".to_string(),
        capability: WireCapability {
            capability_id: "cap_00000000000000000000000000".to_string(),
            token: encode_base64url(&[7_u8; 32]),
        },
        body: AdministrativeBody::Stage(MigrationStageBody {
            expected_source_snapshot_sha256: digest.clone(),
            inventory: serde_json::json!({"inventory_sha256": digest}),
        }),
    };
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    request.operation = "migration.commit".to_string();
    assert_eq!(
        validate_request(&request, &nonce),
        Err(AdministrativeProtocolError::Envelope)
    );
    request.operation = "migration.normalize".to_string();
    assert_eq!(
        validate_request(&request, &nonce),
        Err(AdministrativeProtocolError::Envelope)
    );
    request.body = AdministrativeBody::Normalize(LegacyNormalizationRequest {
        contract_version: "legacy-normalization-request-v1".to_string(),
        migration_id: "mig_00000000000000000000000000".to_string(),
        expected_source_snapshot_sha256: "a".repeat(64),
        source_normalized_snapshot_sha256: "b".repeat(64),
        source_semantic_snapshot_sha256: "c".repeat(64),
        physical_references: Vec::new(),
    });
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    request.operation = "migration.dry-run".to_string();
    request.body = AdministrativeBody::DryRun(crate::legacy_dry_run::dry_run_request(
        "mig_00000000000000000000000000".to_string(),
        "a".repeat(64),
        "b".repeat(64),
        "c".repeat(64),
        "d".repeat(64),
    ));
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    request.operation = "migration.convert".to_string();
    request.body = AdministrativeBody::Convert(crate::legacy_conversion::conversion_request(
        "mig_00000000000000000000000000".to_string(),
        "a".repeat(64),
        "b".repeat(64),
        "c".repeat(64),
        "d".repeat(64),
        "e".repeat(64),
    ));
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    request.operation = "migration.attest".to_string();
    request.body = AdministrativeBody::Attest(crate::legacy_attestation::attestation_request(
        "mig_00000000000000000000000000".to_string(),
        "prj_00000000000000000000000000".to_string(),
        "wsp_00000000000000000000000000".to_string(),
        "f".repeat(64),
    ));
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    let authorization = capability_request(
        &ConnectionAuthentication::for_test(1000, [4_u8; 32]),
        &request,
    )
    .unwrap();
    assert_eq!(
        authorization.operation,
        CapabilityOperation::AttestationCreate
    );
    assert_eq!(
        authorization.project_id.as_deref(),
        Some("prj_00000000000000000000000000")
    );
    assert_eq!(
        authorization.workspace_id.as_deref(),
        Some("wsp_00000000000000000000000000")
    );
    request.operation = "migration.approve".to_string();
    request.body = AdministrativeBody::Approve(crate::legacy_approval::approval_request(
        "mig_00000000000000000000000000".to_string(),
        "prj_00000000000000000000000000".to_string(),
        "wsp_00000000000000000000000000".to_string(),
        "1".repeat(64),
        "2".repeat(64),
        "3".repeat(64),
        "4".repeat(64),
        "att_00000000000000000000000000".to_string(),
        "5".repeat(64),
        "6".repeat(64),
        "2".repeat(64),
    ));
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    let authorization = capability_request(
        &ConnectionAuthentication::for_test(1000, [4_u8; 32]),
        &request,
    )
    .unwrap();
    assert_eq!(authorization.operation, CapabilityOperation::StateAdmin);
    assert!(authorization.project_id.is_none());
    assert!(authorization.workspace_id.is_none());
    request.operation = "migration.cutover".to_string();
    request.body = AdministrativeBody::Cutover(crate::legacy_cutover::cutover_request(
        "mig_00000000000000000000000000".to_string(),
        "prj_00000000000000000000000000".to_string(),
        "wsp_00000000000000000000000000".to_string(),
        "1".repeat(64),
        "2".repeat(64),
        "3".repeat(64),
        "4".repeat(64),
        "att_00000000000000000000000000".to_string(),
        "5".repeat(64),
        "apr_00000000000000000000000000".to_string(),
        "6".repeat(64),
        "7".repeat(64),
        "2".repeat(64),
        crate::legacy_cutover::MINIMUM_ROLLBACK_WINDOW_SECONDS,
    ));
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    let authorization = capability_request(
        &ConnectionAuthentication::for_test(1000, [4_u8; 32]),
        &request,
    )
    .unwrap();
    assert_eq!(authorization.operation, CapabilityOperation::StateAdmin);
    assert!(authorization.project_id.is_none());
    assert!(authorization.workspace_id.is_none());
    request.operation = "migration.stage".to_string();
    assert_eq!(
        validate_request(&request, &nonce),
        Err(AdministrativeProtocolError::Envelope)
    );
}

#[cfg(unix)]
#[test]
fn installation_admin_grants_and_revokes_pre_cutover_ci_authority_over_ada1() {
    if !enter_isolated_socket_test(
        "capability-management",
        "administrative_protocol::administrative_protocol_tests::installation_admin_grants_and_revokes_pre_cutover_ci_authority_over_ada1",
    ) {
        return;
    }
    let authority = VerifiedServiceAuthority::for_current_test_process().unwrap();
    let clock = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let now = clock.as_secs() as i64;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/admin-protocol-tests/capability-management-{}-{}",
        std::process::id(),
        clock.as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let admin_secret = [31_u8; 32];
    let admin_id = "cap_aaaaaaaaaaaaaaaaaaaaaaaaaa";
    storage
        .registry()
        .store_capability_grant(&NewCapabilityGrant {
            grant_id: admin_id.to_string(),
            secret_hash: sha256(&admin_secret),
            subject_uid: authority.effective_uid(),
            actor: CapabilityActor::Operator,
            channel: CapabilityChannel::Administrative,
            operation: CapabilityOperation::InstallationAdmin,
            project_id: None,
            workspace_id: None,
            task_id: None,
            not_before_unix: now - 60,
            expires_at_unix: now + 600,
            revocation_epoch: 0,
            granted_at: format!("unix:{}", now - 60),
        })
        .unwrap();

    let target_id = "cap_bbbbbbbbbbbbbbbbbbbbbbbbbb";
    let target_secret = [32_u8; 32];
    let grant_nonce = [33_u8; 32];
    let grant_authentication =
        ConnectionAuthentication::for_test(authority.effective_uid(), grant_nonce);
    let (mut grant_client, mut grant_server) = UnixStream::pair().unwrap();
    let grant_response_bytes = std::thread::scope(|scope| {
        scope.spawn(|| {
            answer_administrative_hello(&mut grant_server, &grant_nonce).unwrap();
            serve_administrative_request(
                &mut grant_server,
                &grant_authentication,
                &authority,
                &storage,
            )
            .unwrap();
        });
        let connection_nonce = negotiate_client(&mut grant_client).unwrap();
        write_frame(
            &mut grant_client,
            &AdministrativeRequest {
                message_type: "request".to_string(),
                wire_version: ADMIN_WIRE_VERSION.to_string(),
                request_id: "req_aaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                connection_nonce,
                operation: "capability.grant".to_string(),
                capability: WireCapability {
                    capability_id: admin_id.to_string(),
                    token: encode_base64url(&admin_secret),
                },
                body: AdministrativeBody::Grant(CapabilityGrantBody {
                    contract_version: "capability-grant-request-v1".to_string(),
                    capability_id: target_id.to_string(),
                    token: encode_base64url(&target_secret),
                    subject_uid: authority.effective_uid(),
                    actor: "ci".to_string(),
                    channel: "administrative".to_string(),
                    capability_operation: "attestation.create".to_string(),
                    project_id: Some("prj_bbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
                    workspace_id: Some("wsp_bbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
                    task_id: None,
                    lifetime_seconds: 300,
                    revocation_epoch: 0,
                }),
            },
        )
        .unwrap();
        read_frame(&mut grant_client).unwrap()
    });
    assert!(
        !String::from_utf8_lossy(&grant_response_bytes).contains(&encode_base64url(&target_secret))
    );
    let grant_response: AdministrativeResponse =
        serde_json::from_slice(&grant_response_bytes).unwrap();
    let AdministrativeResult::Grant(grant_receipt) = grant_response.result.unwrap() else {
        panic!("grant receipt expected");
    };
    assert_eq!(grant_receipt.capability_id, target_id);
    assert_eq!(grant_receipt.state, "active");

    let target_nonce = [34_u8; 32];
    let target_request = CapabilityAuthorizationRequest {
        grant_id: target_id.to_string(),
        presented_secret: target_secret,
        operation: CapabilityOperation::AttestationCreate,
        project_id: Some("prj_bbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
        workspace_id: Some("wsp_bbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
        task_id: None,
        presented_connection_nonce: target_nonce,
        request_id: "req_bbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        request_fingerprint: [35_u8; 32],
    };
    let authorization_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(matches!(
        storage.registry().authorize_capability(
            authority.effective_uid(),
            CapabilityChannel::Administrative,
            &target_nonce,
            authorization_now,
            &target_request,
        ),
        Ok(CapabilityAuthorizationOutcome::Fresh(_))
    ));

    let revoke_nonce = [36_u8; 32];
    let revoke_authentication =
        ConnectionAuthentication::for_test(authority.effective_uid(), revoke_nonce);
    let (mut revoke_client, mut revoke_server) = UnixStream::pair().unwrap();
    let revoke_response_bytes = std::thread::scope(|scope| {
        scope.spawn(|| {
            answer_administrative_hello(&mut revoke_server, &revoke_nonce).unwrap();
            serve_administrative_request(
                &mut revoke_server,
                &revoke_authentication,
                &authority,
                &storage,
            )
            .unwrap();
        });
        let connection_nonce = negotiate_client(&mut revoke_client).unwrap();
        write_frame(
            &mut revoke_client,
            &AdministrativeRequest {
                message_type: "request".to_string(),
                wire_version: ADMIN_WIRE_VERSION.to_string(),
                request_id: "req_cccccccccccccccccccccccccc".to_string(),
                connection_nonce,
                operation: "capability.revoke".to_string(),
                capability: WireCapability {
                    capability_id: admin_id.to_string(),
                    token: encode_base64url(&admin_secret),
                },
                body: AdministrativeBody::Revoke(CapabilityRevokeBody {
                    contract_version: "capability-revoke-request-v1".to_string(),
                    capability_id: target_id.to_string(),
                    revocation_epoch: 0,
                }),
            },
        )
        .unwrap();
        read_frame(&mut revoke_client).unwrap()
    });
    let revoke_response: AdministrativeResponse =
        serde_json::from_slice(&revoke_response_bytes).unwrap();
    let AdministrativeResult::Revoke(revoke_receipt) = revoke_response.result.unwrap() else {
        panic!("revoke receipt expected");
    };
    assert_eq!(revoke_receipt.capability_id, target_id);
    assert_eq!(revoke_receipt.state, "revoked");
    assert_eq!(
        storage
            .registry()
            .capability_management_event_count()
            .unwrap(),
        2
    );

    let mut after_revoke = target_request;
    after_revoke.request_id = "req_dddddddddddddddddddddddddd".to_string();
    after_revoke.request_fingerprint = [37_u8; 32];
    assert_eq!(
        storage.registry().authorize_capability(
            authority.effective_uid(),
            CapabilityChannel::Administrative,
            &target_nonce,
            now + 2,
            &after_revoke,
        ),
        Ok(CapabilityAuthorizationOutcome::Denied)
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn capability_descriptor_writer_requires_a_private_empty_regular_file() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/admin-protocol-tests/capability-descriptor-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let bundle = AdministrativeCapabilityBundle {
        contract_version: "admin-capability-v1".to_string(),
        capability: WireCapability {
            capability_id: "cap_cccccccccccccccccccccccccc".to_string(),
            token: encode_base64url(&[41_u8; 32]),
        },
    };
    let private_path = root.join("private.json");
    let private = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&private_path)
        .unwrap();
    fs::set_permissions(&private_path, fs::Permissions::from_mode(0o600)).unwrap();
    write_capability_bundle_to_fd(private.into_raw_fd().to_string().into(), &bundle).unwrap();
    let persisted: AdministrativeCapabilityBundle =
        serde_json::from_slice(&fs::read(&private_path).unwrap()).unwrap();
    assert_eq!(
        persisted.capability.capability_id,
        bundle.capability.capability_id
    );

    let public_path = root.join("public.json");
    let public = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&public_path)
        .unwrap();
    fs::set_permissions(&public_path, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        write_capability_bundle_to_fd(public.into_raw_fd().to_string().into(), &bundle),
        Err(AdministrativeProtocolError::Authorization)
    );
    assert_eq!(fs::metadata(&public_path).unwrap().len(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn authorized_admin_request_stages_bytes_through_the_socket_service() {
    if !enter_isolated_socket_test(
        "stage",
        "administrative_protocol::administrative_protocol_tests::authorized_admin_request_stages_bytes_through_the_socket_service",
    ) {
        return;
    }
    let authority = VerifiedServiceAuthority::for_current_test_process().unwrap();
    let clock = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/admin-protocol-tests/socket-stage-{}-{}",
        std::process::id(),
        clock.as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let secret = [7_u8; 32];
    let now = clock.as_secs() as i64;
    storage
        .registry()
        .store_capability_grant(&NewCapabilityGrant {
            grant_id: "cap_00000000000000000000000000".to_string(),
            secret_hash: sha256(&secret),
            subject_uid: authority.effective_uid(),
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
        .unwrap();

    let source = br#"{"entries":[]}"#.to_vec();
    let payload = serde_json::json!({
        "source_kind": "legacy-plugin",
        "source_locator": "operator-private",
        "root": {"type": "directory", "mode": "700", "uid": authority.effective_uid(), "gid": authority.effective_uid()},
        "validation": {"json_files": 1, "valid_json_files": 1, "invalid_json_files": 0},
        "counts": {"entries": 1, "regular_files": 1, "directories": 0, "symlinks": 0, "other": 0},
        "mode_histogram": [{"mode": "600", "count": 1}],
        "ownership_histogram": [{"uid": authority.effective_uid(), "gid": authority.effective_uid(), "count": 1}],
        "semantic_counts": {
            "plans": 0, "tasks": 0, "submissions": 0, "reviews": 0,
            "history_events": 0, "problems": 0, "transitions": 0,
            "json_schemas": 0, "generated_artifacts": 0, "task_evidence": 0
        },
        "git": {"repository": false, "head": null, "branch": null, "worktree_clean": null, "status": []},
        "entries": [{
            "path": "governance_history.json", "type": "regular", "mode": "600",
            "uid": authority.effective_uid(), "gid": authority.effective_uid(),
            "size": source.len(), "sha256": lowercase_hex(&sha256(&source)),
            "link": {"kind": "none", "target": null, "target_sha256": null}
        }]
    });
    let mut canonical_payload = serde_json::to_vec(&payload).unwrap();
    canonical_payload.push(b'\n');
    let digest = lowercase_hex(&sha256(&canonical_payload));
    let inventory = serde_json::json!({
        "contract_version": "legacy-source-inventory-v1",
        "captured_at": "2026-08-20T00:00:00Z",
        "inventory_sha256": digest,
        "consistency": "two-identical-read-only-passes",
        "payload": payload
    });

    let connection_nonce = [9_u8; 32];
    let authentication =
        ConnectionAuthentication::for_test(authority.effective_uid(), connection_nonce);
    let (mut client, mut server) = UnixStream::pair().unwrap();
    let normalization_nonce = [10_u8; 32];
    let normalization_authentication =
        ConnectionAuthentication::for_test(authority.effective_uid(), normalization_nonce);
    let (mut normalization_client, mut normalization_server) = UnixStream::pair().unwrap();
    let dry_run_nonce = [11_u8; 32];
    let dry_run_authentication =
        ConnectionAuthentication::for_test(authority.effective_uid(), dry_run_nonce);
    let (mut dry_run_client, mut dry_run_server) = UnixStream::pair().unwrap();
    let server_thread = std::thread::spawn(move || {
        answer_administrative_hello(&mut server, &connection_nonce).unwrap();
        serve_administrative_request(&mut server, &authentication, &authority, &storage).unwrap();
        answer_administrative_hello(&mut normalization_server, &normalization_nonce).unwrap();
        serve_administrative_request(
            &mut normalization_server,
            &normalization_authentication,
            &authority,
            &storage,
        )
        .unwrap();
        answer_administrative_hello(&mut dry_run_server, &dry_run_nonce).unwrap();
        serve_administrative_request(
            &mut dry_run_server,
            &dry_run_authentication,
            &authority,
            &storage,
        )
        .unwrap();
    });

    let negotiated = negotiate_client(&mut client).unwrap();
    let request_id = "req_00000000000000000000000000".to_string();
    write_frame(
        &mut client,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request_id.clone(),
            connection_nonce: negotiated,
            operation: "migration.stage".to_string(),
            capability: WireCapability {
                capability_id: "cap_00000000000000000000000000".to_string(),
                token: encode_base64url(&secret),
            },
            body: AdministrativeBody::Stage(MigrationStageBody {
                expected_source_snapshot_sha256: digest.clone(),
                inventory,
            }),
        },
    )
    .unwrap();
    write_all_before(&mut client, &source, Instant::now() + ADMIN_TIMEOUT).unwrap();
    let response: AdministrativeResponse =
        serde_json::from_slice(&read_frame(&mut client).unwrap()).unwrap();
    assert_eq!(response.request_id, request_id);
    assert_eq!(response.status, "ok");
    let AdministrativeResult::Stage(receipt) = response.result.unwrap() else {
        panic!("stage receipt expected");
    };
    assert_eq!(receipt.source_snapshot_sha256, digest);
    assert_eq!(receipt.regular_files, 1);
    assert_eq!(receipt.staged_bytes, source.len() as u64);
    assert!(root
        .join("migration-staging")
        .join(&receipt.migration_id)
        .join("source/governance_history.json")
        .is_file());

    let content_sha256 = lowercase_hex(&sha256(&source));
    #[derive(Serialize)]
    struct TestSnapshotEntry<'a> {
        path: &'a str,
        r#type: &'a str,
        content_sha256: String,
    }
    let snapshot = [TestSnapshotEntry {
        path: "governance_history.json",
        r#type: "regular",
        content_sha256,
    }];
    let snapshot_sha256 = lowercase_hex(&sha256(&serde_json::to_vec(&snapshot).unwrap()));
    let negotiated = negotiate_client(&mut normalization_client).unwrap();
    let normalization_request_id = "req_11111111111111111111111111".to_string();
    write_frame(
        &mut normalization_client,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: normalization_request_id.clone(),
            connection_nonce: negotiated,
            operation: "migration.normalize".to_string(),
            capability: WireCapability {
                capability_id: "cap_00000000000000000000000000".to_string(),
                token: encode_base64url(&secret),
            },
            body: AdministrativeBody::Normalize(LegacyNormalizationRequest {
                contract_version: "legacy-normalization-request-v1".to_string(),
                migration_id: receipt.migration_id.clone(),
                expected_source_snapshot_sha256: digest,
                source_normalized_snapshot_sha256: snapshot_sha256.clone(),
                source_semantic_snapshot_sha256: snapshot_sha256,
                physical_references: Vec::new(),
            }),
        },
    )
    .unwrap();
    let response: AdministrativeResponse =
        serde_json::from_slice(&read_frame(&mut normalization_client).unwrap()).unwrap();
    assert_eq!(response.request_id, normalization_request_id);
    let AdministrativeResult::Normalize(normalization) = response.result.unwrap() else {
        panic!("normalization receipt expected");
    };
    assert_eq!(normalization.comparison, "equivalent");
    assert_eq!(normalization.state, "normalized");

    let negotiated = negotiate_client(&mut dry_run_client).unwrap();
    let dry_run_request_id = "req_22222222222222222222222222".to_string();
    write_frame(
        &mut dry_run_client,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: dry_run_request_id.clone(),
            connection_nonce: negotiated,
            operation: "migration.dry-run".to_string(),
            capability: WireCapability {
                capability_id: "cap_00000000000000000000000000".to_string(),
                token: encode_base64url(&secret),
            },
            body: AdministrativeBody::DryRun(crate::legacy_dry_run::dry_run_request(
                receipt.migration_id.clone(),
                normalization.source_snapshot_sha256.clone(),
                normalization.staging_snapshot_sha256.clone(),
                normalization.normalized_snapshot_sha256.clone(),
                normalization.semantic_snapshot_sha256.clone(),
            )),
        },
    )
    .unwrap();
    let response: AdministrativeResponse =
        serde_json::from_slice(&read_frame(&mut dry_run_client).unwrap()).unwrap();
    assert_eq!(response.request_id, dry_run_request_id);
    let AdministrativeResult::DryRun(report) = response.result.unwrap() else {
        panic!("dry-run report expected");
    };
    assert_eq!(report.cutover_readiness, "blocked");
    assert!(!report.approval_created);
    assert!(!report.cutover_performed);
    assert!(root
        .join("migration-staging")
        .join(&receipt.migration_id)
        .join("normalized/source/governance_history.json")
        .is_file());
    assert!(root
        .join("migration-staging")
        .join(receipt.migration_id)
        .join("dry-run/legacy-dry-run-report-v1.json")
        .is_file());
    server_thread.join().unwrap();
}
