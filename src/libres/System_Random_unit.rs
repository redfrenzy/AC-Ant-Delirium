use std::io;

const CROCKFORD_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

pub fn fill_system_random(output: &mut [u8]) -> io::Result<()> {
    let mut written = 0usize;
    while written < output.len() {
        let result = unsafe {
            libc::getrandom(
                output[written..].as_mut_ptr().cast(),
                output.len() - written,
                0,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if result == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "system entropy returned no bytes",
            ));
        }
        written += result as usize;
    }
    Ok(())
}

pub fn random_crockford_id(prefix: &str) -> io::Result<String> {
    let mut entropy = [0_u8; 16];
    fill_system_random(&mut entropy)?;
    Ok(format!("{prefix}{}", crockford_token(&entropy)))
}

fn crockford_token(entropy: &[u8; 16]) -> String {
    let value = u128::from_be_bytes(*entropy);
    let mut token = [b'0'; 26];
    for (index, slot) in token.iter_mut().enumerate() {
        let shift = 125usize.saturating_sub(index * 5);
        let digit = if index == 0 {
            ((value >> shift) & 0x07) as usize
        } else {
            ((value >> shift) & 0x1f) as usize
        };
        *slot = CROCKFORD_ALPHABET[digit];
    }
    String::from_utf8(token.to_vec()).expect("Crockford alphabet is ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_entropy_produces_nonzero_distinct_crockford_ids() {
        let first = random_crockford_id("evt_").expect("first identifier");
        let second = random_crockford_id("evt_").expect("second identifier");
        for identifier in [&first, &second] {
            assert_eq!(identifier.len(), 30);
            assert!(identifier.starts_with("evt_"));
            assert!(identifier[4..].bytes().all(|byte| {
                matches!(byte, b'0'..=b'9' | b'a'..=b'h' | b'j'..=b'k' | b'm'..=b'n' | b'p'..=b't' | b'v'..=b'z')
            }));
        }
        assert_ne!(first, second);
    }
}
