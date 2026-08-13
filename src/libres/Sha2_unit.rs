use sha2::{Digest, Sha256};

pub fn sha256(input: &[u8]) -> [u8; 32] {
    Sha256::digest(input).into()
}

pub fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let compared = left.len().min(right.len());
    for index in 0..compared {
        difference |= usize::from(left[index] ^ right[index]);
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_with_sha256_and_compares_without_early_exit() {
        assert_eq!(
            sha256(b"abc"),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]
        );
        assert!(constant_time_equal(b"same", b"same"));
        assert!(!constant_time_equal(b"same", b"sand"));
        assert!(!constant_time_equal(b"same", b"same-longer"));
    }
}
