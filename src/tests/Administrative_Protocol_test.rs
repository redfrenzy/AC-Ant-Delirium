use super::*;

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::fs::OpenOptions;
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
#[test]
fn identity_association_uses_scoped_operator_authority_and_private_descriptors() {
    if !enter_isolated_socket_test("identity-association",
        "administrative_protocol::administrative_protocol_tests::identity_association_uses_scoped_operator_authority_and_private_descriptors") { return; }
    let f = crate::migration_identity::tests::fixture("ada1");
    let uid = f.authority.effective_uid();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let secret = [53; 32];
    let grants = [
        (
            "cap_00000000000000000000000001",
            CapabilityActor::Operator,
            CapabilityOperation::StateAdmin,
            None,
            None,
        ),
        (
            "cap_00000000000000000000000002",
            CapabilityActor::Operator,
            CapabilityOperation::StateAdmin,
            Some(f.request.provisional_project_id.clone()),
            Some(f.request.provisional_workspace_id.clone()),
        ),
        (
            "cap_00000000000000000000000003",
            CapabilityActor::Ci,
            CapabilityOperation::InstallationAdmin,
            Some(f.request.candidate_project_id.clone()),
            Some(f.request.candidate_workspace_id.clone()),
        ),
        (
            "cap_00000000000000000000000004",
            CapabilityActor::Operator,
            CapabilityOperation::StateAdmin,
            Some(f.request.candidate_project_id.clone()),
            Some(f.request.candidate_workspace_id.clone()),
        ),
    ];
    for (id, actor, operation, project_id, workspace_id) in &grants {
        f.storage
            .registry()
            .store_capability_grant(&NewCapabilityGrant {
                grant_id: id.to_string(),
                secret_hash: sha256(&secret),
                subject_uid: uid,
                actor: *actor,
                channel: CapabilityChannel::Administrative,
                operation: *operation,
                project_id: project_id.clone(),
                workspace_id: workspace_id.clone(),
                task_id: None,
                not_before_unix: now - 60,
                expires_at_unix: now + 600,
                revocation_epoch: 0,
                granted_at: format!("unix:{now}"),
            })
            .unwrap();
    }
    let wire = |id: &str, request_id: &str| AdministrativeRequest {
        message_type: "request".to_string(),
        wire_version: ADMIN_WIRE_VERSION.to_string(),
        request_id: request_id.to_string(),
        connection_nonce: String::new(),
        operation: "migration.identity-associate".to_string(),
        capability: WireCapability {
            capability_id: id.to_string(),
            token: encode_base64url(&secret),
        },
        body: AdministrativeBody::IdentityAssociate(f.request.clone()),
    };
    for (index, grant) in grants[..3].iter().enumerate() {
        assert!(try_exchange_test_request(
            &f.authority,
            &f.storage,
            uid,
            [54 + index as u8; 32],
            wire(grant.0, &format!("req_{:026}", index + 1))
        )
        .is_err());
    }
    assert!(try_exchange_test_request(
        &f.authority,
        &f.storage,
        uid + 1,
        [57; 32],
        wire(grants[3].0, "req_00000000000000000000000004")
    )
    .is_err());

    let socket = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/t116-{}-{}.sock",
        std::process::id(),
        now
    ));
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    let input = f.root.join("association.json");
    let capability = f.root.join("capability.json");
    let output = f.root.join("receipt.json");
    fs::write(&input, serde_json::to_vec(&f.request).unwrap()).unwrap();
    fs::write(
        &capability,
        serde_json::to_vec(&AdministrativeCapabilityBundle {
            contract_version: "admin-capability-v1".to_string(),
            capability: WireCapability {
                capability_id: grants[3].0.to_string(),
                token: encode_base64url(&secret),
            },
        })
        .unwrap(),
    )
    .unwrap();
    fs::set_permissions(&input, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&capability, fs::Permissions::from_mode(0o600)).unwrap();
    use std::os::unix::fs::OpenOptionsExt;
    let writer = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&output)
        .unwrap();
    std::env::set_var(ADMIN_SOCKET_ENV, &socket);
    std::env::set_var(
        IDENTITY_ASSOCIATION_INPUT_FD_ENV,
        fs::File::open(&input).unwrap().into_raw_fd().to_string(),
    );
    std::env::set_var(
        IDENTITY_ASSOCIATION_OUTPUT_FD_ENV,
        writer.into_raw_fd().to_string(),
    );
    std::env::set_var(
        ADMIN_CAPABILITY_FD_ENV,
        fs::File::open(&capability)
            .unwrap()
            .into_raw_fd()
            .to_string(),
    );
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut server = accept_identity_test_connection(&listener);
            let nonce = [58; 32];
            answer_administrative_hello(&mut server, &nonce).unwrap();
            serve_administrative_request(
                &mut server,
                &ConnectionAuthentication::for_test(uid, nonce),
                &f.authority,
                &f.storage,
            )
            .unwrap();
        });
        run_official_task_package_operator_inner("identity-associate").unwrap()
    });
    let AdministrativeResult::IdentityAssociation(receipt) = result else {
        panic!("association receipt expected")
    };
    assert_eq!(
        receipt,
        crate::migration_identity::association_receipt(&f.request).unwrap()
    );
    let stored: MigrationIdentityAssociationReceipt =
        serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(stored, receipt);
    for nonce in [[59; 32], [60; 32]] {
        let response = exchange_test_request(
            &f.authority,
            &f.storage,
            uid,
            nonce,
            wire(grants[3].0, "req_00000000000000000000000005"),
        );
        let AdministrativeResult::IdentityAssociation(replayed) = response.result.unwrap() else {
            panic!("replayed receipt expected")
        };
        assert_eq!(replayed, receipt);
    }
    let mut substituted = receipt;
    substituted.workspace_id = "wsp_00000000000000000000000009".to_string();
    assert!(validate_official_task_package_result(
        &AdministrativeBody::IdentityAssociate(f.request.clone()),
        &AdministrativeResult::IdentityAssociation(substituted)
    )
    .is_err());

    let rollback = crate::migration_identity::tests::recovery::rollback_request(&f);
    let rollback_wire = |id: &str| AdministrativeRequest {
        operation: "migration.identity-rollback".to_string(),
        body: AdministrativeBody::IdentityRollback(rollback.clone()),
        ..wire(id, "req_00000000000000000000000006")
    };
    for grant in &grants[..3] {
        assert!(try_exchange_test_request(
            &f.authority,
            &f.storage,
            uid,
            [61; 32],
            rollback_wire(grant.0)
        )
        .is_err());
    }
    assert!(try_exchange_test_request(
        &f.authority,
        &f.storage,
        uid + 1,
        [62; 32],
        rollback_wire(grants[3].0)
    )
    .is_err());
    let rollback_input = f.root.join("rollback.json");
    let rollback_output = f.root.join("rollback-receipt.json");
    fs::write(&rollback_input, serde_json::to_vec(&rollback).unwrap()).unwrap();
    fs::set_permissions(&rollback_input, fs::Permissions::from_mode(0o600)).unwrap();
    let writer = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&rollback_output)
        .unwrap();
    std::env::set_var(ADMIN_SOCKET_ENV, &socket);
    std::env::set_var(
        IDENTITY_ASSOCIATION_INPUT_FD_ENV,
        fs::File::open(&rollback_input)
            .unwrap()
            .into_raw_fd()
            .to_string(),
    );
    std::env::set_var(
        IDENTITY_ASSOCIATION_OUTPUT_FD_ENV,
        writer.into_raw_fd().to_string(),
    );
    std::env::set_var(
        ADMIN_CAPABILITY_FD_ENV,
        fs::File::open(&capability)
            .unwrap()
            .into_raw_fd()
            .to_string(),
    );
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut server = accept_identity_test_connection(&listener);
            let nonce = [63; 32];
            answer_administrative_hello(&mut server, &nonce).unwrap();
            serve_administrative_request(
                &mut server,
                &ConnectionAuthentication::for_test(uid, nonce),
                &f.authority,
                &f.storage,
            )
            .unwrap();
        });
        run_official_task_package_operator_inner("identity-rollback").unwrap()
    });
    let AdministrativeResult::IdentityRollback(receipt) = result else {
        panic!("rollback receipt expected")
    };
    assert_eq!(
        receipt,
        crate::migration_identity_recovery::rollback_receipt(&rollback).unwrap()
    );
    let stored: MigrationIdentityRollbackReceipt =
        serde_json::from_slice(&fs::read(&rollback_output).unwrap()).unwrap();
    assert_eq!(stored, receipt);
    assert!(
        try_exchange_test_request(
            &f.authority,
            &f.storage,
            uid,
            [64; 32],
            rollback_wire(grants[3].0)
        )
        .is_err(),
        "rollback must revoke even the consumed operator grant"
    );
    let fresh_id = "cap_00000000000000000000000005";
    f.storage
        .registry()
        .store_capability_grant(&NewCapabilityGrant {
            grant_id: fresh_id.to_string(),
            secret_hash: sha256(&secret),
            subject_uid: uid,
            actor: CapabilityActor::Operator,
            channel: CapabilityChannel::Administrative,
            operation: CapabilityOperation::StateAdmin,
            project_id: Some(f.request.candidate_project_id.clone()),
            workspace_id: Some(f.request.candidate_workspace_id.clone()),
            task_id: None,
            not_before_unix: now - 60,
            expires_at_unix: now + 600,
            revocation_epoch: 0,
            granted_at: format!("unix:{now}"),
        })
        .unwrap();
    for nonce in [[65; 32], [66; 32]] {
        let response = exchange_test_request(
            &f.authority,
            &f.storage,
            uid,
            nonce,
            rollback_wire(fresh_id),
        );
        let AdministrativeResult::IdentityRollback(replayed) = response.result.unwrap() else {
            panic!("replayed rollback expected")
        };
        assert_eq!(replayed, receipt);
    }
    let mut substituted = receipt;
    substituted.workspace_id = "wsp_00000000000000000000000009".to_string();
    assert!(validate_official_task_package_result(
        &AdministrativeBody::IdentityRollback(rollback),
        &AdministrativeResult::IdentityRollback(substituted)
    )
    .is_err());
}

