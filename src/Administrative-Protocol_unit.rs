use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ac_ant_delirium::libres::Sha2_unit::sha256;
use ac_ant_delirium::libres::System_Random_unit::fill_system_random;
use ac_ant_delirium::structs::{
    CapabilityActor, CapabilityAuthorizationOutcome, CapabilityAuthorizationRequest,
    CapabilityChannel, CapabilityOperation,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::legacy_normalization::{LegacyNormalizationReceipt, LegacyNormalizationRequest};
use crate::legacy_staging::LegacyStageReceipt;
use crate::limited_protocol::{
    decode_base64url, encode_base64url, is_encoded_nonce, random_request_id, valid_opaque_id,
};
use crate::public_output::{OutputRouter, PublicError};
use crate::service_authentication::ConnectionAuthentication;
use crate::service_authority::VerifiedServiceAuthority;
use crate::service_hardening::set_descriptor_close_on_exec;
use crate::service_storage::ProtectedServiceStorage;

const ADMIN_MAGIC: [u8; 4] = *b"ADA1";
const ADMIN_WIRE_VERSION: &str = "ada/1";
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_INVENTORY_BYTES: u64 = 8 * 1024 * 1024;
const ADMIN_TIMEOUT: Duration = Duration::from_secs(120);
const MIGRATION_OPERATION_TIMEOUT: Duration = Duration::from_secs(600);
const ADMIN_SOCKET_ENV: &str = "ANTDELIRIUM_ADMIN_SOCKET";
const ADMIN_CAPABILITY_FD_ENV: &str = "ANTDELIRIUM_ADMIN_CAPABILITY_FD";
const MIGRATION_SOURCE_ENV: &str = "ANTDELIRIUM_MIGRATION_SOURCE";
const MIGRATION_INVENTORY_ENV: &str = "ANTDELIRIUM_MIGRATION_INVENTORY";
const EXPECTED_SOURCE_SHA256_ENV: &str = "ANTDELIRIUM_EXPECTED_SOURCE_SHA256";
const MIGRATION_ID_ENV: &str = "ANTDELIRIUM_MIGRATION_ID";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdministrativeProtocolError {
    Endpoint,
    Transport(&'static str),
    Framing,
    Envelope,
    Authorization,
    Staging,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientHello {
    message_type: String,
    channel: String,
    wire_versions: Vec<String>,
    client_nonce: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerHello {
    message_type: String,
    wire_version: String,
    connection_nonce: String,
    max_frame_bytes: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCapability {
    capability_id: String,
    token: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdministrativeCapabilityBundle {
    contract_version: String,
    capability: WireCapability,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationStageBody {
    expected_source_snapshot_sha256: String,
    inventory: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum AdministrativeBody {
    Stage(MigrationStageBody),
    Normalize(LegacyNormalizationRequest),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdministrativeRequest {
    message_type: String,
    wire_version: String,
    request_id: String,
    connection_nonce: String,
    operation: String,
    capability: WireCapability,
    body: AdministrativeBody,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum AdministrativeResult {
    Stage(LegacyStageReceipt),
    Normalize(LegacyNormalizationReceipt),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdministrativeResponse {
    message_type: String,
    wire_version: String,
    request_id: String,
    status: String,
    result: Option<AdministrativeResult>,
    error: Option<Value>,
}

pub(crate) fn run_migration_operator(args: &[OsString]) {
    let normalize = args.len() == 2
        && args[0] == OsStr::new("migrate-plugin")
        && args[1] == OsStr::new("normalize");
    if !(args.len() == 1 && args[0] == OsStr::new("migrate-plugin")) && !normalize {
        OutputRouter::exit(PublicError::Authorization);
    }
    if cfg!(debug_assertions) {
        OutputRouter::exit(PublicError::Configuration);
    }
    let result = if normalize {
        run_operator_normalize_inner().map(AdministrativeResult::Normalize)
    } else {
        run_operator_stage_inner().map(AdministrativeResult::Stage)
    };
    match result {
        Ok(AdministrativeResult::Stage(receipt)) => OutputRouter::write_success(&format!(
                "antdelirium[AD-MIGRATION-STAGED]: migration_id={} project_id={} source_sha256={} staging_sha256={} state=staged",
                receipt.migration_id,
                receipt.project_id,
                receipt.source_snapshot_sha256,
                receipt.staging_snapshot_sha256,
            )),
        Ok(AdministrativeResult::Normalize(receipt)) => OutputRouter::write_success(&format!(
                "antdelirium[AD-MIGRATION-NORMALIZED]: migration_id={} project_id={} semantic_sha256={} comparison=equivalent state=normalized",
                receipt.migration_id,
                receipt.project_id,
                receipt.semantic_snapshot_sha256,
            )),
        Err(AdministrativeProtocolError::Authorization) => {
            OutputRouter::exit(PublicError::Authorization)
        }
        Err(AdministrativeProtocolError::Endpoint | AdministrativeProtocolError::Transport(_)) => {
            OutputRouter::exit(PublicError::ServiceUnavailable)
        }
        Err(
            AdministrativeProtocolError::Framing | AdministrativeProtocolError::Envelope,
        ) => OutputRouter::exit(PublicError::Protocol),
        Err(AdministrativeProtocolError::Staging) => {
            OutputRouter::exit(PublicError::Configuration)
        }
    }
}

#[cfg(unix)]
fn run_operator_stage_inner() -> Result<LegacyStageReceipt, AdministrativeProtocolError> {
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let source = std::env::var_os(MIGRATION_SOURCE_ENV);
    let inventory_path = std::env::var_os(MIGRATION_INVENTORY_ENV);
    let expected = std::env::var(EXPECTED_SOURCE_SHA256_ENV).ok();
    clear_operator_path_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let source = required_absolute_path(source)?;
    let inventory_path = required_absolute_path(inventory_path)?;
    let expected = expected.ok_or(AdministrativeProtocolError::Envelope)?;
    let capability = read_administrative_capability()?;
    let inventory = read_bounded_regular_file(&inventory_path, MAX_INVENTORY_BYTES)?;
    let inventory_value: Value =
        serde_json::from_slice(&inventory).map_err(|_| AdministrativeProtocolError::Envelope)?;

    let mut stream = UnixStream::connect(endpoint)
        .map_err(|_| AdministrativeProtocolError::Transport("connect"))?;
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Transport("close-on-exec"))?;
    let connection_nonce = negotiate_client(&mut stream)?;
    let request_id = random_request_id().map_err(|_| AdministrativeProtocolError::Envelope)?;
    let request = AdministrativeRequest {
        message_type: "request".to_string(),
        wire_version: ADMIN_WIRE_VERSION.to_string(),
        request_id: request_id.clone(),
        connection_nonce,
        operation: "migration.stage".to_string(),
        capability: capability.capability,
        body: AdministrativeBody::Stage(MigrationStageBody {
            expected_source_snapshot_sha256: expected.clone(),
            inventory: inventory_value,
        }),
    };
    write_frame(&mut stream, &request)?;
    {
        let mut writer = DeadlineWriter::new(&mut stream, MIGRATION_OPERATION_TIMEOUT);
        crate::legacy_staging::write_validated_source_stream(
            &source,
            &inventory,
            &expected,
            &mut writer,
        )
        .map_err(|_| AdministrativeProtocolError::Staging)?;
        writer
            .flush()
            .map_err(|_| AdministrativeProtocolError::Transport("source-flush"))?;
    }
    let response: AdministrativeResponse = serde_json::from_slice(&read_frame_with_timeout(
        &mut stream,
        MIGRATION_OPERATION_TIMEOUT,
    )?)
    .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match response.result {
        Some(AdministrativeResult::Stage(receipt)) => Ok(receipt),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(not(unix))]
fn run_operator_stage_inner() -> Result<LegacyStageReceipt, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(unix)]
fn run_operator_normalize_inner() -> Result<LegacyNormalizationReceipt, AdministrativeProtocolError>
{
    let endpoint = std::env::var_os(ADMIN_SOCKET_ENV);
    let source = std::env::var_os(MIGRATION_SOURCE_ENV);
    let inventory_path = std::env::var_os(MIGRATION_INVENTORY_ENV);
    let expected = std::env::var(EXPECTED_SOURCE_SHA256_ENV).ok();
    let migration_id = std::env::var(MIGRATION_ID_ENV).ok();
    clear_operator_path_environment();
    let endpoint = required_absolute_path(endpoint)?;
    let source = required_absolute_path(source)?;
    let inventory_path = required_absolute_path(inventory_path)?;
    let expected = expected.ok_or(AdministrativeProtocolError::Envelope)?;
    let migration_id = migration_id.ok_or(AdministrativeProtocolError::Envelope)?;
    let capability = read_administrative_capability()?;
    let inventory = read_bounded_regular_file(&inventory_path, MAX_INVENTORY_BYTES)?;
    let normalization = crate::legacy_normalization::prepare_source_normalization(
        &source,
        &inventory,
        &expected,
        &migration_id,
    )
    .map_err(|_| AdministrativeProtocolError::Staging)?;

    let mut stream = UnixStream::connect(endpoint)
        .map_err(|_| AdministrativeProtocolError::Transport("connect"))?;
    set_descriptor_close_on_exec(stream.as_raw_fd())
        .map_err(|_| AdministrativeProtocolError::Transport("close-on-exec"))?;
    let connection_nonce = negotiate_client(&mut stream)?;
    let request_id = random_request_id().map_err(|_| AdministrativeProtocolError::Envelope)?;
    write_frame(
        &mut stream,
        &AdministrativeRequest {
            message_type: "request".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request_id.clone(),
            connection_nonce,
            operation: "migration.normalize".to_string(),
            capability: capability.capability,
            body: AdministrativeBody::Normalize(normalization),
        },
    )?;
    let response: AdministrativeResponse = serde_json::from_slice(&read_frame_with_timeout(
        &mut stream,
        MIGRATION_OPERATION_TIMEOUT,
    )?)
    .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if response.message_type != "response"
        || response.wire_version != ADMIN_WIRE_VERSION
        || response.request_id != request_id
        || response.status != "ok"
        || response.error.is_some()
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match response.result {
        Some(AdministrativeResult::Normalize(receipt)) => Ok(receipt),
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(not(unix))]
fn run_operator_normalize_inner() -> Result<LegacyNormalizationReceipt, AdministrativeProtocolError>
{
    Err(AdministrativeProtocolError::Transport(
        "unsupported-platform",
    ))
}

#[cfg(unix)]
pub(crate) fn answer_administrative_hello(
    stream: &mut UnixStream,
    connection_nonce: &[u8; 32],
) -> Result<(), AdministrativeProtocolError> {
    configure_nonblocking(stream)?;
    let hello: ClientHello = serde_json::from_slice(&read_frame(stream)?)
        .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if hello.message_type != "client_hello"
        || hello.channel != "admin"
        || hello.wire_versions != [ADMIN_WIRE_VERSION]
        || !is_encoded_nonce(&hello.client_nonce)
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    write_frame(
        stream,
        &ServerHello {
            message_type: "server_hello".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            connection_nonce: encode_base64url(connection_nonce),
            max_frame_bytes: MAX_FRAME_BYTES,
        },
    )
}

#[cfg(unix)]
pub(crate) fn serve_administrative_request(
    stream: &mut UnixStream,
    authentication: &ConnectionAuthentication,
    authority: &VerifiedServiceAuthority,
    storage: &ProtectedServiceStorage,
) -> Result<(), AdministrativeProtocolError> {
    let payload = read_frame(stream)?;
    let request: AdministrativeRequest =
        serde_json::from_slice(&payload).map_err(|_| AdministrativeProtocolError::Envelope)?;
    validate_request(&request, authentication.connection_nonce())?;
    let authorization = capability_request(authentication, &request)?;
    let outcome = authentication
        .authorize(
            storage.registry(),
            CapabilityChannel::Administrative,
            &authorization,
        )
        .map_err(|_| AdministrativeProtocolError::Authorization)?;
    let capability = match outcome {
        CapabilityAuthorizationOutcome::Fresh(capability)
            if capability.actor() == CapabilityActor::Operator
                && capability.operation() == CapabilityOperation::StateAdmin =>
        {
            capability
        }
        _ => return Err(AdministrativeProtocolError::Authorization),
    };
    let result = match request.body {
        AdministrativeBody::Stage(body) => {
            let inventory_document = serde_json::to_vec_pretty(&body.inventory)
                .map_err(|_| AdministrativeProtocolError::Envelope)?;
            let mut reader = DeadlineReader::new(stream, MIGRATION_OPERATION_TIMEOUT);
            let receipt = storage
                .stage_legacy_stream(
                    authority,
                    &capability,
                    &inventory_document,
                    &body.expected_source_snapshot_sha256,
                    &mut reader,
                )
                .map_err(|_| AdministrativeProtocolError::Staging)?;
            AdministrativeResult::Stage(receipt)
        }
        AdministrativeBody::Normalize(body) => {
            let receipt = storage
                .normalize_legacy_migration(authority, &capability, &body)
                .map_err(|_| AdministrativeProtocolError::Staging)?;
            AdministrativeResult::Normalize(receipt)
        }
    };
    write_frame(
        stream,
        &AdministrativeResponse {
            message_type: "response".to_string(),
            wire_version: ADMIN_WIRE_VERSION.to_string(),
            request_id: request.request_id,
            status: "ok".to_string(),
            result: Some(result),
            error: None,
        },
    )
}

fn capability_request(
    authentication: &ConnectionAuthentication,
    request: &AdministrativeRequest,
) -> Result<CapabilityAuthorizationRequest, AdministrativeProtocolError> {
    if !valid_opaque_id(&request.capability.capability_id, "cap_") {
        return Err(AdministrativeProtocolError::Authorization);
    }
    let mut fingerprint = request.operation.as_bytes().to_vec();
    fingerprint.push(0);
    fingerprint.extend_from_slice(
        &serde_json::to_vec(&request.body).map_err(|_| AdministrativeProtocolError::Envelope)?,
    );
    Ok(CapabilityAuthorizationRequest {
        grant_id: request.capability.capability_id.clone(),
        presented_secret: decode_base64url(&request.capability.token)
            .map_err(|_| AdministrativeProtocolError::Authorization)?,
        operation: CapabilityOperation::StateAdmin,
        project_id: None,
        workspace_id: None,
        task_id: None,
        presented_connection_nonce: *authentication.connection_nonce(),
        request_id: request.request_id.clone(),
        request_fingerprint: sha256(&fingerprint),
    })
}

fn validate_request(
    request: &AdministrativeRequest,
    expected_nonce: &[u8; 32],
) -> Result<(), AdministrativeProtocolError> {
    if request.message_type != "request"
        || request.wire_version != ADMIN_WIRE_VERSION
        || !matches!(
            request.operation.as_str(),
            "migration.stage" | "migration.normalize"
        )
        || !valid_opaque_id(&request.request_id, "req_")
        || decode_base64url(&request.connection_nonce)
            .map_err(|_| AdministrativeProtocolError::Envelope)?
            != *expected_nonce
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    match (&request.operation[..], &request.body) {
        ("migration.stage", AdministrativeBody::Stage(body))
            if body.inventory["inventory_sha256"].as_str()
                == Some(&body.expected_source_snapshot_sha256) =>
        {
            Ok(())
        }
        ("migration.normalize", AdministrativeBody::Normalize(body))
            if body.contract_version == "legacy-normalization-request-v1" =>
        {
            Ok(())
        }
        _ => Err(AdministrativeProtocolError::Envelope),
    }
}

#[cfg(unix)]
fn negotiate_client(stream: &mut UnixStream) -> Result<String, AdministrativeProtocolError> {
    configure_nonblocking(stream)?;
    let mut nonce = [0_u8; 32];
    fill_system_random(&mut nonce)
        .map_err(|_| AdministrativeProtocolError::Transport("client-entropy"))?;
    if nonce.iter().all(|byte| *byte == 0) {
        return Err(AdministrativeProtocolError::Transport(
            "client-entropy-zero",
        ));
    }
    write_frame(
        stream,
        &ClientHello {
            message_type: "client_hello".to_string(),
            channel: "admin".to_string(),
            wire_versions: vec![ADMIN_WIRE_VERSION.to_string()],
            client_nonce: encode_base64url(&nonce),
        },
    )?;
    let hello: ServerHello = serde_json::from_slice(&read_frame(stream)?)
        .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if hello.message_type != "server_hello"
        || hello.wire_version != ADMIN_WIRE_VERSION
        || hello.max_frame_bytes != MAX_FRAME_BYTES
        || !is_encoded_nonce(&hello.connection_nonce)
    {
        return Err(AdministrativeProtocolError::Envelope);
    }
    Ok(hello.connection_nonce)
}

#[cfg(unix)]
fn write_frame<T: Serialize>(
    stream: &mut UnixStream,
    value: &T,
) -> Result<(), AdministrativeProtocolError> {
    let payload = serde_json::to_vec(value).map_err(|_| AdministrativeProtocolError::Envelope)?;
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(AdministrativeProtocolError::Framing);
    }
    let length = u32::try_from(payload.len()).map_err(|_| AdministrativeProtocolError::Framing)?;
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&ADMIN_MAGIC);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    write_all_before(stream, &frame, Instant::now() + ADMIN_TIMEOUT)
}

#[cfg(unix)]
fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>, AdministrativeProtocolError> {
    read_frame_with_timeout(stream, ADMIN_TIMEOUT)
}

#[cfg(unix)]
fn read_frame_with_timeout(
    stream: &mut UnixStream,
    timeout: Duration,
) -> Result<Vec<u8>, AdministrativeProtocolError> {
    let deadline = Instant::now() + timeout;
    let mut header = [0_u8; 8];
    read_exact_before(stream, &mut header, deadline)?;
    if header[..4] != ADMIN_MAGIC {
        return Err(AdministrativeProtocolError::Framing);
    }
    let size = u32::from_be_bytes(
        header[4..]
            .try_into()
            .map_err(|_| AdministrativeProtocolError::Framing)?,
    ) as usize;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err(AdministrativeProtocolError::Framing);
    }
    let mut payload = vec![0_u8; size];
    read_exact_before(stream, &mut payload, deadline)?;
    Ok(payload)
}

#[cfg(unix)]
fn configure_nonblocking(stream: &UnixStream) -> Result<(), AdministrativeProtocolError> {
    stream
        .set_nonblocking(true)
        .map_err(|_| AdministrativeProtocolError::Transport("socket-nonblocking"))
}

#[cfg(unix)]
fn read_exact_before(
    stream: &mut UnixStream,
    mut output: &mut [u8],
    deadline: Instant,
) -> Result<(), AdministrativeProtocolError> {
    while !output.is_empty() {
        wait_for(stream.as_raw_fd(), libc::POLLIN, deadline)?;
        let count =
            unsafe { libc::read(stream.as_raw_fd(), output.as_mut_ptr().cast(), output.len()) };
        if count == 0 {
            return Err(AdministrativeProtocolError::Transport("stream-eof"));
        }
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            return Err(AdministrativeProtocolError::Transport("stream-read"));
        }
        output = &mut output[count as usize..];
    }
    Ok(())
}

#[cfg(unix)]
fn write_all_before(
    stream: &mut UnixStream,
    mut input: &[u8],
    deadline: Instant,
) -> Result<(), AdministrativeProtocolError> {
    while !input.is_empty() {
        wait_for(stream.as_raw_fd(), libc::POLLOUT, deadline)?;
        let chunk = &input[..input.len().min(64 * 1024)];
        let count = unsafe { libc::write(stream.as_raw_fd(), chunk.as_ptr().cast(), chunk.len()) };
        if count == 0 {
            return Err(AdministrativeProtocolError::Transport("stream-write-zero"));
        }
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            return Err(AdministrativeProtocolError::Transport("stream-write"));
        }
        input = &input[count as usize..];
    }
    Ok(())
}

#[cfg(unix)]
fn wait_for(
    raw_fd: i32,
    events: libc::c_short,
    deadline: Instant,
) -> Result<(), AdministrativeProtocolError> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(AdministrativeProtocolError::Transport("deadline"))?;
        let timeout_ms = i32::try_from(remaining.as_millis().max(1)).unwrap_or(i32::MAX);
        let mut descriptor = libc::pollfd {
            fd: raw_fd,
            events,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        if result == 0 {
            return Err(AdministrativeProtocolError::Transport("deadline"));
        }
        if result < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(AdministrativeProtocolError::Transport("poll"));
        }
        if descriptor.revents & (events | libc::POLLHUP) != 0 {
            return Ok(());
        }
        return Err(AdministrativeProtocolError::Transport("poll-events"));
    }
}

#[cfg(unix)]
struct DeadlineReader<'a> {
    stream: &'a mut UnixStream,
    deadline: Instant,
}

#[cfg(unix)]
impl<'a> DeadlineReader<'a> {
    fn new(stream: &'a mut UnixStream, timeout: Duration) -> Self {
        Self {
            stream,
            deadline: Instant::now() + timeout,
        }
    }
}

#[cfg(unix)]
impl Read for DeadlineReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        loop {
            wait_for(self.stream.as_raw_fd(), libc::POLLIN, self.deadline).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "admin read deadline")
            })?;
            let count = unsafe {
                libc::read(
                    self.stream.as_raw_fd(),
                    output.as_mut_ptr().cast(),
                    output.len(),
                )
            };
            if count >= 0 {
                return Ok(count as usize);
            }
            let error = std::io::Error::last_os_error();
            if !matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                return Err(error);
            }
        }
    }
}

