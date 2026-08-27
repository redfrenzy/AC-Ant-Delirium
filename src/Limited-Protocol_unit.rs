#[cfg(unix)]
use std::ffi::OsStr;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::{fill_system_random, random_crockford_id};
use ac_ant_delirium::structs::{
    AuditStore, AuthorizedSubmissionMetadata, CapabilityActor, CapabilityAuthorizationOutcome,
    CapabilityAuthorizationRequest, CapabilityChannel, CapabilityOperation, EventWriteOutcome,
    NewAuditEvent, SubmissionCreateRequest, SubmissionEvidence, SubmissionWriteOutcome,
    AUDIT_EVENT_CONTRACT_VERSION,
};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use crate::service_authentication::ConnectionAuthentication;
#[cfg(unix)]
use crate::service_authority::VerifiedServiceAuthority;
#[cfg(unix)]
use crate::service_hardening::set_descriptor_close_on_exec;
#[cfg(unix)]
use crate::service_storage::{ProtectedServiceStorage, ProvisionalBootstrapResult};

const LIMITED_MAGIC: [u8; 4] = *b"ADL1";
const LIMITED_WIRE_VERSION: &str = "adp/1";
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const HANDSHAKE_DEADLINE: Duration = Duration::from_millis(500);
const REQUEST_RESPONSE_DEADLINE: Duration = Duration::from_secs(5);
const MAX_PROJECT_SLUG_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LimitedProtocolError {
    Endpoint,
    Transport,
    Read,
    Write,
    Deadline,
    Framing,
    Envelope,
    Entropy,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ClientHello<'a> {
    message_type: &'static str,
    channel: &'static str,
    wire_versions: [&'static str; 1],
    client_nonce: &'a str,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerHello {
    message_type: String,
    wire_version: String,
    connection_nonce: String,
    max_frame_bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceivedClientHello {
    message_type: String,
    channel: String,
    wire_versions: Vec<String>,
    client_nonce: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCapability {
    capability_id: String,
    token: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LimitedCapabilityBundle {
    contract_version: String,
    project_id: String,
    workspace_id: String,
    compile: WireCapability,
    projection: WireCapability,
}

#[derive(Clone)]
pub(crate) struct LimitedAuditObservation {
    pub(crate) identity_fingerprint: [u8; 32],
    pub(crate) compilation_fingerprint: [u8; 32],
    pub(crate) project_slug: String,
}

pub(crate) struct LimitedProjectionDocuments {
    pub(crate) current_task: Vec<u8>,
    pub(crate) governance_errors: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitedRequest {
    message_type: String,
    wire_version: String,
    request_id: String,
    connection_nonce: String,
    operation: String,
    capability: Option<WireCapability>,
    body: LimitedRequestBody,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitedRequestBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compilation_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    submission: Option<SubmissionWireBody>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmissionWireEvidence {
    kind: String,
    text: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmissionWireBody {
    contract_version: String,
    request_id: String,
    project_id: String,
    workspace_id: String,
    task_id: String,
    task_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_revision: Option<String>,
    evidence: Vec<SubmissionWireEvidence>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitedResponse {
    message_type: String,
    wire_version: String,
    request_id: String,
    status: String,
    result: Option<LimitedResponseResult>,
    error: Option<LimitedWireError>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitedResponseResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capability_bundle: Option<LimitedCapabilityBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deduplicated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    governance_errors: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    submission_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_sequence: Option<i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitedWireError {
    code: String,
    category: String,
    retryable: bool,
    message: String,
    action: String,
    correlation_id: Option<String>,
}

#[cfg(unix)]
pub(crate) struct NegotiatedLimitedConnection {
    stream: UnixStream,
    connection_nonce: String,
}

#[cfg(unix)]
impl NegotiatedLimitedConnection {
    #[allow(dead_code)]
    pub(crate) fn connection_nonce(&self) -> &str {
        &self.connection_nonce
    }

    #[allow(dead_code)]
    pub(crate) fn stream(&mut self) -> &mut UnixStream {
        &mut self.stream
    }
}

#[cfg(unix)]
pub(crate) fn run_limited_audit_cycle(
    endpoint: &OsStr,
    observation: &LimitedAuditObservation,
    inherited: Option<LimitedCapabilityBundle>,
) -> Result<LimitedProjectionDocuments, LimitedProtocolError> {
    let capabilities = match inherited {
        Some(capabilities) => capabilities,
        None => bootstrap_capabilities(endpoint, observation)?,
    };
    observe_compilation(endpoint, observation, &capabilities)?;
    refresh_projections(endpoint, &capabilities)
}

#[cfg(unix)]
pub(crate) fn read_capability_bundle_from_descriptor(
    raw_fd: i32,
) -> Result<LimitedCapabilityBundle, LimitedProtocolError> {
    if raw_fd < 3 || unsafe { libc::fcntl(raw_fd, libc::F_GETFD) } < 0 {
        return Err(LimitedProtocolError::Transport);
    }
    let _ = set_descriptor_close_on_exec(raw_fd);
    let deadline = std::time::Instant::now() + HANDSHAKE_DEADLINE;
    let mut payload = Vec::with_capacity(512);
    let mut buffer = [0_u8; 512];
    let result = loop {
        if payload.len() >= 4096 {
            break Err(LimitedProtocolError::Framing);
        }
        if let Err(error) = wait_for(raw_fd, libc::POLLIN, deadline) {
            break Err(error);
        }
        let read = unsafe { libc::read(raw_fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read == 0 {
            break serde_json::from_slice::<LimitedCapabilityBundle>(&payload)
                .map_err(|_| LimitedProtocolError::Envelope)
                .and_then(|bundle| {
                    valid_capability_bundle(&bundle)
                        .then_some(bundle)
                        .ok_or(LimitedProtocolError::Envelope)
                });
        }
        if read < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break Err(LimitedProtocolError::Read);
        }
        payload.extend_from_slice(&buffer[..read as usize]);
    };
    unsafe {
        libc::close(raw_fd);
    }
    result
}

#[cfg(unix)]
fn bootstrap_capabilities(
    endpoint: &OsStr,
    observation: &LimitedAuditObservation,
) -> Result<LimitedCapabilityBundle, LimitedProtocolError> {
    let request = request_for_connection(
        connect_limited_service(endpoint)?,
        "project.bootstrap.observe",
        None,
        LimitedRequestBody {
            identity_fingerprint: Some(encode_base64url(&observation.identity_fingerprint)),
            project_slug: Some(observation.project_slug.clone()),
            ..LimitedRequestBody::default()
        },
    )?;
    request
        .result
        .and_then(|result| result.capability_bundle)
        .filter(valid_capability_bundle)
        .ok_or(LimitedProtocolError::Envelope)
}

#[cfg(unix)]
fn observe_compilation(
    endpoint: &OsStr,
    observation: &LimitedAuditObservation,
    capabilities: &LimitedCapabilityBundle,
) -> Result<(), LimitedProtocolError> {
    let response = request_for_connection(
        connect_limited_service(endpoint)?,
        "compile.observe",
        Some(capabilities.compile.clone()),
        LimitedRequestBody {
            project_id: Some(capabilities.project_id.clone()),
            workspace_id: Some(capabilities.workspace_id.clone()),
            compilation_fingerprint: Some(encode_base64url(&observation.compilation_fingerprint)),
            ..LimitedRequestBody::default()
        },
    )?;
    response
        .result
        .and_then(|result| result.deduplicated)
        .map(|_| ())
        .ok_or(LimitedProtocolError::Envelope)
}

#[cfg(unix)]
fn refresh_projections(
    endpoint: &OsStr,
    capabilities: &LimitedCapabilityBundle,
) -> Result<LimitedProjectionDocuments, LimitedProtocolError> {
    let response = request_for_connection(
        connect_limited_service(endpoint)?,
        "projection.refresh",
        Some(capabilities.projection.clone()),
        LimitedRequestBody {
            project_id: Some(capabilities.project_id.clone()),
            workspace_id: Some(capabilities.workspace_id.clone()),
            ..LimitedRequestBody::default()
        },
    )?;
    let result = response.result.ok_or(LimitedProtocolError::Envelope)?;
    Ok(LimitedProjectionDocuments {
        current_task: result
            .current_task
            .ok_or(LimitedProtocolError::Envelope)?
            .into_bytes(),
        governance_errors: result
            .governance_errors
            .ok_or(LimitedProtocolError::Envelope)?
            .into_bytes(),
    })
}

#[cfg(unix)]
fn request_for_connection(
    mut connection: NegotiatedLimitedConnection,
    operation: &str,
    capability: Option<WireCapability>,
    body: LimitedRequestBody,
) -> Result<LimitedResponse, LimitedProtocolError> {
    let request_id = random_request_id()?;
    let request = LimitedRequest {
        message_type: "request".to_string(),
        wire_version: LIMITED_WIRE_VERSION.to_string(),
        request_id: request_id.clone(),
        connection_nonce: connection.connection_nonce.clone(),
        operation: operation.to_string(),
        capability,
        body,
    };
    write_stream_frame(&mut connection.stream, &request)?;
    let payload =
        read_stream_frame_with_timeout(&mut connection.stream, REQUEST_RESPONSE_DEADLINE)?;
    let response: LimitedResponse =
        serde_json::from_slice(&payload).map_err(|_| LimitedProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != LIMITED_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.result.is_none()
        || response.error.is_some()
    {
        return Err(LimitedProtocolError::Envelope);
    }
    Ok(response)
}

/// Opens only the installation-provided limited endpoint and negotiates adp/1.
/// This client has no representation for administrative operations or state
/// locations. Capability-bound requests are added by T6.5.
#[cfg(unix)]
pub(crate) fn connect_limited_service(
    endpoint: &OsStr,
) -> Result<NegotiatedLimitedConnection, LimitedProtocolError> {
    if !is_absolute_socket_endpoint(endpoint) {
        return Err(LimitedProtocolError::Endpoint);
    }
    let stream = UnixStream::connect(endpoint).map_err(|_| LimitedProtocolError::Transport)?;
    negotiate_limited_stream(stream)
}

#[cfg(unix)]
fn is_absolute_socket_endpoint(endpoint: &OsStr) -> bool {
    std::path::Path::new(endpoint).is_absolute()
}

#[cfg(unix)]
fn negotiate_limited_stream(
    mut stream: UnixStream,
) -> Result<NegotiatedLimitedConnection, LimitedProtocolError> {
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| LimitedProtocolError::Transport)?;
    let client_nonce = random_nonce()?;
    let hello = ClientHello {
        message_type: "client_hello",
        channel: "limited",
        wire_versions: [LIMITED_WIRE_VERSION],
        client_nonce: &client_nonce,
    };
    write_stream_frame(&mut stream, &hello)?;

    let response = read_stream_frame(&mut stream)?;
    let hello: ServerHello =
        serde_json::from_slice(&response).map_err(|_| LimitedProtocolError::Envelope)?;
    validate_server_hello(&hello)?;
    Ok(NegotiatedLimitedConnection {
        stream,
        connection_nonce: hello.connection_nonce,
    })
}

#[cfg(unix)]
pub(crate) fn answer_limited_hello(
    stream: &mut UnixStream,
    connection_nonce: &[u8; 32],
) -> Result<(), LimitedProtocolError> {
    let request = read_stream_frame(stream)?;
    let hello: ReceivedClientHello =
        serde_json::from_slice(&request).map_err(|_| LimitedProtocolError::Envelope)?;
    validate_client_hello(&hello)?;

    let connection_nonce = encode_base64url(connection_nonce);
    let response = ServerHello {
        message_type: "server_hello".to_string(),
        wire_version: LIMITED_WIRE_VERSION.to_string(),
        connection_nonce,
        max_frame_bytes: MAX_FRAME_BYTES,
    };
    write_stream_frame(stream, &response)
}

#[cfg(unix)]
pub(crate) fn serve_limited_request(
    stream: &mut UnixStream,
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
) -> Result<(), LimitedProtocolError> {
    let payload = read_stream_frame(stream)?;
    let response = limited_response_for_payload(&payload, authentication, authority, storage)?;
    write_stream_frame(stream, &response)
}

#[cfg(unix)]
fn limited_response_for_payload(
    payload: &[u8],
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
) -> Result<LimitedResponse, LimitedProtocolError> {
    let request: LimitedRequest =
        serde_json::from_slice(payload).map_err(|_| LimitedProtocolError::Envelope)?;
    validate_request_envelope(&request, authentication.connection_nonce())?;
    let result = match request.operation.as_str() {
        "project.bootstrap.observe" => {
            handle_bootstrap_request(authentication, authority, storage, &request)?
        }
        "compile.observe" => handle_compile_request(authentication, authority, storage, &request)?,
        "projection.refresh" => {
            handle_projection_request(authentication, authority, storage, &request)?
        }
        "submission.create" => {
            handle_submission_request(authentication, authority, storage, &request)?
        }
        _ => return Err(LimitedProtocolError::Envelope),
    };
    Ok(LimitedResponse {
        message_type: "response".to_string(),
        wire_version: LIMITED_WIRE_VERSION.to_string(),
        request_id: request.request_id,
        status: "ok".to_string(),
        result: Some(result),
        error: None,
    })
}

#[cfg(unix)]
fn handle_bootstrap_request(
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
    request: &LimitedRequest,
) -> Result<LimitedResponseResult, LimitedProtocolError> {
    if request.capability.is_some()
        || request.body.project_id.is_some()
        || request.body.workspace_id.is_some()
        || request.body.compilation_fingerprint.is_some()
    {
        return Err(LimitedProtocolError::Envelope);
    }
    let identity = decode_base64url(
        request
            .body
            .identity_fingerprint
            .as_deref()
            .ok_or(LimitedProtocolError::Envelope)?,
    )?;
    let slug = request
        .body
        .project_slug
        .as_deref()
        .filter(|slug| !slug.is_empty() && slug.len() <= MAX_PROJECT_SLUG_BYTES)
        .ok_or(LimitedProtocolError::Envelope)?;
    let bootstrap = storage
        .bootstrap_provisional(authority, authentication.peer_uid(), &identity, slug)
        .map_err(|_| LimitedProtocolError::Envelope)?;
    Ok(LimitedResponseResult {
        capability_bundle: Some(bundle_from_bootstrap(bootstrap)),
        ..LimitedResponseResult::default()
    })
}

#[cfg(unix)]
fn handle_compile_request(
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
    request: &LimitedRequest,
) -> Result<LimitedResponseResult, LimitedProtocolError> {
    if request.body.identity_fingerprint.is_some() || request.body.project_slug.is_some() {
        return Err(LimitedProtocolError::Envelope);
    }
    let compilation_fingerprint = decode_base64url(
        request
            .body
            .compilation_fingerprint
            .as_deref()
            .ok_or(LimitedProtocolError::Envelope)?,
    )?;
    let (outcome, resource) = authorize_scoped_wrapper_request(
        authentication,
        authority,
        storage,
        request,
        CapabilityOperation::CompileObserve,
    )?;
    let now = trusted_unix_time()?;
    let event = NewAuditEvent {
        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
        event_id: random_crockford_id("evt_").map_err(|_| LimitedProtocolError::Entropy)?,
        project_id: resource.project_id().to_string(),
        workspace_id: resource.workspace_id().map(str::to_string),
        actor_kind: "wrapper".to_string(),
        actor_subject_id: "cargo-workspace-wrapper".to_string(),
        event_kind: "compilation_observed".to_string(),
        outcome: "succeeded".to_string(),
        occurred_at: format!("unix:{now}"),
        fingerprint: lowercase_hex(&compilation_fingerprint),
        correlation_id: random_crockford_id("ref_").map_err(|_| LimitedProtocolError::Entropy)?,
        resource_id: None,
        payload: b"audit-only compilation observed".to_vec(),
    };
    let write = resource
        .database()
        .append_audit_event(&event)
        .map_err(|_| LimitedProtocolError::Envelope)?;
    Ok(LimitedResponseResult {
        deduplicated: Some(
            matches!(outcome, CapabilityAuthorizationOutcome::Replay(_))
                || matches!(write, EventWriteOutcome::Duplicate { .. }),
        ),
        ..LimitedResponseResult::default()
    })
}

#[cfg(unix)]
fn handle_projection_request(
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
    request: &LimitedRequest,
) -> Result<LimitedResponseResult, LimitedProtocolError> {
    if request.body.identity_fingerprint.is_some()
        || request.body.project_slug.is_some()
        || request.body.compilation_fingerprint.is_some()
    {
        return Err(LimitedProtocolError::Envelope);
    }
    let (_outcome, resource) = authorize_scoped_wrapper_request(
        authentication,
        authority,
        storage,
        request,
        CapabilityOperation::ProjectionRefresh,
    )?;
    let project_id = resource.project_id();
    let workspace_id = resource
        .workspace_id()
        .ok_or(LimitedProtocolError::Envelope)?;
    let rendered_at = format!("unix:{}", trusted_unix_time()?);
    let current_task = resource
        .database()
        .render_current_task_projection(project_id, workspace_id, &rendered_at)
        .map_err(|_| LimitedProtocolError::Envelope)?;
    let governance_errors = resource
        .database()
        .render_governance_errors_projection(project_id, workspace_id, &rendered_at)
        .map_err(|_| LimitedProtocolError::Envelope)?;
    Ok(LimitedResponseResult {
        current_task: Some(
            String::from_utf8(current_task).map_err(|_| LimitedProtocolError::Envelope)?,
        ),
        governance_errors: Some(
            String::from_utf8(governance_errors).map_err(|_| LimitedProtocolError::Envelope)?,
        ),
        ..LimitedResponseResult::default()
    })
}

#[cfg(unix)]
fn handle_submission_request(
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
    request: &LimitedRequest,
) -> Result<LimitedResponseResult, LimitedProtocolError> {
    if request.body.identity_fingerprint.is_some()
        || request.body.project_slug.is_some()
        || request.body.project_id.is_some()
        || request.body.workspace_id.is_some()
        || request.body.compilation_fingerprint.is_some()
    {
        return Err(LimitedProtocolError::Envelope);
    }
    let wire = request
        .body
        .submission
        .as_ref()
        .ok_or(LimitedProtocolError::Envelope)?;
    if wire.request_id != request.request_id {
        return Err(LimitedProtocolError::Envelope);
    }
    let submission = submission_from_wire(wire)?;
    let payload = ac_ant_delirium::structs::serialize_submission_request(&submission)
        .map_err(|_| LimitedProtocolError::Envelope)?;
    let (authorization, resource) =
        authorize_scoped_agent_submission(authentication, authority, storage, request, wire)?;
    let fingerprint = lowercase_hex(&sha256(&payload));
    let received_at = format!("unix:{}", trusted_unix_time()?);
    let submission_id = random_crockford_id("sub_").map_err(|_| LimitedProtocolError::Entropy)?;
    let event = NewAuditEvent {
        contract_version: AUDIT_EVENT_CONTRACT_VERSION.to_string(),
        event_id: random_crockford_id("evt_").map_err(|_| LimitedProtocolError::Entropy)?,
        project_id: wire.project_id.clone(),
        workspace_id: Some(wire.workspace_id.clone()),
        actor_kind: "agent".to_string(),
        actor_subject_id: "authorized-agent-capability".to_string(),
        event_kind: "submission_recorded".to_string(),
        outcome: "succeeded".to_string(),
        occurred_at: received_at.clone(),
        fingerprint: fingerprint.clone(),
        correlation_id: random_crockford_id("ref_").map_err(|_| LimitedProtocolError::Entropy)?,
        resource_id: Some(submission_id.clone()),
        payload: b"immutable submission received".to_vec(),
    };
    let outcome = ac_ant_delirium::structs::record_authorized_submission(
        resource.database(),
        &submission,
        &AuthorizedSubmissionMetadata {
            submission_id,
            fingerprint,
            received_at,
        },
        &event,
    )
    .map_err(|_| LimitedProtocolError::Envelope)?;
    Ok(LimitedResponseResult {
        submission_id: Some(outcome.submission_id().to_string()),
        event_sequence: Some(outcome.event_sequence()),
        deduplicated: Some(
            matches!(authorization, CapabilityAuthorizationOutcome::Replay(_))
                || matches!(outcome, SubmissionWriteOutcome::Duplicate { .. }),
        ),
        ..LimitedResponseResult::default()
    })
}

fn submission_from_wire(
    wire: &SubmissionWireBody,
) -> Result<SubmissionCreateRequest, LimitedProtocolError> {
    let evidence = wire
        .evidence
        .iter()
        .map(|item| {
            if item.kind != "note" {
                return Err(LimitedProtocolError::Envelope);
            }
            Ok(SubmissionEvidence::Note {
                text: item.text.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SubmissionCreateRequest {
        contract_version: wire.contract_version.clone(),
        request_id: wire.request_id.clone(),
        project_id: wire.project_id.clone(),
        workspace_id: wire.workspace_id.clone(),
        task_id: wire.task_id.clone(),
        task_revision: wire.task_revision,
        base_revision: wire.base_revision.clone(),
        evidence,
    })
}

#[cfg(unix)]
fn authorize_scoped_agent_submission(
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
    request: &LimitedRequest,
    submission: &SubmissionWireBody,
) -> Result<
    (
        CapabilityAuthorizationOutcome,
        crate::service_storage::ResolvedAuthorizedResource,
    ),
    LimitedProtocolError,
> {
    let capability = request
        .capability
        .as_ref()
        .filter(|capability| valid_opaque_id(&capability.capability_id, "cap_"))
        .ok_or(LimitedProtocolError::Envelope)?;
    if !valid_opaque_id(&submission.project_id, "prj_")
        || !valid_opaque_id(&submission.workspace_id, "wsp_")
        || !valid_opaque_id(&submission.task_id, "tsk_")
    {
        return Err(LimitedProtocolError::Envelope);
    }
    let mut fingerprint_material = CapabilityOperation::SubmissionCreate
        .storage_value()
        .as_bytes()
        .to_vec();
    fingerprint_material.push(0);
    fingerprint_material.extend_from_slice(
        &serde_json::to_vec(&request.body).map_err(|_| LimitedProtocolError::Envelope)?,
    );
    let authorization = CapabilityAuthorizationRequest {
        grant_id: capability.capability_id.clone(),
        presented_secret: decode_base64url(&capability.token)?,
        operation: CapabilityOperation::SubmissionCreate,
        project_id: Some(submission.project_id.clone()),
        workspace_id: Some(submission.workspace_id.clone()),
        task_id: Some(submission.task_id.clone()),
        presented_connection_nonce: decode_base64url(&request.connection_nonce)?,
        request_id: request.request_id.clone(),
        request_fingerprint: sha256(&fingerprint_material),
    };
    let outcome = authentication
        .authorize(
            storage.registry(),
            CapabilityChannel::Limited,
            &authorization,
        )
        .map_err(|_| LimitedProtocolError::Envelope)?;
    let authorized = match &outcome {
        CapabilityAuthorizationOutcome::Fresh(capability)
        | CapabilityAuthorizationOutcome::Replay(capability)
            if capability.actor() == CapabilityActor::Agent
                && capability.operation() == CapabilityOperation::SubmissionCreate =>
        {
            capability
        }
        _ => return Err(LimitedProtocolError::Envelope),
    };
    let resource = storage
        .resolve_authorized_resource(authority, authorized)
        .map_err(|_| LimitedProtocolError::Envelope)?;
    Ok((outcome, resource))
}

#[cfg(unix)]
fn authorize_scoped_wrapper_request(
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
    request: &LimitedRequest,
    operation: CapabilityOperation,
) -> Result<
    (
        CapabilityAuthorizationOutcome,
        crate::service_storage::ResolvedAuthorizedResource,
    ),
    LimitedProtocolError,
> {
    let capability = request
        .capability
        .as_ref()
        .filter(|capability| valid_opaque_id(&capability.capability_id, "cap_"))
        .ok_or(LimitedProtocolError::Envelope)?;
    let project_id = request
        .body
        .project_id
        .as_deref()
        .filter(|value| valid_opaque_id(value, "prj_"))
        .ok_or(LimitedProtocolError::Envelope)?;
    let workspace_id = request
        .body
        .workspace_id
        .as_deref()
        .filter(|value| valid_opaque_id(value, "wsp_"))
        .ok_or(LimitedProtocolError::Envelope)?;
    let mut fingerprint_material = Vec::new();
    fingerprint_material.extend_from_slice(operation.storage_value().as_bytes());
    fingerprint_material.push(0);
    fingerprint_material.extend_from_slice(
        &serde_json::to_vec(&request.body).map_err(|_| LimitedProtocolError::Envelope)?,
    );
    let authorization = CapabilityAuthorizationRequest {
        grant_id: capability.capability_id.clone(),
        presented_secret: decode_base64url(&capability.token)?,
        operation,
        project_id: Some(project_id.to_string()),
        workspace_id: Some(workspace_id.to_string()),
        task_id: None,
        presented_connection_nonce: decode_base64url(&request.connection_nonce)?,
        request_id: request.request_id.clone(),
        request_fingerprint: sha256(&fingerprint_material),
    };
    let outcome = authentication
        .authorize(
            storage.registry(),
            CapabilityChannel::Limited,
            &authorization,
        )
        .map_err(|_| LimitedProtocolError::Envelope)?;
    let authorized = match &outcome {
        CapabilityAuthorizationOutcome::Fresh(capability)
        | CapabilityAuthorizationOutcome::Replay(capability)
            if capability.actor() == CapabilityActor::Wrapper
                && capability.operation() == operation =>
        {
            capability
        }
        _ => return Err(LimitedProtocolError::Envelope),
    };
    let resource = storage
        .resolve_authorized_resource(authority, authorized)
        .map_err(|_| LimitedProtocolError::Envelope)?;
    Ok((outcome, resource))
}

fn validate_request_envelope(
    request: &LimitedRequest,
    expected_connection_nonce: &[u8; 32],
) -> Result<(), LimitedProtocolError> {
    if request.message_type != "request"
        || request.wire_version != LIMITED_WIRE_VERSION
        || !valid_opaque_id(&request.request_id, "req_")
        || decode_base64url(&request.connection_nonce)? != *expected_connection_nonce
    {
        return Err(LimitedProtocolError::Envelope);
    }
    Ok(())
}

fn bundle_from_bootstrap(result: ProvisionalBootstrapResult) -> LimitedCapabilityBundle {
    LimitedCapabilityBundle {
        contract_version: "limited-capability-v1".to_string(),
        project_id: result.project_id,
        workspace_id: result.workspace_id,
        compile: WireCapability {
            capability_id: result.compile.capability_id,
            token: encode_base64url(&result.compile.token),
        },
        projection: WireCapability {
            capability_id: result.projection.capability_id,
            token: encode_base64url(&result.projection.token),
        },
    }
}

fn valid_capability_bundle(bundle: &LimitedCapabilityBundle) -> bool {
    bundle.contract_version == "limited-capability-v1"
        && valid_opaque_id(&bundle.project_id, "prj_")
        && valid_opaque_id(&bundle.workspace_id, "wsp_")
        && valid_wire_capability(&bundle.compile)
        && valid_wire_capability(&bundle.projection)
}

fn valid_wire_capability(capability: &WireCapability) -> bool {
    valid_opaque_id(&capability.capability_id, "cap_") && is_encoded_nonce(&capability.token)
}

pub(crate) fn valid_opaque_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|token| {
        token.len() == 26
            && token.bytes().all(|byte| {
                matches!(byte, b'0'..=b'9' | b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z')
            })
    })
}

pub(crate) fn random_request_id() -> Result<String, LimitedProtocolError> {
    random_crockford_id("req_").map_err(|_| LimitedProtocolError::Entropy)
}

fn trusted_unix_time() -> Result<i64, LimitedProtocolError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LimitedProtocolError::Envelope)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| LimitedProtocolError::Envelope)
}

fn lowercase_hex(input: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(input.len() * 2);
    for byte in input {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn validate_client_hello(hello: &ReceivedClientHello) -> Result<(), LimitedProtocolError> {
    let versions_are_valid = !hello.wire_versions.is_empty()
        && hello.wire_versions.len() <= 8
        && hello
            .wire_versions
            .iter()
            .all(|version| !version.is_empty() && version.len() <= 32)
        && hello
            .wire_versions
            .iter()
            .enumerate()
            .all(|(index, version)| !hello.wire_versions[..index].contains(version))
        && hello
            .wire_versions
            .iter()
            .any(|version| version == LIMITED_WIRE_VERSION);
    if hello.message_type != "client_hello"
        || hello.channel != "limited"
        || !versions_are_valid
        || !is_encoded_nonce(&hello.client_nonce)
    {
        return Err(LimitedProtocolError::Envelope);
    }
    Ok(())
}

fn validate_server_hello(hello: &ServerHello) -> Result<(), LimitedProtocolError> {
    if hello.message_type != "server_hello"
        || hello.wire_version != LIMITED_WIRE_VERSION
        || hello.max_frame_bytes != MAX_FRAME_BYTES
        || !is_encoded_nonce(&hello.connection_nonce)
    {
        return Err(LimitedProtocolError::Envelope);
    }
    Ok(())
}

#[cfg(test)]
fn write_frame<W: Write, T: Serialize>(
    writer: &mut W,
    envelope: &T,
) -> Result<(), LimitedProtocolError> {
    let frame = encode_frame(envelope)?;
    writer
        .write_all(&frame)
        .map_err(|_| LimitedProtocolError::Write)
}

fn encode_frame<T: Serialize>(envelope: &T) -> Result<Vec<u8>, LimitedProtocolError> {
    let payload = serde_json::to_vec(envelope).map_err(|_| LimitedProtocolError::Envelope)?;
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(LimitedProtocolError::Framing);
    }
    let length = u32::try_from(payload.len()).map_err(|_| LimitedProtocolError::Framing)?;
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&LIMITED_MAGIC);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

#[cfg(test)]
fn read_frame<R: Read>(reader: &mut R) -> Result<Vec<u8>, LimitedProtocolError> {
    let mut header = [0_u8; 8];
    reader
        .read_exact(&mut header)
        .map_err(|_| LimitedProtocolError::Read)?;
    if header[..4] != LIMITED_MAGIC {
        return Err(LimitedProtocolError::Framing);
    }
    let length = u32::from_be_bytes(
        header[4..]
            .try_into()
            .map_err(|_| LimitedProtocolError::Framing)?,
    ) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(LimitedProtocolError::Framing);
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .map_err(|_| LimitedProtocolError::Read)?;
    Ok(payload)
}

fn random_nonce() -> Result<String, LimitedProtocolError> {
    let mut bytes = [0_u8; 32];
    fill_system_random(&mut bytes).map_err(|_| LimitedProtocolError::Entropy)?;
    if bytes.iter().all(|byte| *byte == 0) {
        return Err(LimitedProtocolError::Entropy);
    }
    Ok(encode_base64url(&bytes))
}

pub(crate) fn encode_base64url(input: &[u8; 32]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(43);
    let mut index = 0;
    while index + 3 <= input.len() {
        let chunk = ((input[index] as u32) << 16)
            | ((input[index + 1] as u32) << 8)
            | input[index + 2] as u32;
        output.push(ALPHABET[((chunk >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((chunk >> 12) & 63) as usize] as char);
        output.push(ALPHABET[((chunk >> 6) & 63) as usize] as char);
        output.push(ALPHABET[(chunk & 63) as usize] as char);
        index += 3;
    }
    let remaining = ((input[index] as u16) << 8) | input[index + 1] as u16;
    output.push(ALPHABET[((remaining >> 10) & 63) as usize] as char);
    output.push(ALPHABET[((remaining >> 4) & 63) as usize] as char);
    output.push(ALPHABET[((remaining << 2) & 63) as usize] as char);
    output
}

pub(crate) fn decode_base64url(value: &str) -> Result<[u8; 32], LimitedProtocolError> {
    if !is_encoded_nonce(value) {
        return Err(LimitedProtocolError::Envelope);
    }
    let mut output = [0_u8; 32];
    let bytes = value.as_bytes();
    let mut input_index = 0;
    let mut output_index = 0;
    while input_index + 4 <= 40 {
        let a = base64url_digit(bytes[input_index])?;
        let b = base64url_digit(bytes[input_index + 1])?;
        let c = base64url_digit(bytes[input_index + 2])?;
        let d = base64url_digit(bytes[input_index + 3])?;
        output[output_index] = (a << 2) | (b >> 4);
        output[output_index + 1] = (b << 4) | (c >> 2);
        output[output_index + 2] = (c << 6) | d;
        input_index += 4;
        output_index += 3;
    }
    let a = base64url_digit(bytes[40])?;
    let b = base64url_digit(bytes[41])?;
    let c = base64url_digit(bytes[42])?;
    if c & 0x03 != 0 {
        return Err(LimitedProtocolError::Envelope);
    }
    output[30] = (a << 2) | (b >> 4);
    output[31] = (b << 4) | (c >> 2);
    Ok(output)
}

fn base64url_digit(byte: u8) -> Result<u8, LimitedProtocolError> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'-' => Ok(62),
        b'_' => Ok(63),
        _ => Err(LimitedProtocolError::Envelope),
    }
}

pub(crate) fn is_encoded_nonce(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(unix)]
fn write_stream_frame<T: Serialize>(
    stream: &mut UnixStream,
    envelope: &T,
) -> Result<(), LimitedProtocolError> {
    let frame = encode_frame(envelope)?;
    write_all_before(
        stream,
        &frame,
        std::time::Instant::now() + HANDSHAKE_DEADLINE,
    )
}

#[cfg(unix)]
fn read_stream_frame(stream: &mut UnixStream) -> Result<Vec<u8>, LimitedProtocolError> {
    read_stream_frame_with_timeout(stream, HANDSHAKE_DEADLINE)
}

#[cfg(unix)]
fn read_stream_frame_with_timeout(
    stream: &mut UnixStream,
    timeout: Duration,
) -> Result<Vec<u8>, LimitedProtocolError> {
    let deadline = std::time::Instant::now() + timeout;
    let mut header = [0_u8; 8];
    read_exact_before(stream, &mut header, deadline)?;
    if header[..4] != LIMITED_MAGIC {
        return Err(LimitedProtocolError::Framing);
    }
    let length = u32::from_be_bytes(
        header[4..]
            .try_into()
            .map_err(|_| LimitedProtocolError::Framing)?,
    ) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(LimitedProtocolError::Framing);
    }
    let mut payload = vec![0_u8; length];
    read_exact_before(stream, &mut payload, deadline)?;
    Ok(payload)
}

#[cfg(unix)]
fn read_exact_before(
    stream: &mut UnixStream,
    mut output: &mut [u8],
    deadline: std::time::Instant,
) -> Result<(), LimitedProtocolError> {
    while !output.is_empty() {
        wait_for(stream.as_raw_fd(), libc::POLLIN, deadline)?;
        match stream.read(output) {
            Ok(0) => return Err(LimitedProtocolError::Read),
            Ok(count) => output = &mut output[count..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(LimitedProtocolError::Read),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn write_all_before(
    stream: &mut UnixStream,
    mut input: &[u8],
    deadline: std::time::Instant,
) -> Result<(), LimitedProtocolError> {
    while !input.is_empty() {
        wait_for(stream.as_raw_fd(), libc::POLLOUT, deadline)?;
        match stream.write(input) {
            Ok(0) => return Err(LimitedProtocolError::Write),
            Ok(count) => input = &input[count..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(LimitedProtocolError::Write),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn wait_for(
    raw_fd: i32,
    events: libc::c_short,
    deadline: std::time::Instant,
) -> Result<(), LimitedProtocolError> {
    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or(LimitedProtocolError::Deadline)?;
        let timeout_ms = i32::try_from(remaining.as_millis().max(1)).unwrap_or(i32::MAX);
        let mut descriptor = libc::pollfd {
            fd: raw_fd,
            events,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        if result == 0 {
            return Err(LimitedProtocolError::Deadline);
        }
        if result < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(LimitedProtocolError::Transport);
        }
        if descriptor.revents & (events | libc::POLLHUP) != 0 {
            return Ok(());
        }
        return Err(LimitedProtocolError::Transport);
    }
}

#[cfg(test)]
#[path = "tests/Limited_Protocol_test.rs"]
mod limited_protocol_tests;