#[cfg(unix)]
fn accept_identity_test_connection(listener: &std::os::unix::net::UnixListener) -> UnixStream {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => panic!("identity test connection failed: {error}"),
        }
    }
}

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
fn exchange_test_request(
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
    peer_uid: u32,
    nonce: [u8; 32],
    request: AdministrativeRequest,
) -> AdministrativeResponse {
    try_exchange_test_request(authority, storage, peer_uid, nonce, request).unwrap()
}

#[cfg(unix)]
fn try_exchange_test_request(
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
    peer_uid: u32,
    nonce: [u8; 32],
    mut request: AdministrativeRequest,
) -> Result<AdministrativeResponse, AdministrativeProtocolError> {
    let authentication = ConnectionAuthentication::for_test(peer_uid, nonce);
    let (mut client, mut server) = UnixStream::pair().unwrap();
    let response_bytes = std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            answer_administrative_hello(&mut server, &nonce).unwrap();
            serve_administrative_request(&mut server, &authentication, authority, storage)
        });
        request.connection_nonce = negotiate_client(&mut client).unwrap();
        write_frame(&mut client, &request).unwrap();
        let response = read_frame(&mut client);
        worker.join().unwrap()?;
        response
    })?;
    assert!(!String::from_utf8_lossy(&response_bytes).contains("/home/"));
    Ok(serde_json::from_slice(&response_bytes).unwrap())
}

#[cfg(unix)]
#[test]
fn migration_source_inputs_require_readonly_inherited_descriptors() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/administrative-descriptor-tests/{}-{nonce}",
        std::process::id()
    ));
    let source = root.join("operator-private-source");
    let inventory_path = root.join("inventory.json");
    fs::create_dir_all(&source).unwrap();
    fs::write(&inventory_path, b"{\"contract\":\"fixture\"}\n").unwrap();

    let source_fd = fs::File::open(&source).unwrap().into_raw_fd();
    let inventory_fd = fs::File::open(&inventory_path).unwrap().into_raw_fd();
    let (directory, inventory) = read_migration_source_inputs(
        OsString::from(source_fd.to_string()),
        OsString::from(inventory_fd.to_string()),
    )
    .unwrap();
    assert!(directory.metadata().unwrap().is_dir());
    assert_eq!(inventory, b"{\"contract\":\"fixture\"}\n");

    let source_fd = fs::File::open(&source).unwrap().into_raw_fd();
    let inventory_fd = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&inventory_path)
        .unwrap()
        .into_raw_fd();
    assert!(matches!(
        read_migration_source_inputs(
            OsString::from(source_fd.to_string()),
            OsString::from(inventory_fd.to_string()),
        ),
        Err(AdministrativeProtocolError::Authorization)
    ));
}

