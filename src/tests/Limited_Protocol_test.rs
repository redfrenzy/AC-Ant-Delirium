use super::*;

#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[test]
fn nonce_encoding_matches_the_protocol_shape() {
    let encoded = encode_base64url(&[0xff_u8; 32]);
    assert_eq!(encoded.len(), 43);
    assert!(is_encoded_nonce(&encoded));
    assert!(!encoded.contains('='));
    assert!(!is_encoded_nonce("short"));
    assert_eq!(decode_base64url(&encoded), Ok([0xff_u8; 32]));
    let mut noncanonical = encoded;
    noncanonical.replace_range(42..43, "_");
    assert_eq!(
        decode_base64url(&noncanonical),
        Err(LimitedProtocolError::Envelope)
    );
}

#[cfg(unix)]
#[test]
fn inherited_capability_descriptor_is_bounded_typed_and_closed_after_read() {
    let bundle = LimitedCapabilityBundle {
        contract_version: "limited-capability-v1".to_string(),
        project_id: "prj_00000000000000000000000000".to_string(),
        workspace_id: "wsp_00000000000000000000000000".to_string(),
        compile: WireCapability {
            capability_id: "cap_00000000000000000000000000".to_string(),
            token: encode_base64url(&[7_u8; 32]),
        },
        projection: WireCapability {
            capability_id: "cap_11111111111111111111111111".to_string(),
            token: encode_base64url(&[8_u8; 32]),
        },
    };
    let payload = serde_json::to_vec(&bundle).unwrap();
    let mut descriptors = [0_i32; 2];
    assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
    assert_eq!(
        unsafe { libc::write(descriptors[1], payload.as_ptr().cast(), payload.len()) },
        payload.len() as isize
    );
    unsafe {
        libc::close(descriptors[1]);
    }

    let decoded = read_capability_bundle_from_descriptor(descriptors[0]).unwrap();
    assert_eq!(decoded.project_id, bundle.project_id);
    assert!(valid_capability_bundle(&decoded));
    assert_eq!(unsafe { libc::fcntl(descriptors[0], libc::F_GETFD) }, -1);
}

