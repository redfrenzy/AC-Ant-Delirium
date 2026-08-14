use crate::libres::Sha2_unit::sha256;

use super::{TransactionalStorageError, TransactionalStorageResult, PUBLIC_PATH_REDACTION};

// Fingerprints keep the administrative vocabulary out of release strings. The
// human-readable denylist belongs to the versioned contract and adversarial tests.
const PROTECTED_PROJECT_ROOT_FINGERPRINTS: [&str; 7] = [
    "4d0a4dff1f762e594ecf10f259c33a6b71a7c02f49e2ce89a95e0719a3bd0529",
    "a9d4034da07d8ef31db1cd4119b6a4552fdfbd19787e2848e71c8ee3b47703a7",
    "92cd10e6b8b068a931196d1d73a032543d5ca1a5bf445e27a1af74258254517c",
    "64879f7d6b960a01909762d911a32d4582c20010c5641ee90278b644a9e3b525",
    "c404dc69b55c0692366e3f28e6d8bc2e506bedbd4f519644adb5802500d83331",
    "0fc8d75ec1445b0fcd89f89f6ae94511e6ef154808dce49aaa16a193850c0028",
    "013877ff312aac698445f97fa9147a8245c7c4c401c84d7dabed433a994e697a",
];
const PROTECTED_ARTIFACT_FINGERPRINTS: [&str; 16] = [
    "ae37437dc0425cba7436777376390ac151c537cbfecfc059bf1e6c9b16a97e25",
    "25eaad17ce499a5450af9e3e7986c89908f1b39fd7dc03baa80cf2232bc67ee1",
    "8c230bda4f6a0c35d9041252162b90d0342a14219803acc9bdf53f3abb5291ae",
    "ba915a0a68b7a2391ade4c3160abe69a8f77c1c324f43e4c5beebc721cb987b7",
    "03f3e7a3859b2a5621edde56bb35bb0607c533f6e2bc3bec6524f5a67cd39891",
    "1ec8f67089d5ee5d78199da27d4589610c44bb1c1d96ee5a8fb6fcc40ac8b84a",
    "e65acba639acc8c0b9a4434e936a854974717b63cd344f1a94345206386c171c",
    "fd1ce6eb49820dca77184b532abd804e773f67aba4e323c96a3db39e4aadf12d",
    "a12aa54daf6b7d53ddaf120b8fd1c8c39b24fa6ab02377832e8b6aaf5f96091f",
    "fc7e389ede69bff1c429c0c55890c0cec90a194accee6a722a095ac312c155ca",
    "8446e018f9ea38ebbd1735ccce3278bd6ff770672c538d26f181f0456d159c49",
    "b864f1ffc0d3cd6ebd0bb82c1b7a8379d43b8af1d03e96502a9f3c333571cf20",
    "b0e2e744d6a09f6d9098493f146bdbba554881941b18f265bc3ad7e8b181beef",
    "b545549dfa97f251258788754ad54094ab780da821011ca026a1329a9ff3a150",
    "974b3a6512620247c7121d97d1a73ff3da4e1e2ebc5f6c1d9645fb3abc78ce29",
    "e6319dfa4e88fe54d082675e350080edaac79d729d4270e296b519056ff8cfd9",
];
const PROTECTED_ADMINISTRATIVE_KEY_FINGERPRINTS: [&str; 18] = [
    "a4cb5cb247c8419e7f18f4e5b3e4ab7d596e6312ec99cdeb0ff97c46db7bf9f1",
    "22e72796f7d49e85ba22dd4b46427faf4a813d514f469e29cafe37c3c6811648",
    "6b7a980094cfe4007447f5dd2989201bce668e9d649f46e92159b9e00e1a65bf",
    "b79606fb3afea5bd1609ed40b622142f1c98125abcfe89a76a661b0e8e343910",
    "b7d64a9221007dfd5390f7df6cd5b8f3ea4f82faa1237141e35ebf161f5511a1",
    "613f4018084633df7a41e7b53734c868d930b2fb33f71ebd98214f7eb648b2e9",
    "ffe97bba510b9f5cf1bb187bebb59cefa42a3f1654d2c993654382cd10468c2e",
    "259aa8ef98a8b91de574cd904138ef643240c23080cf24da4793a6f10a43fa9d",
    "6842ca7df10554ecae09529503fafc52abfb2b8dca1747a0811da4adc6290f52",
    "428821350e9691491f616b754cd8315fb86d797ab35d843479e732ef90665324",
    "0966b7aaf3a9c601dcb73536d1e39fe828636bb29f5867feb9a32398ef43c44a",
    "915cfe2842da29241318718ccea83501cb0de2a8679b46540146995312ec02a1",
    "64879f7d6b960a01909762d911a32d4582c20010c5641ee90278b644a9e3b525",
    "c404dc69b55c0692366e3f28e6d8bc2e506bedbd4f519644adb5802500d83331",
    "4d87be9c2a979580973c126702514ce9f862d78051199e1d226c6645d6d686c3",
    "e673e1e0389af81571609d3a512bdc106bed47ab5156906571665f82628ffd46",
    "085154084c7427596104bc42f51f59f6d3ffd3d5f49f098210c20449fb7b2c71",
    "9cb149d7aa698ee6b5fa9f85283b7bb8c609e42ce9817d82571b594f0a8c4b03",
];
const PROTECTED_TEXT_IDENTIFIER_FINGERPRINTS: [&str; 7] = [
    "a4cb5cb247c8419e7f18f4e5b3e4ab7d596e6312ec99cdeb0ff97c46db7bf9f1",
    "22e72796f7d49e85ba22dd4b46427faf4a813d514f469e29cafe37c3c6811648",
    "6b7a980094cfe4007447f5dd2989201bce668e9d649f46e92159b9e00e1a65bf",
    "6842ca7df10554ecae09529503fafc52abfb2b8dca1747a0811da4adc6290f52",
    "4d87be9c2a979580973c126702514ce9f862d78051199e1d226c6645d6d686c3",
    "e673e1e0389af81571609d3a512bdc106bed47ab5156906571665f82628ffd46",
    "fe0acc0d615e5972066ad716f6d55c716969a6b33ec51383f5e01851e367aa75",
];