#[cfg(unix)]
struct DeadlineWriter<'a> {
    stream: &'a mut UnixStream,
    deadline: Instant,
}

#[cfg(unix)]
impl<'a> DeadlineWriter<'a> {
    fn new(stream: &'a mut UnixStream, timeout: Duration) -> Self {
        Self {
            stream,
            deadline: Instant::now() + timeout,
        }
    }
}

#[cfg(unix)]
impl Write for DeadlineWriter<'_> {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        loop {
            wait_for(self.stream.as_raw_fd(), libc::POLLOUT, self.deadline).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "admin write deadline")
            })?;
            let chunk = &input[..input.len().min(64 * 1024)];
            let count =
                unsafe { libc::write(self.stream.as_raw_fd(), chunk.as_ptr().cast(), chunk.len()) };
            if count >= 0 {
                return Ok(count as usize);
            }
            let error = std::io::Error::last_os_error();
            if !matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                return Err(error);
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

fn required_absolute_path(value: Option<OsString>) -> Result<PathBuf, AdministrativeProtocolError> {
    let value = value.ok_or(AdministrativeProtocolError::Endpoint)?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(AdministrativeProtocolError::Endpoint);
    }
    Ok(path)
}

fn clear_operator_path_environment() {
    for name in [
        ADMIN_SOCKET_ENV,
        MIGRATION_SOURCE_ENV,
        MIGRATION_INVENTORY_ENV,
        EXPECTED_SOURCE_SHA256_ENV,
        MIGRATION_ID_ENV,
    ] {
        std::env::remove_var(name);
    }
}

