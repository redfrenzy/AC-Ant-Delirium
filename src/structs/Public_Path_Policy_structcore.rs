use super::{TransactionalStorageError, TransactionalStorageResult, PUBLIC_PATH_REDACTION};

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

pub(super) fn validate_serialized_public_document(
    value: &[u8],
    field: &str,
) -> TransactionalStorageResult<()> {
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
        reject_public_path_references(&decoded, field)?;
        index = end;
    }
    Ok(())
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
