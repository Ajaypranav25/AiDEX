//! Sealing of artifact metadata and of artifact bytes in independently
//! authenticated 64 KiB segments, so a paged read decrypts at most two segments.
//!
//! Each sealed unit is `nonce (24) || XChaCha20-Poly1305 ciphertext+tag`. Segment
//! AAD binds the artifact id, the plaintext SHA-256 recorded in the sealed
//! metadata, the segment index, and the segment count: segments cannot be
//! moved, dropped, reordered, or mixed across artifacts or revisions.
use crate::StoreError;
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use zeroize::Zeroizing;

/// Plaintext bytes per sealed segment.
pub(crate) const SEGMENT_BYTES: u64 = 64 * 1024;
const NONCE_BYTES: usize = 24;
/// Nonce plus Poly1305 tag added to each sealed unit.
pub(crate) const OVERHEAD: u64 = 40;

pub(crate) fn cipher(key: &[u8]) -> Result<XChaCha20Poly1305, StoreError> {
    XChaCha20Poly1305::new_from_slice(key).map_err(|_| "invalid record key".into())
}

pub(crate) fn seal(
    cipher: &XChaCha20Poly1305,
    aad: &str,
    plain: &[u8],
) -> Result<Vec<u8>, StoreError> {
    let mut nonce = [0u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "artifact encryption failed")?;
    let mut out = Vec::with_capacity(NONCE_BYTES + encrypted.len());
    out.extend_from_slice(&nonce);
    out.extend(encrypted);
    Ok(out)
}

pub(crate) fn open(
    cipher: &XChaCha20Poly1305,
    aad: &str,
    sealed: &[u8],
) -> Result<Zeroizing<Vec<u8>>, StoreError> {
    if sealed.len() < OVERHEAD as usize {
        return Err("truncated encrypted artifact".into());
    }
    cipher
        .decrypt(
            XNonce::from_slice(&sealed[..NONCE_BYTES]),
            Payload {
                msg: &sealed[NONCE_BYTES..],
                aad: aad.as_bytes(),
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| "artifact authentication failed".into())
}

pub(crate) fn meta_aad(id: &str, session_id: &str) -> String {
    format!("aidex-native-v1:artifact:{id}:{session_id}")
}

fn segment_aad(id: &str, sha256: &str, index: u64, count: u64) -> String {
    format!("aidex-native-v1:artifact-content:{id}:{sha256}:{index}:{count}")
}

pub(crate) fn segment_count(size: u64) -> u64 {
    size.div_ceil(SEGMENT_BYTES)
}

/// Total sealed length of `size` plaintext bytes.
pub(crate) fn sealed_len(size: u64) -> u64 {
    size + segment_count(size) * OVERHEAD
}

/// Byte range `(start, len)` of sealed segment `index` within the sealed content.
pub(crate) fn segment_span(index: u64, size: u64) -> (u64, u64) {
    let plain = (size - index * SEGMENT_BYTES).min(SEGMENT_BYTES);
    (index * (SEGMENT_BYTES + OVERHEAD), plain + OVERHEAD)
}

/// Seal `bytes` as consecutive segments.
pub(crate) fn seal_content(
    cipher: &XChaCha20Poly1305,
    id: &str,
    sha256: &str,
    bytes: &[u8],
) -> Result<Vec<u8>, StoreError> {
    let count = segment_count(bytes.len() as u64);
    let mut out = Vec::with_capacity(sealed_len(bytes.len() as u64) as usize);
    for (index, segment) in bytes.chunks(SEGMENT_BYTES as usize).enumerate() {
        out.extend(seal(
            cipher,
            &segment_aad(id, sha256, index as u64, count),
            segment,
        )?);
    }
    Ok(out)
}

/// Open sealed segment `index` of an artifact of `size` plaintext bytes.
pub(crate) fn open_segment(
    cipher: &XChaCha20Poly1305,
    id: &str,
    sha256: &str,
    size: u64,
    index: u64,
    sealed: &[u8],
) -> Result<Zeroizing<Vec<u8>>, StoreError> {
    let (_, len) = segment_span(index, size);
    if sealed.len() as u64 != len {
        return Err("truncated encrypted artifact".into());
    }
    open(
        cipher,
        &segment_aad(id, sha256, index, segment_count(size)),
        sealed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_round_trip_and_refuse_reordering() {
        let cipher = cipher(&[7u8; 32]).unwrap();
        let bytes: Vec<u8> = (0..SEGMENT_BYTES * 2 + 5).map(|n| n as u8).collect();
        let size = bytes.len() as u64;
        let sealed = seal_content(&cipher, "art_a", "sha", &bytes).unwrap();
        assert_eq!(sealed.len() as u64, sealed_len(size));
        assert_eq!(segment_count(size), 3);
        let mut plain = vec![];
        for index in 0..3 {
            let (start, len) = segment_span(index, size);
            let segment = &sealed[start as usize..(start + len) as usize];
            plain.extend_from_slice(
                &open_segment(&cipher, "art_a", "sha", size, index, segment).unwrap(),
            );
        }
        assert_eq!(plain, bytes);
        let (start, len) = segment_span(0, size);
        let first = &sealed[start as usize..(start + len) as usize];
        assert!(open_segment(&cipher, "art_a", "sha", size, 1, first).is_err());
        assert!(open_segment(&cipher, "art_b", "sha", size, 0, first).is_err());
        assert!(open_segment(&cipher, "art_a", "other", size, 0, first).is_err());
        assert_eq!(sealed_len(0), 0);
    }
}