pub(super) fn sanitize_public_text(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let mut sanitized = String::with_capacity(value.len());
    let mut index = 0usize;
    while index < characters.len() {
        if let Some(end) = private_path_reference_end(&characters, index) {
            sanitized.push_str(PUBLIC_PATH_REDACTION);
            index = end;
            continue;
        }
        sanitized.push(characters[index]);
        index += 1;
    }
    sanitized
}

pub(super) fn reject_public_path_references(
    value: &str,
    field: &str,
) -> TransactionalStorageResult<()> {
    let characters = value.chars().collect::<Vec<_>>();
    if (0..characters.len()).any(|index| private_path_reference_end(&characters, index).is_some()) {
        return Err(invalid(&format!(
            "{field} contains an absolute, external, or traversing path reference"
        )));
    }
    Ok(())
}

pub(super) fn validate_project_relative_path(
    value: &str,
    field: &str,
) -> TransactionalStorageResult<()> {
    let length = value.chars().count();
    if length == 0 || length > 4_096 || value.chars().any(char::is_control) {
        return Err(invalid(&format!(
            "{field} violates the public relative-path text contract"
        )));
    }
    if value.contains('\\')
        || starts_with_private_path_reference(value)
        || has_uri_scheme(value)
        || contains_percent_escape(value)
    {
        return Err(invalid(&format!(
            "{field} must remain a canonical project-relative path"
        )));
    }
    if value
        .split('/')
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(invalid(&format!(
            "{field} must not contain empty, current, or parent components"
        )));
    }
    Ok(())
}

pub(super) fn validate_agent_visible_project_path(
    value: &str,
    field: &str,
) -> TransactionalStorageResult<()> {
    validate_project_relative_path(value, field)?;
    let normalized = value.to_ascii_lowercase();
    let mut components = normalized.split('/');
    if components.next().is_some_and(|root| {
        matches_protected_fingerprint(root, &PROTECTED_PROJECT_ROOT_FINGERPRINTS)
    }) || normalized
        .split('/')
        .any(|component| matches_protected_fingerprint(component, &PROTECTED_ARTIFACT_FINGERPRINTS))
    {
        return Err(invalid(&format!(
            "{field} identifies protected administrative state"
        )));
    }
    Ok(())
}