#[cfg(unix)]
#[test]
fn task_package_input_requires_a_bounded_readonly_descriptor_at_offset_zero() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/administrative-descriptor-tests/task-package-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let package_path = root.join("package.json");
    let package = br#"{"contract_version":"governance-task-package-v1"}"#;
    fs::write(&package_path, package).unwrap();

    let readonly_fd = fs::File::open(&package_path).unwrap().into_raw_fd();
    assert_eq!(
        read_bounded_json_from_raw_fd(OsString::from(readonly_fd.to_string())).unwrap(),
        package
    );

    let writable_fd = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&package_path)
        .unwrap()
        .into_raw_fd();
    assert_eq!(
        read_bounded_json_from_raw_fd(OsString::from(writable_fd.to_string())),
        Err(AdministrativeProtocolError::Authorization)
    );

    let mut advanced = fs::File::open(&package_path).unwrap();
    let mut first = [0_u8; 1];
    advanced.read_exact(&mut first).unwrap();
    assert_eq!(
        read_bounded_json_from_raw_fd(OsString::from(advanced.into_raw_fd().to_string())),
        Err(AdministrativeProtocolError::Authorization)
    );
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

#[test]
fn official_task_package_grammar_binds_each_operation_to_exact_scope() {
    let nonce = [14_u8; 32];
    let project_id = "prj_11111111111111111111111111";
    let workspace_id = "wsp_11111111111111111111111111";
    let source = TaskGenerationSource {
        contract_version: crate::task_package_contracts::TASK_GENERATION_SOURCE_V1.to_string(),
        request_id: "req_11111111111111111111111111".to_string(),
        source_format: "ordered-objectives-v1".to_string(),
        source_sha256: "a".repeat(64),
        project_id: project_id.to_string(),
        workspace_id: workspace_id.to_string(),
        expected_state_revision: 0,
        plans: vec![crate::task_package_contracts::TaskGenerationSourcePlan {
            source_key_sha256: "b".repeat(64),
            objective: "Prepare one proposal".to_string(),
            tasks: vec![crate::task_package_contracts::TaskGenerationSourceTask {
                source_key_sha256: "c".repeat(64),
                objective: "Exercise the closed grammar".to_string(),
                acceptance_criteria: Vec::new(),
                relevant_paths: vec!["src/lib.rs".to_string()],
            }],
        }],
    };
    let proposal = crate::task_package_generator::generate_task_package_proposal(&source).unwrap();
    let mut request = AdministrativeRequest {
        message_type: "request".to_string(),
        wire_version: ADMIN_WIRE_VERSION.to_string(),
        request_id: "req_22222222222222222222222222".to_string(),
        connection_nonce: encode_base64url(&nonce),
        operation: "governance.task-package.prepare".to_string(),
        capability: WireCapability {
            capability_id: "cap_11111111111111111111111111".to_string(),
            token: encode_base64url(&[15_u8; 32]),
        },
        body: AdministrativeBody::TaskPackagePrepare(source),
    };
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    let authentication = ConnectionAuthentication::for_test(1000, nonce);
    let authorization = capability_request(&authentication, &request).unwrap();
    assert_eq!(authorization.operation, CapabilityOperation::StateAdmin);
    assert_eq!(authorization.project_id.as_deref(), Some(project_id));
    assert_eq!(authorization.workspace_id.as_deref(), Some(workspace_id));
    assert!(authorization.task_id.is_none());

    request.operation = "governance.task-package.inspect".to_string();
    request.body = AdministrativeBody::TaskPackageInspect(TaskPackageInspectBody {
        contract_version: "task-package-inspect-v1".to_string(),
        request_id: "req_33333333333333333333333333".to_string(),
        project_id: project_id.to_string(),
        workspace_id: workspace_id.to_string(),
        expected_state_revision: 0,
        proposal_id: proposal.proposal_id.clone(),
    });
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    assert_eq!(
        capability_request(&authentication, &request)
            .unwrap()
            .operation,
        CapabilityOperation::DiagnosticPrivate
    );

    let attestation = OfficialTaskPackageAttestation {
        contract_version: crate::task_package_contracts::TASK_PACKAGE_ATTESTATION_V1.to_string(),
        attestation_id: "att_11111111111111111111111111".to_string(),
        proposal_id: proposal.proposal_id.clone(),
        proposal_sha256: proposal.payload_sha256.clone(),
        source_sha256: proposal.source_sha256.clone(),
        project_id: project_id.to_string(),
        workspace_id: workspace_id.to_string(),
        expected_state_revision: 0,
        verifier_subject_sha256: "d".repeat(64),
        decision: "accepted".to_string(),
        attested_at: "unix:1".to_string(),
        state: "attested".to_string(),
    };
    request.operation = "governance.task-package.attest".to_string();
    request.body = AdministrativeBody::TaskPackageAttest(attestation.clone());
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    assert_eq!(
        capability_request(&authentication, &request)
            .unwrap()
            .operation,
        CapabilityOperation::AttestationCreate
    );

    request.operation = "governance.task-package.approve".to_string();
    request.body = AdministrativeBody::TaskPackageApprove(OfficialTaskPackageApproval {
        contract_version: crate::task_package_contracts::TASK_PACKAGE_APPROVAL_V1.to_string(),
        approval_id: "apr_11111111111111111111111111".to_string(),
        proposal_id: proposal.proposal_id,
        proposal_sha256: proposal.payload_sha256,
        attestation_id: attestation.attestation_id,
        attestation_sha256: "e".repeat(64),
        project_id: project_id.to_string(),
        workspace_id: workspace_id.to_string(),
        expected_state_revision: 0,
        operator_subject_sha256: "f".repeat(64),
        decision: "approved".to_string(),
        approved_at: "unix:2".to_string(),
        state: "approved".to_string(),
    });
    assert_eq!(validate_request(&request, &nonce), Ok(()));
    assert_eq!(
        capability_request(&authentication, &request)
            .unwrap()
            .operation,
        CapabilityOperation::StateAdmin
    );

    request.operation = "governance.task-package.prepare".to_string();
    assert_eq!(
        validate_request(&request, &nonce),
        Err(AdministrativeProtocolError::Envelope)
    );
}