fn read_bounded_regular_file(
    path: &Path,
    maximum: u64,
) -> Result<Vec<u8>, AdministrativeProtocolError> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| AdministrativeProtocolError::Envelope)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > maximum {
        return Err(AdministrativeProtocolError::Envelope);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(path)
        .map_err(|_| AdministrativeProtocolError::Envelope)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| AdministrativeProtocolError::Envelope)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(AdministrativeProtocolError::Envelope);
    }
    Ok(bytes)
}

#[cfg(unix)]
fn read_administrative_capability(
) -> Result<AdministrativeCapabilityBundle, AdministrativeProtocolError> {
    let raw = std::env::var(ADMIN_CAPABILITY_FD_ENV)
        .map_err(|_| AdministrativeProtocolError::Authorization)?;
    std::env::remove_var(ADMIN_CAPABILITY_FD_ENV);
    let raw_fd = raw
        .parse::<i32>()
        .ok()
        .filter(|fd| *fd >= 3 && unsafe { libc::fcntl(*fd, libc::F_GETFD) } >= 0)
        .ok_or(AdministrativeProtocolError::Authorization)?;
    set_descriptor_close_on_exec(raw_fd).map_err(|_| AdministrativeProtocolError::Authorization)?;
    let mut file = unsafe { std::fs::File::from_raw_fd(raw_fd) };
    let mut bytes = Vec::with_capacity(512);
    std::io::Read::by_ref(&mut file)
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| AdministrativeProtocolError::Authorization)?;
    if bytes.is_empty() || bytes.len() > 4096 {
        return Err(AdministrativeProtocolError::Authorization);
    }
    let capability: AdministrativeCapabilityBundle =
        serde_json::from_slice(&bytes).map_err(|_| AdministrativeProtocolError::Authorization)?;
    if capability.contract_version != "admin-capability-v1"
        || !valid_opaque_id(&capability.capability.capability_id, "cap_")
        || !is_encoded_nonce(&capability.capability.token)
    {
        return Err(AdministrativeProtocolError::Authorization);
    }
    Ok(capability)
}

#[cfg(not(unix))]
fn read_administrative_capability(
) -> Result<AdministrativeCapabilityBundle, AdministrativeProtocolError> {
    Err(AdministrativeProtocolError::Authorization)
}

#[cfg(test)]
#[path = "tests/Administrative_Protocol_test.rs"]
mod administrative_protocol_tests;
