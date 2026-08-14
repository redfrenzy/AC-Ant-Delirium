#[cfg(unix)]
use std::ffi::OsStr;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::time::Duration;

use ac_ant_delirium::libres::System_Random_unit::fill_system_random;
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use crate::service_hardening::set_descriptor_close_on_exec;

const LIMITED_MAGIC: [u8; 4] = *b"ADL1";
const LIMITED_WIRE_VERSION: &str = "adp/1";
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const HANDSHAKE_DEADLINE: Duration = Duration::from_millis(500);

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

fn encode_base64url(input: &[u8; 32]) -> String {
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

fn is_encoded_nonce(value: &str) -> bool {
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
    let deadline = std::time::Instant::now() + HANDSHAKE_DEADLINE;
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