pub(super) fn reject_protected_administrative_references(
    value: &str,
    field: &str,
) -> TransactionalStorageResult<()> {
    let normalized = value.to_ascii_lowercase();
    let has_artifact = contains_protected_token(&normalized, &PROTECTED_ARTIFACT_FINGERPRINTS);
    let has_identifier =
        contains_protected_token(&normalized, &PROTECTED_TEXT_IDENTIFIER_FINGERPRINTS);
    let has_root = contains_protected_root_reference(&normalized);
    if has_artifact || has_identifier || has_root {
        return Err(invalid(&format!(
            "{field} contains a protected administrative reference"
        )));
    }
    Ok(())
}

pub(super) fn validate_serialized_public_document(
    value: &[u8],
    field: &str,
) -> TransactionalStorageResult<()> {
    let document =
        std::str::from_utf8(value).map_err(|_| invalid(&format!("{field} must be valid UTF-8")))?;
    let trimmed = document.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err(invalid(&format!(
            "{field} must be a serialized JSON object"
        )));
    }
    let characters = document.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < characters.len() {
        if characters[index] != '"' {
            index = consume_json_public_token(&characters, index, field)?;
            continue;
        }
        let (decoded, end) = decode_json_string(&characters, index + 1, field)?;
        reject_public_path_references(&decoded, field)?;
        index = end;
    }
    Ok(())
}

fn consume_json_public_token(
    characters: &[char],
    index: usize,
    field: &str,
) -> TransactionalStorageResult<usize> {
    let value = characters[index];
    if value.is_whitespace()
        || matches!(
            value,
            '{' | '}' | '[' | ']' | ':' | ',' | '-' | '+' | '.' | '0'..='9' | 'e' | 'E'
        )
    {
        return Ok(index + 1);
    }
    for literal in ["true", "false", "null"] {
        if starts_with_exact(characters, index, literal) {
            return Ok(index + literal.len());
        }
    }
    Err(invalid(&format!(
        "{field} contains a non-JSON token outside a string"
    )))
}

fn starts_with_exact(characters: &[char], index: usize, expected: &str) -> bool {
    let expected = expected.chars().collect::<Vec<_>>();
    characters
        .get(index..index + expected.len())
        .is_some_and(|actual| actual == expected)
}

pub(super) fn validate_serialized_agent_projection_document(
    value: &[u8],
    field: &str,
) -> TransactionalStorageResult<()> {
    validate_serialized_public_document(value, field)?;
    let document =
        std::str::from_utf8(value).map_err(|_| invalid(&format!("{field} must be valid UTF-8")))?;
    let characters = document.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < characters.len() {
        if characters[index] != '"' {
            index += 1;
            continue;
        }
        let (decoded, end) = decode_json_string(&characters, index + 1, field)?;
        if json_key_follows(&characters, end) {
            let key = decoded.to_ascii_lowercase();
            if matches_protected_fingerprint(&key, &PROTECTED_ADMINISTRATIVE_KEY_FINGERPRINTS) {
                return Err(invalid(&format!(
                    "{field} contains a protected administrative field"
                )));
            }
        }
        reject_protected_administrative_references(&decoded, field)?;
        index = end;
    }
    Ok(())
}

fn json_key_follows(characters: &[char], mut index: usize) -> bool {
    while characters
        .get(index)
        .is_some_and(|value| value.is_whitespace())
    {
        index += 1;
    }
    characters.get(index) == Some(&':')
}

fn contains_protected_token(value: &str, fingerprints: &[&str]) -> bool {
    value
        .split(|character: char| !is_identifier_character(character))
        .map(|token| token.trim_matches('.'))
        .filter(|token| !token.is_empty())
        .any(|token| matches_protected_fingerprint(token, fingerprints))
}

fn contains_protected_root_reference(value: &str) -> bool {
    value.match_indices(['/', '\\']).any(|(separator, _)| {
        let prefix = &value[..separator];
        let candidate = prefix
            .rsplit(|character: char| !is_identifier_character(character))
            .next()
            .unwrap_or_default();
        !candidate.is_empty()
            && matches_protected_fingerprint(candidate, &PROTECTED_PROJECT_ROOT_FINGERPRINTS)
    })
}

fn is_identifier_character(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '_' | '-' | '.')
}