#[test]
fn generic_task_package_request_is_closed_and_requires_state_admin() {
    let nonce = [5_u8; 32];
    let request = AdministrativeRequest {
        message_type: "request".to_string(),
        wire_version: ADMIN_WIRE_VERSION.to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        connection_nonce: encode_base64url(&nonce),
        operation: "governance.task-package.install".to_string(),
        capability: WireCapability {
            capability_id: "cap_00000000000000000000000000".to_string(),
            token: encode_base64url(&[7_u8; 32]),
        },
        body: AdministrativeBody::TaskPackage(TaskPackageBody {
            contract_version: "governance-task-package-v1".to_string(),
            project_id: "prj_00000000000000000000000000".to_string(),
            workspace_id: "wsp_00000000000000000000000000".to_string(),
            plan_id: "pln_00000000000000000000000000".to_string(),
            expected_state_revision: 0,
            tasks: vec![TaskPackageTaskBody {
                task_id: "tsk_00000000000000000000000000".to_string(),
                task_revision: 1,
                objective: "Implement the synthetic protocol task.".to_string(),
                acceptance_criteria: vec!["The protocol test passes.".to_string()],
                relevant_paths: vec!["src/lib.rs".to_string()],
            }],
        }),
    };

    assert_eq!(validate_request(&request, &nonce), Ok(()));
    let authentication = ConnectionAuthentication::for_test(1000, nonce);
    let authorization = capability_request(&authentication, &request).unwrap();
    assert_eq!(authorization.operation, CapabilityOperation::StateAdmin);
    assert_eq!(authorization.project_id, None);
    assert_eq!(authorization.workspace_id, None);

    let mut wrong_operation = request;
    wrong_operation.operation = "migration.convert".to_string();
    assert_eq!(
        validate_request(&wrong_operation, &nonce),
        Err(AdministrativeProtocolError::Envelope)
    );
}

#[test]
fn accepted_task_advance_is_closed_and_requires_exact_ci_scope() {
    let nonce = [6_u8; 32];
    let request = AdministrativeRequest {
        message_type: "request".to_string(),
        wire_version: ADMIN_WIRE_VERSION.to_string(),
        request_id: "req_00000000000000000000000000".to_string(),
        connection_nonce: encode_base64url(&nonce),
        operation: "governance.task.advance.accepted".to_string(),
        capability: WireCapability {
            capability_id: "cap_00000000000000000000000000".to_string(),
            token: encode_base64url(&[8_u8; 32]),
        },
        body: AdministrativeBody::TaskAdvance(TaskAdvanceBody {
            contract_version: "governance-task-advance-v1".to_string(),
            project_id: "prj_00000000000000000000000000".to_string(),
            workspace_id: "wsp_00000000000000000000000000".to_string(),
            task_id: "tsk_00000000000000000000000000".to_string(),
            review_id: "rev_00000000000000000000000000".to_string(),
            expected_state_revision: 1,
            protected_gate: serde_json::json!({}),
        }),
    };

    assert_eq!(validate_request(&request, &nonce), Ok(()));
    let authentication = ConnectionAuthentication::for_test(1000, nonce);
    let authorization = capability_request(&authentication, &request).unwrap();
    assert_eq!(
        authorization.operation,
        CapabilityOperation::StateTransition
    );
    assert_eq!(
        authorization.project_id.as_deref(),
        Some("prj_00000000000000000000000000")
    );
    assert_eq!(
        authorization.workspace_id.as_deref(),
        Some("wsp_00000000000000000000000000")
    );
    assert_eq!(
        authorization.task_id.as_deref(),
        Some("tsk_00000000000000000000000000")
    );
}

#[test]
fn issued_agent_submission_capability_has_a_non_administrative_contract() {
    assert_eq!(
        issued_capability_contract("agent", "limited", "submission.create"),
        "agent-submission-capability-v1"
    );
    assert_eq!(
        issued_capability_contract("reviewer", "limited", "submission.read.assigned"),
        "reviewer-assignment-capability-v1"
    );
    assert_eq!(
        issued_capability_contract("reviewer", "limited", "review.decide.assigned"),
        "reviewer-decision-capability-v1"
    );
    assert_eq!(
        issued_capability_contract("ci", "administrative", "state.transition"),
        "ci-state-transition-capability-v1"
    );
    assert_eq!(
        issued_capability_contract("wrapper", "limited", "projection.refresh"),
        "wrapper-projection-capability-v1"
    );
    assert_eq!(
        issued_capability_contract("operator", "administrative", "state.admin"),
        "admin-capability-v1"
    );
    assert_eq!(
        issued_capability_contract("operator", "administrative", "diagnostic.private"),
        "admin-capability-v1"
    );
    assert_eq!(
        issued_capability_contract("ci", "administrative", "attestation.create"),
        "admin-capability-v1"
    );
    assert_eq!(
        issued_capability_contract("ci", "administrative", "installation.admin"),
        "admin-capability-v1"
    );
    for (actor, channel, operation) in [
        ("agent", "administrative", "submission.create"),
        ("agent", "limited", "task.current.read"),
    ] {
        assert_eq!(
            issued_capability_contract(actor, channel, operation),
            "admin-capability-v1"
        );
    }
}

