//! Thin wrappers over reviewed RustCrypto primitives: OS randomness, HKDF-SHA256
//! subkeys, XChaCha20-Poly1305 sealing, HMAC-SHA256, and an unambiguous
//! length-prefixed transcript used for associated data and MAC input.
//!
//! No primitive is implemented here; this module only fixes how they are combined.

use crate::{VaultError, secret::REDACTED};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::fmt;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub(crate) const KEY_LEN: usize = 32;
pub(crate) const NONCE_LEN: usize = 24;
pub(crate) const TAG_LEN: usize = 16;
pub(crate) const SALT_LEN: usize = 32;
pub(crate) const MAC_LEN: usize = 32;

/// 256-bit key material, zeroized on drop and never printed.
pub(crate) struct Key(Zeroizing<[u8; KEY_LEN]>);

impl Key {
    pub(crate) fn random() -> Result<Self, VaultError> {
        let mut bytes = Zeroizing::new([0_u8; KEY_LEN]);
        fill_random(bytes.as_mut())?;
        Ok(Self(bytes))
    }

    pub(crate) fn from_bytes(bytes: Zeroizing<[u8; KEY_LEN]>) -> Self {
        Self(bytes)
    }

    /// Copy 32 bytes from a decrypted buffer; any other length is rejected.
    pub(crate) fn from_slice(bytes: &[u8]) -> Option<Self> {
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);
        if bytes.len() != KEY_LEN {
            return None;
        }
        key.copy_from_slice(bytes);
        Some(Self(key))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// HKDF-SHA256 with an all-zero salt (the input is a uniformly random key) and
    /// `info` = the concatenation of `info_parts`.
    pub(crate) fn derive(&self, info_parts: &[&[u8]]) -> Result<Self, VaultError> {
        hkdf_expand(None, self.as_bytes(), info_parts)
    }

    /// Constant-time equality.
    pub(crate) fn ct_eq(&self, other: &Self) -> bool {
        bool::from(self.0.as_slice().ct_eq(other.0.as_slice()))
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Key({REDACTED})")
    }
}

pub(crate) fn hkdf_expand(
    salt: Option<&[u8]>,
    input: &[u8],
    info_parts: &[&[u8]],
) -> Result<Key, VaultError> {
    let mut output = Zeroizing::new([0_u8; KEY_LEN]);
    Hkdf::<Sha256>::new(salt, input)
        .expand_multi_info(info_parts, output.as_mut())
        .map_err(|_| VaultError::Crypto("HKDF output length rejected"))?;
    Ok(Key(output))
}

pub(crate) fn fill_random(buffer: &mut [u8]) -> Result<(), VaultError> {
    getrandom::fill(buffer).map_err(|error| VaultError::Randomness(error.to_string()))
}

pub(crate) fn random_array<const N: usize>() -> Result<[u8; N], VaultError> {
    let mut bytes = [0_u8; N];
    fill_random(&mut bytes)?;
    Ok(bytes)
}

/// XChaCha20-Poly1305 with a fresh random 192-bit nonce. Returns `(nonce, ciphertext || tag)`.
pub(crate) fn seal(
    key: &Key,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<([u8; NONCE_LEN], Vec<u8>), VaultError> {
    let nonce = random_array::<NONCE_LEN>()?;
    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key.as_bytes()));
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::Crypto("authenticated encryption failed"))?;
    Ok((nonce, ciphertext))
}

/// Authenticated decryption; `None` when the key, nonce, AAD, or ciphertext do not match.
pub(crate) fn open(
    key: &Key,
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    ciphertext: &[u8],
) -> Option<Zeroizing<Vec<u8>>> {
    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key.as_bytes()));
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .ok()
        .map(Zeroizing::new)
}

pub(crate) fn mac(key: &Key, message: &[u8]) -> Result<[u8; MAC_LEN], VaultError> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key.as_bytes())
        .map_err(|_| VaultError::Crypto("HMAC key length rejected"))?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().into())
}

/// Constant-time HMAC verification.
pub(crate) fn verify_mac(key: &Key, message: &[u8], tag: &[u8]) -> bool {
    let Ok(mut mac) = <Hmac<Sha256> as Mac>::new_from_slice(key.as_bytes()) else {
        return false;
    };
    mac.update(message);
    mac.verify_slice(tag).is_ok()
}

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Unambiguous encoding of labelled fields: every byte string is prefixed with its
/// length as a little-endian `u64`; integers are fixed-width little-endian.
pub(crate) struct Transcript(Vec<u8>);

impl Transcript {
    pub(crate) fn new(label: &str) -> Self {
        Self(Vec::new()).bytes(label.as_bytes())
    }

    pub(crate) fn bytes(mut self, value: &[u8]) -> Self {
        self.0
            .extend_from_slice(&(value.len() as u64).to_le_bytes());
        self.0.extend_from_slice(value);
        self
    }

    pub(crate) fn str(self, value: &str) -> Self {
        self.bytes(value.as_bytes())
    }

    pub(crate) fn u32(mut self, value: u32) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub(crate) fn u64(mut self, value: u64) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_data_opens_only_with_the_same_key_and_associated_data() {
        let key = Key::random().unwrap();
        let other = Key::random().unwrap();
        let (nonce, ciphertext) = seal(&key, b"aad", b"payload").unwrap();
        assert_eq!(ciphertext.len(), b"payload".len() + TAG_LEN);
        assert_eq!(
            open(&key, &nonce, b"aad", &ciphertext).unwrap().as_slice(),
            b"payload"
        );
        assert!(open(&other, &nonce, b"aad", &ciphertext).is_none());
        assert!(open(&key, &nonce, b"other", &ciphertext).is_none());
        let mut flipped = ciphertext.clone();
        flipped[0] ^= 1;
        assert!(open(&key, &nonce, b"aad", &flipped).is_none());
        let (second_nonce, _) = seal(&key, b"aad", b"payload").unwrap();
        assert_ne!(nonce, second_nonce, "nonces are random per seal");
    }

    #[test]
    fn derived_keys_are_domain_separated() {
        let root = Key::random().unwrap();
        let a = root.derive(&[b"aidex/vault/record/v1", b"sec_1"]).unwrap();
        let b = root.derive(&[b"aidex/vault/record/v1", b"sec_2"]).unwrap();
        let again = root.derive(&[b"aidex/vault/record/v1", b"sec_1"]).unwrap();
        assert!(a.ct_eq(&again));
        assert!(!a.ct_eq(&b));
        assert_eq!(format!("{a:?}"), "Key([REDACTED])");
    }

    #[test]
    fn transcripts_are_unambiguous_and_macs_verify() {
        let ab_c = Transcript::new("t").str("ab").str("c").finish();
        let a_bc = Transcript::new("t").str("a").str("bc").finish();
        assert_ne!(ab_c, a_bc);
        let key = Key::random().unwrap();
        let tag = mac(&key, &ab_c).unwrap();
        assert!(verify_mac(&key, &ab_c, &tag));
        assert!(!verify_mac(&key, &a_bc, &tag));
        assert!(!verify_mac(&key, &ab_c, &tag[..31]));
    }
}