fn matches_protected_fingerprint(value: &str, fingerprints: &[&str]) -> bool {
    let digest = sha256(value.as_bytes());
    let mut encoded = [0u8; 64];
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (index, byte) in digest.iter().copied().enumerate() {
        encoded[index * 2] = HEX[(byte >> 4) as usize];
        encoded[index * 2 + 1] = HEX[(byte & 0x0f) as usize];
    }
    fingerprints
        .iter()
        .any(|expected| expected.as_bytes() == encoded)
}

fn decode_json_string(
    characters: &[char],
    start: usize,
    field: &str,
) -> TransactionalStorageResult<(String, usize)> {
    let mut decoded = String::new();
    let mut index = start;
    while index < characters.len() {
        match characters[index] {
            '"' => return Ok((decoded, index + 1)),
            '\\' => {
                let escape = characters.get(index + 1).ok_or_else(|| {
                    invalid(&format!("{field} contains an incomplete JSON escape"))
                })?;
                match escape {
                    '"' | '\\' | '/' => decoded.push(*escape),
                    'b' => decoded.push('\u{0008}'),
                    'f' => decoded.push('\u{000c}'),
                    'n' => decoded.push('\n'),
                    'r' => decoded.push('\r'),
                    't' => decoded.push('\t'),
                    'u' => {
                        decoded.push(decode_json_ascii_escape(characters, index + 2, field)?);
                        index += 4;
                    }
                    _ => {
                        return Err(invalid(&format!("{field} contains an invalid JSON escape")));
                    }
                }
                index += 2;
            }
            value if value.is_control() => {
                return Err(invalid(&format!(
                    "{field} contains an unescaped JSON control character"
                )));
            }
            value => {
                decoded.push(value);
                index += 1;
            }
        }
    }
    Err(invalid(&format!(
        "{field} contains an unterminated JSON string"
    )))
}

fn decode_json_ascii_escape(
    characters: &[char],
    start: usize,
    field: &str,
) -> TransactionalStorageResult<char> {
    let digits = characters
        .get(start..start + 4)
        .ok_or_else(|| invalid(&format!("{field} contains an incomplete Unicode escape")))?;
    if !digits.iter().all(|digit| digit.is_ascii_hexdigit()) {
        return Err(invalid(&format!(
            "{field} contains an invalid Unicode escape"
        )));
    }
    let encoded = digits.iter().collect::<String>();
    let code = u32::from_str_radix(&encoded, 16)
        .map_err(|_| invalid(&format!("{field} contains an invalid Unicode escape")))?;
    if code <= 0x7f {
        return char::from_u32(code)
            .ok_or_else(|| invalid(&format!("{field} contains an invalid Unicode escape")));
    }
    Ok('x')
}

fn starts_with_private_path_reference(value: &str) -> bool {
    let characters = value.chars().collect::<Vec<_>>();
    private_path_reference_end(&characters, 0).is_some()
}

fn private_path_reference_end(characters: &[char], index: usize) -> Option<usize> {
    if index >= characters.len() {
        return None;
    }
    let boundary = is_token_boundary(characters, index);
    let traversal_boundary = boundary || (index > 0 && matches!(characters[index - 1], '/' | '\\'));
    if traversal_boundary && traversal_starts_at(characters, index) {
        return Some(path_reference_end(characters, index));
    }
    if !boundary {
        return None;
    }
    if file_uri_starts_at(characters, index)
        || environment_path_starts_at(characters, index)
        || tilde_path_starts_at(characters, index)
        || windows_drive_path_starts_at(characters, index)
        || windows_rooted_path_starts_at(characters, index)
        || encoded_private_path_starts_at(characters, index)
    {
        return Some(path_reference_end(characters, index));
    }
    if characters[index] == '/' && !web_uri_path_starts_at(characters, index) {
        return Some(path_reference_end(characters, index));
    }
    None
}

fn is_token_boundary(characters: &[char], index: usize) -> bool {
    index == 0 || is_path_boundary(characters[index - 1])
}

fn file_uri_starts_at(characters: &[char], index: usize) -> bool {
    starts_with_ascii_case_insensitive(characters, index, "file:")
        && matches!(characters.get(index + 5), Some('/' | '\\'))
}