#[cfg(unix)]
#[test]
fn bootstrap_compile_and_projection_complete_one_typed_limited_exchange() {
    use std::time::{SystemTime, UNIX_EPOCH};

    use ac_ant_delirium::structs::{AuditStore, ProjectDatabase};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/lp/{}-{}",
        std::process::id(),
        nonce % 1_000_000_000_000_u128,
    ));
    std::fs::create_dir_all(&root).unwrap();
    let authority = crate::service_authority::VerifiedServiceAuthority::for_current_test_process()
        .expect("Linux effective uid");
    let storage_root = root.join("service-state");
    std::fs::create_dir(&storage_root).unwrap();
    let storage =
        crate::service_storage::ProtectedServiceStorage::initialize(&authority, &storage_root)
            .unwrap();
    let peer_uid = unsafe { libc::geteuid() };
    let bootstrap_authentication =
        crate::service_authentication::ConnectionAuthentication::for_test(peer_uid, [1_u8; 32]);
    let bootstrap = execute_typed_limited_request(
        &bootstrap_authentication,
        &authority,
        &storage,
        "project.bootstrap.observe",
        None,
        LimitedRequestBody {
            identity_fingerprint: Some(encode_base64url(&[31_u8; 32])),
            project_slug: Some("typed-cycle".to_string()),
            ..LimitedRequestBody::default()
        },
    );
    let capabilities = bootstrap.result.unwrap().capability_bundle.unwrap();
    let compile_authentication =
        crate::service_authentication::ConnectionAuthentication::for_test(peer_uid, [2_u8; 32]);
    let compile = execute_typed_limited_request(
        &compile_authentication,
        &authority,
        &storage,
        "compile.observe",
        Some(capabilities.compile.clone()),
        LimitedRequestBody {
            project_id: Some(capabilities.project_id.clone()),
            workspace_id: Some(capabilities.workspace_id.clone()),
            compilation_fingerprint: Some(encode_base64url(&[32_u8; 32])),
            ..LimitedRequestBody::default()
        },
    );
    assert_eq!(compile.result.unwrap().deduplicated, Some(false));
    let projection_authentication =
        crate::service_authentication::ConnectionAuthentication::for_test(peer_uid, [3_u8; 32]);
    let projection = execute_typed_limited_request(
        &projection_authentication,
        &authority,
        &storage,
        "projection.refresh",
        Some(capabilities.projection.clone()),
        LimitedRequestBody {
            project_id: Some(capabilities.project_id.clone()),
            workspace_id: Some(capabilities.workspace_id.clone()),
            ..LimitedRequestBody::default()
        },
    )
    .result
    .unwrap();
    assert!(projection.current_task.unwrap().contains("\"task\": null"));
    assert!(projection
        .governance_errors
        .unwrap()
        .contains("\"violations\": ["));

    let registration = storage
        .registry()
        .provisional_registration(peer_uid, &[31_u8; 32])
        .unwrap()
        .unwrap();
    let database = ProjectDatabase::open_existing_at(
        &storage_root.join("projects").join(registration.project_key),
    )
    .unwrap();
    assert_eq!(
        database
            .audit_event_count(&registration.project_id)
            .unwrap(),
        1
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn execute_typed_limited_request(
    authentication: &crate::service_authentication::ConnectionAuthentication,
    authority: &crate::service_authority::VerifiedServiceAuthority,
    storage: &crate::service_storage::ProtectedServiceStorage,
    operation: &str,
    capability: Option<WireCapability>,
    body: LimitedRequestBody,
) -> LimitedResponse {
    let request_id = random_request_id().unwrap();
    let payload = serde_json::to_vec(&LimitedRequest {
        message_type: "request".to_string(),
        wire_version: LIMITED_WIRE_VERSION.to_string(),
        request_id: request_id.clone(),
        connection_nonce: encode_base64url(authentication.connection_nonce()),
        operation: operation.to_string(),
        capability,
        body,
    })
    .unwrap();
    let response =
        limited_response_for_payload(&payload, authentication, authority, storage).unwrap();
    let encoded_response = serde_json::to_vec(&response).unwrap();
    let decoded: LimitedResponse = serde_json::from_slice(&encoded_response).unwrap();
    assert_eq!(decoded.message_type, "response");
    assert_eq!(decoded.wire_version, LIMITED_WIRE_VERSION);
    assert_eq!(decoded.request_id, request_id);
    assert_eq!(decoded.status, "ok");
    assert!(decoded.result.is_some());
    assert!(decoded.error.is_none());
    decoded
}

#[test]
fn framing_uses_only_the_limited_magic_and_big_endian_length() {
    let mut bytes = Vec::new();
    write_frame(
        &mut bytes,
        &ClientHello {
            message_type: "client_hello",
            channel: "limited",
            wire_versions: [LIMITED_WIRE_VERSION],
            client_nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        },
    )
    .expect("limited frame");

    assert_eq!(&bytes[..4], b"ADL1");
    assert_eq!(
        u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize,
        bytes.len() - 8
    );
}

#[test]
fn framing_rejects_cross_channel_empty_and_oversized_frames_before_allocation() {
    for header in [
        [b'A', b'D', b'A', b'1', 0, 0, 0, 2],
        [b'A', b'D', b'L', b'1', 0, 0, 0, 0],
        [b'A', b'D', b'L', b'1', 0, 0x40, 0, 1],
    ] {
        assert_eq!(
            read_frame(&mut header.as_slice()),
            Err(LimitedProtocolError::Framing)
        );
    }
}

#[test]
fn typed_hello_rejects_unknown_fields_versions_and_invalid_nonces() {
    let unknown = br#"{"message_type":"server_hello","wire_version":"adp/1","connection_nonce":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","max_frame_bytes":4194304,"admin":true}"#;
    assert!(serde_json::from_slice::<ServerHello>(unknown).is_err());

    for hello in [
        ServerHello {
            message_type: "server_hello".to_string(),
            wire_version: "adp/2".to_string(),
            connection_nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            max_frame_bytes: MAX_FRAME_BYTES,
        },
        ServerHello {
            message_type: "server_hello".to_string(),
            wire_version: LIMITED_WIRE_VERSION.to_string(),
            connection_nonce: "short".to_string(),
            max_frame_bytes: MAX_FRAME_BYTES,
        },
    ] {
        assert_eq!(
            validate_server_hello(&hello),
            Err(LimitedProtocolError::Envelope)
        );
    }
}

#[test]
fn service_selects_adp1_only_from_a_bounded_unique_offer() {
    let nonce = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string();
    let valid = ReceivedClientHello {
        message_type: "client_hello".to_string(),
        channel: "limited".to_string(),
        wire_versions: vec!["adp/2".to_string(), LIMITED_WIRE_VERSION.to_string()],
        client_nonce: nonce.clone(),
    };
    assert_eq!(validate_client_hello(&valid), Ok(()));

    for wire_versions in [
        vec![],
        vec!["adp/2".to_string()],
        vec![
            LIMITED_WIRE_VERSION.to_string(),
            LIMITED_WIRE_VERSION.to_string(),
        ],
        vec!["x".repeat(33), LIMITED_WIRE_VERSION.to_string()],
    ] {
        let invalid = ReceivedClientHello {
            message_type: "client_hello".to_string(),
            channel: "limited".to_string(),
            wire_versions,
            client_nonce: nonce.clone(),
        };
        assert_eq!(
            validate_client_hello(&invalid),
            Err(LimitedProtocolError::Envelope)
        );
    }
}

#[test]
fn typed_client_and_server_hellos_round_trip_through_limited_framing() {
    let client_hello = ClientHello {
        message_type: "client_hello",
        channel: "limited",
        wire_versions: [LIMITED_WIRE_VERSION],
        client_nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    };
    let mut client_frame = Vec::new();
    write_frame(&mut client_frame, &client_hello).expect("client hello frame");
    let client_payload = read_frame(&mut client_frame.as_slice()).expect("client hello payload");
    let decoded_client: ReceivedClientHello =
        serde_json::from_slice(&client_payload).expect("typed client hello");
    assert_eq!(validate_client_hello(&decoded_client), Ok(()));

    let server_hello = ServerHello {
        message_type: "server_hello".to_string(),
        wire_version: LIMITED_WIRE_VERSION.to_string(),
        connection_nonce: encode_base64url(&[7_u8; 32]),
        max_frame_bytes: MAX_FRAME_BYTES,
    };
    let mut server_frame = Vec::new();
    write_frame(&mut server_frame, &server_hello).expect("server hello frame");
    let server_payload = read_frame(&mut server_frame.as_slice()).expect("server hello payload");
    let decoded_server: ServerHello =
        serde_json::from_slice(&server_payload).expect("typed server hello");
    assert_eq!(validate_server_hello(&decoded_server), Ok(()));
    assert_eq!(
        decoded_server.connection_nonce,
        encode_base64url(&[7_u8; 32])
    );
}

#[cfg(unix)]
#[test]
fn client_socket_descriptor_is_close_on_exec_before_transport() {
    let (client, _service) = UnixStream::pair().expect("limited socket pair");
    set_descriptor_close_on_exec(client.as_raw_fd()).expect("close-on-exec");
    let flags = unsafe { libc::fcntl(client.as_raw_fd(), libc::F_GETFD) };
    assert!(flags >= 0);
    assert_ne!(flags & libc::FD_CLOEXEC, 0);
}

#[cfg(unix)]
#[test]
fn client_endpoint_is_lexical_absolute_and_never_a_default_path() {
    assert!(is_absolute_socket_endpoint(OsStr::new(
        "/runtime/agent.sock"
    )));
    assert!(!is_absolute_socket_endpoint(OsStr::new("agent.sock")));
    assert!(!is_absolute_socket_endpoint(OsStr::new("")));
}

#[test]
fn client_source_has_no_admin_wire_or_protected_file_api() {
    let source = include_str!("../Limited-Protocol_unit.rs");
    for forbidden in [
        "ADA1",
        "admin.sock",
        "std::fs",
        "File::open",
        "read_link",
        "read_dir",
        "canonicalize",
        "plan/",
        "plans/",
        "state.db",
    ] {
        assert!(
            !source.contains(forbidden),
            "forbidden client surface: {forbidden}"
        );
    }
}
