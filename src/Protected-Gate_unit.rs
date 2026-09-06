use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use ring::signature::{UnparsedPublicKey, ED25519};
use serde::Deserialize;
use serde_json::Value;

const INSTALLED_PUBLIC_KEY: &str = "/usr/lib/antdelirium/protected-gate-public.der";
const PUBLIC_KEY_DER_PREFIX: &[u8] = &[
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];
const PUBLIC_KEY_BYTES: usize = 32;
const SIGNATURE_BYTES: usize = 64;
const MAX_ENVELOPE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtectedGateBinding<'a> {
    pub(crate) project_id: &'a str,
    pub(crate) workspace_id: &'a str,
    pub(crate) task_id: &'a str,
    pub(crate) task_revision: i64,
    pub(crate) state_revision: i64,
    pub(crate) review_id: &'a str,
    pub(crate) source_fingerprint: &'a str,
    pub(crate) wrapper_sha256: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtectedGateError {
    MissingAuthority,
    InvalidAuthority,
    InvalidEnvelope,
    InvalidSignature,
    Blocked,
    BindingMismatch,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedProtectedTaskGate {
    contract_version: String,
    attestation: ProtectedTaskGateAttestation,
    signature_base64: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtectedTaskGateAttestation {
    contract_version: String,
    decision: String,
    project_id: String,
    workspace_id: String,
    task_id: String,
    task_revision: i64,
    state_revision: i64,
    review_id: String,
    source_fingerprint: String,
    wrapper_sha256: String,
    policy_sha256: String,
    mechanical_evidence_sha256: String,
    integrity_envelope_sha256: String,
    checkpoint_epoch: i64,
    blocking_violations: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct ProtectedGateVerifier {
    public_key: [u8; PUBLIC_KEY_BYTES],
    trusted_wrapper_sha256: String,
}

impl ProtectedGateVerifier {
    pub(crate) fn from_installed_authority() -> Result<Self, ProtectedGateError> {
        let mut verifier = Self::from_der_path(Path::new(INSTALLED_PUBLIC_KEY))?;
        verifier.trusted_wrapper_sha256 = running_binary_sha256()?;
        Ok(verifier)
    }

    fn from_der_path(path: &Path) -> Result<Self, ProtectedGateError> {
        let metadata =
            fs::symlink_metadata(path).map_err(|_| ProtectedGateError::MissingAuthority)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(ProtectedGateError::InvalidAuthority);
        }
        #[cfg(unix)]
        if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(ProtectedGateError::InvalidAuthority);
        }
        let der = fs::read(path).map_err(|_| ProtectedGateError::InvalidAuthority)?;
        Self::from_subject_public_key_info(&der)
    }

    fn from_subject_public_key_info(der: &[u8]) -> Result<Self, ProtectedGateError> {
        if der.len() != PUBLIC_KEY_DER_PREFIX.len() + PUBLIC_KEY_BYTES
            || !der.starts_with(PUBLIC_KEY_DER_PREFIX)
        {
            return Err(ProtectedGateError::InvalidAuthority);
        }
        let public_key = der[PUBLIC_KEY_DER_PREFIX.len()..]
            .try_into()
            .map_err(|_| ProtectedGateError::InvalidAuthority)?;
        Ok(Self {
            public_key,
            trusted_wrapper_sha256: String::new(),
        })
    }

    #[cfg(test)]
    pub(crate) fn from_raw_public_key(
        public_key: [u8; PUBLIC_KEY_BYTES],
        trusted_wrapper_sha256: String,
    ) -> Self {
        Self {
            public_key,
            trusted_wrapper_sha256,
        }
    }

    pub(crate) fn trusted_wrapper_sha256(&self) -> &str {
        &self.trusted_wrapper_sha256
    }

    pub(crate) fn verify(
        &self,
        envelope: &[u8],
        binding: &ProtectedGateBinding<'_>,
    ) -> Result<(), ProtectedGateError> {
        if envelope.is_empty() || envelope.len() > MAX_ENVELOPE_BYTES {
            return Err(ProtectedGateError::InvalidEnvelope);
        }
        let value: Value =
            serde_json::from_slice(envelope).map_err(|_| ProtectedGateError::InvalidEnvelope)?;
        let parsed: SignedProtectedTaskGate = serde_json::from_value(value.clone())
            .map_err(|_| ProtectedGateError::InvalidEnvelope)?;
        if parsed.contract_version != "signed-protected-task-gate-attestation-v1"
            || parsed.attestation.contract_version != "protected-task-gate-attestation-v1"
            || parsed.attestation.checkpoint_epoch < 1
            || !valid_sha256(&parsed.attestation.source_fingerprint)
            || !valid_sha256(&parsed.attestation.wrapper_sha256)
            || !valid_sha256(&parsed.attestation.policy_sha256)
            || !valid_sha256(&parsed.attestation.mechanical_evidence_sha256)
            || !valid_sha256(&parsed.attestation.integrity_envelope_sha256)
            || !is_sorted_unique(&parsed.attestation.blocking_violations)
        {
            return Err(ProtectedGateError::InvalidEnvelope);
        }
        let signature = decode_standard_base64(&parsed.signature_base64)
            .ok_or(ProtectedGateError::InvalidSignature)?;
        if signature.len() != SIGNATURE_BYTES {
            return Err(ProtectedGateError::InvalidSignature);
        }
        let attestation_value = value
            .get("attestation")
            .ok_or(ProtectedGateError::InvalidEnvelope)?;
        let mut canonical = serde_json::to_vec(attestation_value)
            .map_err(|_| ProtectedGateError::InvalidEnvelope)?;
        canonical.push(b'\n');
        UnparsedPublicKey::new(&ED25519, self.public_key)
            .verify(&canonical, &signature)
            .map_err(|_| ProtectedGateError::InvalidSignature)?;
        if parsed.attestation.decision != "passed"
            || !parsed.attestation.blocking_violations.is_empty()
        {
            return Err(ProtectedGateError::Blocked);
        }
        if parsed.attestation.project_id != binding.project_id
            || parsed.attestation.workspace_id != binding.workspace_id
            || parsed.attestation.task_id != binding.task_id
            || parsed.attestation.task_revision != binding.task_revision
            || parsed.attestation.state_revision != binding.state_revision
            || parsed.attestation.review_id != binding.review_id
            || parsed.attestation.source_fingerprint != binding.source_fingerprint
            || parsed.attestation.wrapper_sha256 != binding.wrapper_sha256
        {
            return Err(ProtectedGateError::BindingMismatch);
        }
        Ok(())
    }
}

fn running_binary_sha256() -> Result<String, ProtectedGateError> {
    let executable = std::env::current_exe().map_err(|_| ProtectedGateError::InvalidAuthority)?;
    let bytes = fs::read(executable).map_err(|_| ProtectedGateError::InvalidAuthority)?;
    Ok(lowercase_hex(&ac_ant_delirium::libres::Sha2_unit::sha256(
        &bytes,
    )))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_sorted_unique(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_standard_base64(value: &str) -> Option<Vec<u8>> {
    if value.is_empty() || value.len() % 4 != 0 || !value.is_ascii() {
        return None;
    }
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(value.len() / 4 * 3);
    for (chunk_index, chunk) in bytes.chunks_exact(4).enumerate() {
        let final_chunk = chunk_index + 1 == bytes.len() / 4;
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c_padding = chunk[2] == b'=';
        let d_padding = chunk[3] == b'=';
        if c_padding && (!d_padding || !final_chunk) || d_padding && !final_chunk {
            return None;
        }
        let c = if c_padding {
            0
        } else {
            base64_value(chunk[2])?
        };
        let d = if d_padding {
            0
        } else {
            base64_value(chunk[3])?
        };
        if c_padding && b & 0x0f != 0 || d_padding && !c_padding && c & 0x03 != 0 {
            return None;
        }
        output.push((a << 2) | (b >> 4));
        if !c_padding {
            output.push((b << 4) | (c >> 2));
        }
        if !d_padding {
            output.push((c << 6) | d);
        }
    }
    Some(output)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) fn test_verifier() -> ProtectedGateVerifier {
    use ring::signature::KeyPair;

    let pair = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[0x5a; 32])
        .expect("fixed Ed25519 test key");
    ProtectedGateVerifier::from_raw_public_key(
        pair.public_key()
            .as_ref()
            .try_into()
            .expect("Ed25519 public key length"),
        "b".repeat(64),
    )
}

#[cfg(test)]
pub(crate) fn sign_test_gate(binding: &ProtectedGateBinding<'_>) -> Vec<u8> {
    let attestation = serde_json::json!({
        "blocking_violations": [],
        "checkpoint_epoch": 1,
        "contract_version": "protected-task-gate-attestation-v1",
        "decision": "passed",
        "project_id": binding.project_id,
        "policy_sha256": "c".repeat(64),
        "mechanical_evidence_sha256": "d".repeat(64),
        "integrity_envelope_sha256": "e".repeat(64),
        "review_id": binding.review_id,
        "source_fingerprint": binding.source_fingerprint,
        "state_revision": binding.state_revision,
        "task_id": binding.task_id,
        "task_revision": binding.task_revision,
        "workspace_id": binding.workspace_id,
        "wrapper_sha256": binding.wrapper_sha256,
    });
    sign_test_attestation(attestation)
}

#[cfg(test)]
fn sign_test_attestation(attestation: Value) -> Vec<u8> {
    let mut canonical = serde_json::to_vec(&attestation).expect("canonical test attestation");
    canonical.push(b'\n');
    let pair = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[0x5a; 32])
        .expect("fixed Ed25519 test key");
    let signature = pair.sign(&canonical);
    serde_json::to_vec(&serde_json::json!({
        "attestation": attestation,
        "contract_version": "signed-protected-task-gate-attestation-v1",
        "signature_base64": encode_standard_base64(signature.as_ref()),
    }))
    .expect("signed test gate")
}

#[cfg(test)]
fn encode_standard_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(c & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

#[cfg(test)]
#[path = "tests/Protected_Gate_test.rs"]
mod protected_gate_tests;