fn web_uri_path_starts_at(characters: &[char], index: usize) -> bool {
    if index == 0 || characters.get(index - 1) != Some(&':') {
        return false;
    }
    let mut start = index - 1;
    while start > 0 && characters[start - 1].is_ascii_alphanumeric() {
        start -= 1;
    }
    let scheme = characters[start..index - 1]
        .iter()
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(scheme.as_str(), "http" | "https")
}

fn environment_path_starts_at(characters: &[char], index: usize) -> bool {
    match characters[index] {
        '$' => dollar_environment_path(characters, index),
        '%' => percent_environment_path(characters, index),
        _ => false,
    }
}

fn dollar_environment_path(characters: &[char], index: usize) -> bool {
    let Some(next) = characters.get(index + 1) else {
        return false;
    };
    if *next == '{' {
        let mut cursor = index + 2;
        while characters
            .get(cursor)
            .is_some_and(|value| is_environment_name(*value))
        {
            cursor += 1;
        }
        return cursor > index + 2
            && characters.get(cursor) == Some(&'}')
            && matches!(characters.get(cursor + 1), Some('/' | '\\'));
    }
    let mut cursor = index + 1;
    while characters
        .get(cursor)
        .is_some_and(|value| is_environment_name(*value))
    {
        cursor += 1;
    }
    cursor > index + 1 && matches!(characters.get(cursor), Some('/' | '\\'))
}

fn percent_environment_path(characters: &[char], index: usize) -> bool {
    let mut cursor = index + 1;
    while characters
        .get(cursor)
        .is_some_and(|value| is_environment_name(*value))
    {
        cursor += 1;
    }
    cursor > index + 1
        && characters.get(cursor) == Some(&'%')
        && matches!(characters.get(cursor + 1), Some('/' | '\\'))
}

fn is_environment_name(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn tilde_path_starts_at(characters: &[char], index: usize) -> bool {
    if characters[index] != '~' {
        return false;
    }
    let mut cursor = index + 1;
    while characters
        .get(cursor)
        .is_some_and(|value| value.is_ascii_alphanumeric() || matches!(value, '_' | '-' | '.'))
    {
        cursor += 1;
    }
    matches!(characters.get(cursor), Some('/' | '\\'))
}

fn windows_drive_path_starts_at(characters: &[char], index: usize) -> bool {
    characters[index].is_ascii_alphabetic()
        && characters.get(index + 1) == Some(&':')
        && matches!(characters.get(index + 2), Some('/' | '\\'))
}

fn windows_rooted_path_starts_at(characters: &[char], index: usize) -> bool {
    characters[index] == '\\'
}

fn traversal_starts_at(characters: &[char], index: usize) -> bool {
    characters.get(index) == Some(&'.')
        && characters.get(index + 1) == Some(&'.')
        && matches!(characters.get(index + 2), Some('/' | '\\'))
}

fn encoded_private_path_starts_at(characters: &[char], index: usize) -> bool {
    characters.get(index) == Some(&'%')
        && characters
            .get(index + 1)
            .is_some_and(|value| value.is_ascii_hexdigit())
        && characters
            .get(index + 2)
            .is_some_and(|value| value.is_ascii_hexdigit())
}

fn path_reference_end(characters: &[char], start: usize) -> usize {
    let mut end = start;
    while end < characters.len() && !is_path_terminator(characters[end]) {
        end += 1;
    }
    end
}

fn is_path_boundary(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '"' | '\'' | '`' | '(' | '[' | '{' | '<' | '=' | ':' | ',' | ';'
        )
}

fn is_path_terminator(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '"' | '\'' | '`' | ')' | ']' | '}' | '>' | ',' | ';'
        )
}

fn starts_with_ascii_case_insensitive(characters: &[char], index: usize, expected: &str) -> bool {
    let expected = expected.chars().collect::<Vec<_>>();
    characters
        .get(index..index + expected.len())
        .is_some_and(|actual| {
            actual
                .iter()
                .zip(expected.iter())
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        })
}

fn has_uri_scheme(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    let scheme = &value[..colon];
    !scheme.is_empty()
        && scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || (index > 0 && (byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')))
        })
}

fn contains_percent_escape(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.windows(3).any(|window| {
        window[0] == b'%' && window[1].is_ascii_hexdigit() && window[2].is_ascii_hexdigit()
    })
}

fn invalid(message: &str) -> TransactionalStorageError {
    TransactionalStorageError::InvalidInput(message.to_string())
}
