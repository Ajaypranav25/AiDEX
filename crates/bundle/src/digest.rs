//! Streaming SHA-256 helpers (lowercase hex, the form used in manifests and SHA256SUMS).

use std::{
    fs::File,
    io::{self, ErrorKind, Read},
    path::Path,
};

use sha2::{Digest, Sha256};

const BUFFER_BYTES: usize = 256 * 1024;

/// SHA-256 of `bytes` as lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// SHA-256 and byte count of everything `reader` yields.
pub fn sha256_reader(mut reader: impl Read) -> io::Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut total = 0_u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((to_hex(&hasher.finalize()), total))
}

/// SHA-256 and size of a file's contents.
pub fn sha256_file(path: &Path) -> io::Result<(String, u64)> {
    sha256_reader(File::open(path)?)
}

/// Whether `value` is a lowercase-hex SHA-256 digest.
pub fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Decode lowercase or uppercase hex; `None` for odd lengths or non-hex digits.
pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

/// Whether `value` is exactly `bytes` bytes of lowercase hex.
pub(crate) fn is_lower_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_match_known_vectors() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let (digest, size) = sha256_reader(&b"abc"[..]).unwrap();
        assert_eq!((digest.as_str(), size), (sha256_hex(b"abc").as_str(), 3));
        assert!(is_sha256_hex(&sha256_hex(b"")));
        assert!(!is_sha256_hex(&sha256_hex(b"").to_uppercase()));
        assert!(!is_sha256_hex("abc"));
        assert_eq!(from_hex("00ffA0").unwrap(), [0x00, 0xff, 0xa0]);
        assert_eq!(to_hex(&from_hex("0a1b").unwrap()), "0a1b");
        assert!(from_hex("abc").is_none() && from_hex("zz").is_none());
        assert!(is_lower_hex("0a1b", 2) && !is_lower_hex("0A1B", 2) && !is_lower_hex("0a", 2));
    }
}