#[cfg(unix)]
#[test]
fn state_admin_installs_a_task_package_end_to_end_over_ada1() {
    if !enter_isolated_socket_test(
        "task-package",
        "administrative_protocol::administrative_protocol_tests::state_admin_installs_a_task_package_end_to_end_over_ada1",
    ) {
        return;
    }
    let authority = VerifiedServiceAuthority::for_current_test_process().unwrap();
    let clock = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let now = clock.as_secs() as i64;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/admin-protocol-tests/task-package-{}-{}",
        std::process::id(),
        clock.as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let registration = storage
        .bootstrap_provisional(
            &authority,
            authority.effective_uid(),
            &[61_u8; 32],
            &[62_u8; 32],
            "synthetic-ada1-project",
        )
        .unwrap();
    let capability_id = "cap_cccccccccccccccccccccccccc";
    let capability_secret = [62_u8; 32];
    storage
        .registry()
        .store_capability_grant(&NewCapabilityGrant {
            grant_id: capability_id.to_string(),
            secret_hash: sha256(&capability_secret),
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
            granted_at: format!("unix:{}", now - 60),
        })
        .unwrap();

    let connection_nonce = [63_u8; 32];
    let authentication =
        ConnectionAuthentication::for_test(authority.effective_uid(), connection_nonce);
    let (mut client, mut server) = UnixStream::pair().unwrap();
    let response_bytes = std::thread::scope(|scope| {
        scope.spawn(|| {
            answer_administrative_hello(&mut server, &connection_nonce).unwrap();
            serve_administrative_request(&mut server, &authentication, &authority, &storage)
                .unwrap();
        });
        let negotiated_nonce = negotiate_client(&mut client).unwrap();
        write_frame(
            &mut client,
            &AdministrativeRequest {
                message_type: "request".to_string(),
                wire_version: ADMIN_WIRE_VERSION.to_string(),
                request_id: "req_cccccccccccccccccccccccccc".to_string(),
                connection_nonce: negotiated_nonce,
                operation: "governance.task-package.install".to_string(),
                capability: WireCapability {
                    capability_id: capability_id.to_string(),
                    token: encode_base64url(&capability_secret),
                },
                body: AdministrativeBody::TaskPackage(TaskPackageBody {
                    contract_version: "governance-task-package-v1".to_string(),
                    project_id: registration.project_id.clone(),
                    workspace_id: registration.workspace_id.clone(),
                    plan_id: "pln_cccccccccccccccccccccccccc".to_string(),
                    expected_state_revision: 0,
                    tasks: vec![TaskPackageTaskBody {
                        task_id: "tsk_cccccccccccccccccccccccccc".to_string(),
                        task_revision: 1,
                        objective: "Implement the synthetic ADA1 task.".to_string(),
                        acceptance_criteria: vec!["The end-to-end test passes.".to_string()],
                        relevant_paths: vec!["src/lib.rs".to_string()],
                    }],
                }),
            },
        )
        .unwrap();
        read_frame(&mut client).unwrap()
    });

    let response: AdministrativeResponse = serde_json::from_slice(&response_bytes).unwrap();
    assert_eq!(response.status, "ok");
    let AdministrativeResult::TaskPackage(receipt) = response.result.unwrap() else {
        panic!("task-package receipt expected");
    };
    assert_eq!(receipt.project_id, registration.project_id);
    assert_eq!(receipt.workspace_id, registration.workspace_id);
    assert_eq!(receipt.current_task_id, "tsk_cccccccccccccccccccccccccc");
    assert_eq!(receipt.state_revision, 1);
    assert_eq!(receipt.task_count, 1);
    assert_eq!(receipt.state, "installed");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn official_task_package_cycle_installs_only_after_scoped_ci_authorization() {
    if !enter_isolated_socket_test(
        "official-task-package-chain",
        "administrative_protocol::administrative_protocol_tests::official_task_package_cycle_installs_only_after_scoped_ci_authorization",
    ) {
        return;
    }
    let authority = VerifiedServiceAuthority::for_current_test_process().unwrap();
    let clock = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let now = clock.as_secs() as i64;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/t114-{}-{}",
        std::process::id(),
        clock.as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let storage = ProtectedServiceStorage::initialize(&authority, &root).unwrap();
    let registration = storage
        .bootstrap_provisional(
            &authority,
            authority.effective_uid(),
            &[71_u8; 32],
            &[72_u8; 32],
            "synthetic-official-task-package",
        )
        .unwrap();

    let capabilities = [
        (
            "cap_11111111111111111111111111",
            [73_u8; 32],
            CapabilityActor::Operator,
            CapabilityOperation::StateAdmin,
        ),
        (
            "cap_22222222222222222222222222",
            [74_u8; 32],
            CapabilityActor::Operator,
            CapabilityOperation::DiagnosticPrivate,
        ),
        (
            "cap_33333333333333333333333333",
            [75_u8; 32],
            CapabilityActor::Ci,
            CapabilityOperation::AttestationCreate,
        ),
    ];
    let operator_uid = authority.effective_uid();
    let verifier_uid = operator_uid + 1;
    for (capability_id, secret, actor, operation) in &capabilities {
        storage
            .registry()
            .store_capability_grant(&NewCapabilityGrant {
                grant_id: (*capability_id).to_string(),
                secret_hash: sha256(secret),
                subject_uid: if *actor == CapabilityActor::Ci {
                    verifier_uid
                } else {
                    operator_uid
                },
                actor: *actor,
                channel: CapabilityChannel::Administrative,
                operation: *operation,
                project_id: Some(registration.project_id.clone()),
                workspace_id: Some(registration.workspace_id.clone()),
                task_id: None,
                not_before_unix: now - 60,
                expires_at_unix: now + 600,
                revocation_epoch: 0,
                granted_at: format!("unix:{}", now - 60),
            })
            .unwrap();
    }

    let source = TaskGenerationSource {
        contract_version: crate::task_package_contracts::TASK_GENERATION_SOURCE_V1.to_string(),
        request_id: "req_11111111111111111111111111".to_string(),
        source_format: "ordered-objectives-v1".to_string(),
        source_sha256: "a".repeat(64),
        project_id: registration.project_id.clone(),
        workspace_id: registration.workspace_id.clone(),
        expected_state_revision: 0,
        plans: vec![crate::task_package_contracts::TaskGenerationSourcePlan {
            source_key_sha256: "b".repeat(64),
            objective: "Prepare an official package".to_string(),
            tasks: vec![crate::task_package_contracts::TaskGenerationSourceTask {
                source_key_sha256: "c".repeat(64),
                objective: "Keep the canonical state unchanged".to_string(),
                acceptance_criteria: vec!["All decisions are persisted separately".to_string()],
                relevant_paths: vec!["src/lib.rs".to_string()],
            }],
        }],
    };
    let request = |request_id: &str,
                   operation: &str,
                   capability_id: &str,
                   secret: &[u8; 32],
                   body: AdministrativeBody| AdministrativeRequest {
        message_type: "request".to_string(),
        wire_version: ADMIN_WIRE_VERSION.to_string(),
        request_id: request_id.to_string(),
        connection_nonce: String::new(),
        operation: operation.to_string(),
        capability: WireCapability {
            capability_id: capability_id.to_string(),
            token: encode_base64url(secret),
        },
        body,
    };

    let prepared = exchange_test_request(
        &authority,
        &storage,
        operator_uid,
        [76_u8; 32],
        request(
            "req_22222222222222222222222222",
            "governance.task-package.prepare",
            capabilities[0].0,
            &capabilities[0].1,
            AdministrativeBody::TaskPackagePrepare(source.clone()),
        ),
    );
    let AdministrativeResult::TaskPackageArtifact(prepared) = prepared.result.unwrap() else {
        panic!("prepared proposal expected");
    };
    let proposal = prepared.proposal.clone().unwrap();
    assert_eq!(prepared.receipt.state, "prepared");
    assert_eq!(prepared.receipt.resulting_state_revision, 0);

    // Exercise the real client FD path, including rejection before transport.
    let endpoint = root.join("admin.sock");
    let input_path = root.join("input.json");
    let capability_path = root.join("capability.json");
    let output_path = root.join("output.json");
    fs::write(&input_path, serde_json::to_vec(&source).unwrap()).unwrap();
    fs::write(
        &capability_path,
        serde_json::to_vec(&AdministrativeCapabilityBundle {
            contract_version: "admin-capability-v1".to_string(),
            capability: WireCapability {
                capability_id: capabilities[0].0.to_string(),
                token: encode_base64url(&capabilities[0].1),
            },
        })
        .unwrap(),
    )
    .unwrap();
    fs::set_permissions(&capability_path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&output_path, b"occupied").unwrap();
    fs::set_permissions(&output_path, fs::Permissions::from_mode(0o600)).unwrap();
    let input = fs::File::open(&input_path).unwrap();
    let credential = fs::File::open(&capability_path).unwrap();
    let output = OpenOptions::new().write(true).open(&output_path).unwrap();
    std::env::set_var(ADMIN_SOCKET_ENV, &endpoint);
    std::env::set_var(ADMIN_CAPABILITY_FD_ENV, credential.as_raw_fd().to_string());
    std::env::set_var(TASK_PACKAGE_INPUT_FD_ENV, input.as_raw_fd().to_string());
    std::env::set_var(TASK_PACKAGE_OUTPUT_FD_ENV, output.into_raw_fd().to_string());
    assert!(matches!(
        run_official_task_package_operator_inner("prepare"),
        Err(AdministrativeProtocolError::Authorization)
    ));
    assert_eq!(fs::read(&output_path).unwrap(), b"occupied");
    assert!(!endpoint.exists());
    // Fresh private output, with no argv payload and no input FD leaked to children.
    let result_path = root.join("proposal.json");
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&result_path)
        .unwrap();
    fs::set_permissions(&result_path, fs::Permissions::from_mode(0o600)).unwrap();
    std::env::set_var(ADMIN_SOCKET_ENV, &endpoint);
    std::env::set_var(
        ADMIN_CAPABILITY_FD_ENV,
        credential.into_raw_fd().to_string(),
    );
    std::env::set_var(TASK_PACKAGE_INPUT_FD_ENV, input.into_raw_fd().to_string());
    std::env::set_var(TASK_PACKAGE_OUTPUT_FD_ENV, output.into_raw_fd().to_string());
    let listener = std::os::unix::net::UnixListener::bind(&endpoint).unwrap();
    let client_result = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let (mut stream, _) = listener.accept().unwrap();
            let nonce = [84_u8; 32];
            answer_administrative_hello(&mut stream, &nonce).unwrap();
            serve_administrative_request(
                &mut stream,
                &ConnectionAuthentication::for_test(operator_uid, nonce),
                &authority,
                &storage,
            )
            .unwrap();
        });
        let result = run_official_task_package_operator_inner("prepare").unwrap();
        worker.join().unwrap();
        result
    });
    assert_eq!(
        serde_json::to_value(&client_result).unwrap(),
        serde_json::from_slice::<Value>(&fs::read(&result_path).unwrap()).unwrap()
    );
    assert!(std::env::var_os(TASK_PACKAGE_INPUT_FD_ENV).is_none());
    assert!(std::env::var_os(TASK_PACKAGE_OUTPUT_FD_ENV).is_none());
    assert!(std::env::var_os(ADMIN_CAPABILITY_FD_ENV).is_none());

    let mut wrong_response = client_result;
    if let AdministrativeResult::TaskPackageArtifact(artifact) = &mut wrong_response {
        artifact.receipt.workspace_id = "wsp_99999999999999999999999999".to_string();
    }
    assert_eq!(
        validate_official_task_package_result(
            &AdministrativeBody::TaskPackagePrepare(source.clone()),
            &wrong_response
        ),
        Err(AdministrativeProtocolError::Envelope)
    );

    let replay = exchange_test_request(
        &authority,
        &storage,
        operator_uid,
        [80_u8; 32],
        request(
            "req_22222222222222222222222222",
            "governance.task-package.prepare",
            capabilities[0].0,
            &capabilities[0].1,
            AdministrativeBody::TaskPackagePrepare(source.clone()),
        ),
    );
    assert_eq!(
        serde_json::to_value(replay.result.unwrap()).unwrap(),
        serde_json::to_value(AdministrativeResult::TaskPackageArtifact(prepared)).unwrap()
    );
    let mut conflicting = source.clone();
    conflicting.plans[0].objective = "A different proposal with the same request".to_string();
    assert!(matches!(
        try_exchange_test_request(
            &authority,
            &storage,
            operator_uid,
            [81_u8; 32],
            request(
                "req_22222222222222222222222222",
                "governance.task-package.prepare",
                capabilities[0].0,
                &capabilities[0].1,
                AdministrativeBody::TaskPackagePrepare(conflicting)
            ),
        ),
        Err(AdministrativeProtocolError::Authorization)
    ));
    // A scoped inspection credential cannot mutate proposals.
    assert!(matches!(
        try_exchange_test_request(
            &authority,
            &storage,
            operator_uid,
            [82_u8; 32],
            request(
                "req_77777777777777777777777777",
                "governance.task-package.prepare",
                capabilities[1].0,
                &capabilities[1].1,
                AdministrativeBody::TaskPackagePrepare(source)
            ),
        ),
        Err(AdministrativeProtocolError::Authorization)
    ));

    let inspected = exchange_test_request(
        &authority,
        &storage,
        operator_uid,
        [77_u8; 32],
        request(
            "req_33333333333333333333333333",
            "governance.task-package.inspect",
            capabilities[1].0,
            &capabilities[1].1,
            AdministrativeBody::TaskPackageInspect(TaskPackageInspectBody {
                contract_version: "task-package-inspect-v1".to_string(),
                request_id: "req_44444444444444444444444444".to_string(),
                project_id: registration.project_id.clone(),
                workspace_id: registration.workspace_id.clone(),
                expected_state_revision: 0,
                proposal_id: proposal.proposal_id.clone(),
            }),
        ),
    );
    let AdministrativeResult::TaskPackageArtifact(inspected) = inspected.result.unwrap() else {
        panic!("inspected proposal expected");
    };
    assert_eq!(inspected.proposal.as_ref(), Some(&proposal));
    assert_eq!(inspected.receipt.state, "inspected");
    assert!(matches!(
        try_exchange_test_request(
            &authority,
            &storage,
            operator_uid,
            [85_u8; 32],
            request(
                "req_88888888888888888888888888",
                "governance.task-package.inspect",
                capabilities[1].0,
                &capabilities[1].1,
                AdministrativeBody::TaskPackageInspect(TaskPackageInspectBody {
                    contract_version: "task-package-inspect-v1".to_string(),
                    request_id: "req_88888888888888888888888888".to_string(),
                    project_id: registration.project_id.clone(),
                    workspace_id: "wsp_99999999999999999999999999".to_string(),
                    expected_state_revision: 0,
                    proposal_id: proposal.proposal_id.clone(),
                })
            ),
        ),
        Err(AdministrativeProtocolError::Authorization)
    ));

    let attestation = OfficialTaskPackageAttestation {
        contract_version: crate::task_package_contracts::TASK_PACKAGE_ATTESTATION_V1.to_string(),
        attestation_id: "att_11111111111111111111111111".to_string(),
        proposal_id: proposal.proposal_id.clone(),
        proposal_sha256: proposal.payload_sha256.clone(),
        source_sha256: proposal.source_sha256.clone(),
        project_id: registration.project_id.clone(),
        workspace_id: registration.workspace_id.clone(),
        expected_state_revision: 0,
        verifier_subject_sha256: crate::service_storage::task_package_subject_sha256(verifier_uid),
        decision: "accepted".to_string(),
        attested_at: "unix:1".to_string(),
        state: "attested".to_string(),
    };
    let mut forged = attestation.clone();
    forged.verifier_subject_sha256 = "f".repeat(64);
    assert!(matches!(
        try_exchange_test_request(
            &authority,
            &storage,
            verifier_uid,
            [83_u8; 32],
            request(
                "req_77777777777777777777777777",
                "governance.task-package.attest",
                capabilities[2].0,
                &capabilities[2].1,
                AdministrativeBody::TaskPackageAttest(forged)
            ),
        ),
        Err(AdministrativeProtocolError::Authorization)
    ));
    let attested = exchange_test_request(
        &authority,
        &storage,
        verifier_uid,
        [78_u8; 32],
        request(
            "req_55555555555555555555555555",
            "governance.task-package.attest",
            capabilities[2].0,
            &capabilities[2].1,
            AdministrativeBody::TaskPackageAttest(attestation.clone()),
        ),
    );
    let AdministrativeResult::TaskPackageDecision(attested) = attested.result.unwrap() else {
        panic!("attestation receipt expected");
    };
    assert_eq!(attested.state, "attested");
    assert_eq!(attested.resulting_state_revision, 0);

    let approval = OfficialTaskPackageApproval {
        contract_version: crate::task_package_contracts::TASK_PACKAGE_APPROVAL_V1.to_string(),
        approval_id: "apr_11111111111111111111111111".to_string(),
        proposal_id: proposal.proposal_id.clone(),
        proposal_sha256: proposal.payload_sha256.clone(),
        attestation_id: attestation.attestation_id,
        attestation_sha256: attested.artifact_sha256,
        project_id: registration.project_id,
        workspace_id: registration.workspace_id,
        expected_state_revision: 0,
        operator_subject_sha256: crate::service_storage::task_package_subject_sha256(operator_uid),
        decision: "approved".to_string(),
        approved_at: "unix:2".to_string(),
        state: "approved".to_string(),
    };
    // Even a separately issued operator grant cannot approve its UID's own attestation.
    let reused_subject_secret = [86_u8; 32];
    let reused_subject_grant = "cap_44444444444444444444444444";
    storage
        .registry()
        .store_capability_grant(&NewCapabilityGrant {
            grant_id: reused_subject_grant.to_string(),
            secret_hash: sha256(&reused_subject_secret),
            subject_uid: verifier_uid,
            actor: CapabilityActor::Operator,
            channel: CapabilityChannel::Administrative,
            operation: CapabilityOperation::StateAdmin,
            project_id: Some(approval.project_id.clone()),
            workspace_id: Some(approval.workspace_id.clone()),
            task_id: None,
            not_before_unix: now - 60,
            expires_at_unix: now + 600,
            revocation_epoch: 0,
            granted_at: format!("unix:{}", now - 60),
        })
        .unwrap();
    let mut self_approval = approval.clone();
    self_approval.operator_subject_sha256 =
        crate::service_storage::task_package_subject_sha256(verifier_uid);
    assert!(matches!(
        try_exchange_test_request(
            &authority,
            &storage,
            verifier_uid,
            [87_u8; 32],
            request(
                "req_99999999999999999999999999",
                "governance.task-package.approve",
                reused_subject_grant,
                &reused_subject_secret,
                AdministrativeBody::TaskPackageApprove(self_approval)
            ),
        ),
        Err(AdministrativeProtocolError::Authorization)
    ));
    let approved = exchange_test_request(
        &authority,
        &storage,
        operator_uid,
        [79_u8; 32],
        request(
            "req_66666666666666666666666666",
            "governance.task-package.approve",
            capabilities[0].0,
            &capabilities[0].1,
            AdministrativeBody::TaskPackageApprove(approval.clone()),
        ),
    );
    let AdministrativeResult::TaskPackageDecision(approved) = approved.result.unwrap() else {
        panic!("approval receipt expected");
    };
    assert_eq!(approved.state, "approved");
    assert_eq!(approved.previous_state_revision, 0);
    assert_eq!(approved.resulting_state_revision, 0);
    let package = GovernanceTaskPackageV2 {
        contract_version: crate::task_package_contracts::GOVERNANCE_TASK_PACKAGE_V2.to_string(),
        request_id: "req_88888888888888888888888888".to_string(),
        project_id: approval.project_id.clone(),
        workspace_id: approval.workspace_id.clone(),
        expected_state_revision: 0,
        proposal_id: approval.proposal_id.clone(),
        proposal_sha256: approval.proposal_sha256.clone(),
        attestation_id: approval.attestation_id.clone(),
        attestation_sha256: approval.attestation_sha256.clone(),
        approval_id: approval.approval_id.clone(),
        approval_sha256: approved.artifact_sha256.clone(),
        active_plan_id: proposal.plans[0].plan_id.clone(),
        current_task_id: proposal.plans[0].tasks[0].task_id.clone(),
        plans: proposal.plans,
    };
    // The operator approval credential does not also authorize installation.
    assert!(matches!(
        try_exchange_test_request(
            &authority,
            &storage,
            operator_uid,
            [88_u8; 32],
            request(
                "req_88888888888888888888888888",
                "governance.task-package.install",
                capabilities[0].0,
                &capabilities[0].1,
                AdministrativeBody::TaskPackageInstallV2(package.clone())
            ),
        ),
        Err(AdministrativeProtocolError::Authorization)
    ));
    let install_secret = [89_u8; 32];
    let install_id = "cap_55555555555555555555555555";
    storage
        .registry()
        .store_capability_grant(&NewCapabilityGrant {
            grant_id: install_id.to_string(),
            secret_hash: sha256(&install_secret),
            subject_uid: operator_uid,
            actor: CapabilityActor::Ci,
            channel: CapabilityChannel::Administrative,
            operation: CapabilityOperation::InstallationAdmin,
            project_id: Some(package.project_id.clone()),
            workspace_id: Some(package.workspace_id.clone()),
            task_id: None,
            not_before_unix: now - 60,
            expires_at_unix: now + 600,
            revocation_epoch: 0,
            granted_at: format!("unix:{}", now - 60),
        })
        .unwrap();
    // A new private output proves the v2 client path and response validation.
    let install_input = root.join("installation.json");
    let install_credential = root.join("installer.json");
    fs::write(&install_input, serde_json::to_vec(&package).unwrap()).unwrap();
    fs::write(
        &install_credential,
        serde_json::to_vec(&AdministrativeCapabilityBundle {
            contract_version: "admin-capability-v1".to_string(),
            capability: WireCapability {
                capability_id: install_id.to_string(),
                token: encode_base64url(&install_secret),
            },
        })
        .unwrap(),
    )
    .unwrap();
    fs::set_permissions(&install_credential, fs::Permissions::from_mode(0o600)).unwrap();
    let install_output = root.join("installed.json");
    let output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&install_output)
        .unwrap();
    fs::set_permissions(&install_output, fs::Permissions::from_mode(0o600)).unwrap();
    std::env::set_var(ADMIN_SOCKET_ENV, &endpoint);
    std::env::set_var(
        ADMIN_CAPABILITY_FD_ENV,
        fs::File::open(&install_credential)
            .unwrap()
            .into_raw_fd()
            .to_string(),
    );
    std::env::set_var(
        TASK_PACKAGE_INPUT_FD_ENV,
        fs::File::open(&install_input)
            .unwrap()
            .into_raw_fd()
            .to_string(),
    );
    std::env::set_var(TASK_PACKAGE_OUTPUT_FD_ENV, output.into_raw_fd().to_string());
    let installed = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let (mut stream, _) = listener.accept().unwrap();
            let nonce = [90_u8; 32];
            answer_administrative_hello(&mut stream, &nonce).unwrap();
            serve_administrative_request(
                &mut stream,
                &ConnectionAuthentication::for_test(operator_uid, nonce),
                &authority,
                &storage,
            )
            .unwrap();
        });
        let installed = run_official_task_package_operator_inner("install").unwrap();
        worker.join().unwrap();
        installed
    });
    let expected = crate::task_package_install::installation_receipt(&package).unwrap();
    assert_eq!(
        serde_json::from_slice::<TaskPackageOperationReceipt>(&fs::read(&install_output).unwrap())
            .unwrap(),
        expected
    );
    validate_official_task_package_result(
        &AdministrativeBody::TaskPackageInstallV2(package.clone()),
        &installed,
    )
    .unwrap();
    for nonce in [[91_u8; 32], [92_u8; 32]] {
        let repeated = exchange_test_request(
            &authority,
            &storage,
            operator_uid,
            nonce,
            request(
                "req_88888888888888888888888888",
                "governance.task-package.install",
                install_id,
                &install_secret,
                AdministrativeBody::TaskPackageInstallV2(package.clone()),
            ),
        );
        let AdministrativeResult::TaskPackageDecision(receipt) = repeated.result.unwrap() else {
            panic!("installation receipt expected");
        };
        assert_eq!(receipt, expected);
    }
    fs::remove_dir_all(root).unwrap();
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
fn task_package_output_requires_a_private_empty_descriptor_and_writes_bounded_json() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/admin-protocol-tests/task-package-output-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let value = serde_json::json!({"contract_version":"task-package-output-test-v1"});

    let private_path = root.join("private.json");
    let private = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&private_path)
        .unwrap();
    fs::set_permissions(&private_path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut output =
        open_private_task_package_output(private.into_raw_fd().to_string().into()).unwrap();
    write_bounded_private_json(&mut output, &value).unwrap();
    let persisted: Value = serde_json::from_slice(&fs::read(&private_path).unwrap()).unwrap();
    assert_eq!(persisted, value);

    let nonempty_path = root.join("nonempty.json");
    fs::write(&nonempty_path, b"occupied").unwrap();
    fs::set_permissions(&nonempty_path, fs::Permissions::from_mode(0o600)).unwrap();
    let nonempty = OpenOptions::new().write(true).open(&nonempty_path).unwrap();
    assert!(matches!(
        open_private_task_package_output(nonempty.into_raw_fd().to_string().into()),
        Err(AdministrativeProtocolError::Authorization)
    ));
    assert_eq!(fs::read(&nonempty_path).unwrap(), b"occupied");

    let public_path = root.join("public.json");
    let public = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&public_path)
        .unwrap();
    fs::set_permissions(&public_path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        open_private_task_package_output(public.into_raw_fd().to_string().into()),
        Err(AdministrativeProtocolError::Authorization)
    ));
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
