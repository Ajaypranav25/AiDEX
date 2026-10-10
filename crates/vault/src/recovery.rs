//! Recovery keys: 256 random bits shown once as Crockford base32 groups with a
//! 24-bit checksum, e.g. `7K3M-Q9ZD-...` (14 groups of 4 characters).

use crate::{
    SecretString, VaultError,
    crypto::{KEY_LEN, fill_random, sha256},
    secret::REDACTED,
};
use std::fmt;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CHECK_LEN: usize = 3;
const ENCODED_BYTES: usize = KEY_LEN + CHECK_LEN;
/// 35 bytes = 280 bits = exactly 56 base32 characters.
const ENCODED_CHARS: usize = ENCODED_BYTES * 8 / 5;
const GROUP_LEN: usize = 4;
const CHECK_LABEL: &[u8] = b"aidex/vault/recovery-key-check/v1";

/// A 256-bit recovery key that unwraps the vault master key independently of the
/// passphrase. Zeroized on drop; `Debug` is redacted and there is no `Display`.
pub struct RecoveryKey(Zeroizing<[u8; KEY_LEN]>);

impl RecoveryKey {
    pub(crate) fn generate() -> Result<Self, VaultError> {
        let mut bytes = Zeroizing::new([0_u8; KEY_LEN]);
        fill_random(bytes.as_mut())?;
        Ok(Self(bytes))
    }

    pub(crate) fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// Parse the grouped form. Case, `-`, and whitespace are ignored; Crockford
    /// aliases `O`→`0` and `I`/`L`→`1` are accepted. A wrong checksum (a typo) is
    /// reported as [`VaultError::InvalidRecoveryKey`], not as a wrong key.
    pub fn parse(text: &str) -> Result<Self, VaultError> {
        let mut values = Zeroizing::new(Vec::with_capacity(ENCODED_CHARS));
        for ch in text.chars() {
            if ch == '-' || ch.is_whitespace() {
                continue;
            }
            let ch = match ch.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                other => other,
            };
            let value = ALPHABET
                .iter()
                .position(|&symbol| char::from(symbol) == ch)
                .ok_or(VaultError::InvalidRecoveryKey(
                    "contains characters outside the recovery-key alphabet",
                ))?;
            if values.len() == ENCODED_CHARS {
                return Err(VaultError::InvalidRecoveryKey("is too long"));
            }
            values.push(value as u8);
        }
        if values.len() != ENCODED_CHARS {
            return Err(VaultError::InvalidRecoveryKey("is too short"));
        }
        let mut decoded = Zeroizing::new([0_u8; ENCODED_BYTES]);
        let (mut buffer, mut bits, mut position) = (0_u32, 0_u32, 0_usize);
        for &value in values.iter() {
            buffer = (buffer << 5) | u32::from(value);
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                decoded[position] = (buffer >> bits) as u8;
                position += 1;
                buffer &= (1 << bits) - 1;
            }
        }
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);
        key.copy_from_slice(&decoded[..KEY_LEN]);
        let expected = checksum(&key);
        if !bool::from(decoded[KEY_LEN..].ct_eq(&expected)) {
            return Err(VaultError::InvalidRecoveryKey(
                "checksum mismatch; check for typos",
            ));
        }
        Ok(Self(key))
    }

    /// The grouped text form to show the user exactly once.
    pub fn encode(&self) -> SecretString {
        let mut bytes = Zeroizing::new([0_u8; ENCODED_BYTES]);
        bytes[..KEY_LEN].copy_from_slice(self.0.as_slice());
        bytes[KEY_LEN..].copy_from_slice(&checksum(&self.0));
        let mut text = String::with_capacity(ENCODED_CHARS + ENCODED_CHARS / GROUP_LEN);
        let (mut buffer, mut bits, mut written) = (0_u32, 0_u32, 0_usize);
        for &byte in bytes.iter() {
            buffer = (buffer << 8) | u32::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                if written > 0 && written % GROUP_LEN == 0 {
                    text.push('-');
                }
                text.push(char::from(ALPHABET[((buffer >> bits) & 0x1f) as usize]));
                written += 1;
                buffer &= (1 << bits) - 1;
            }
        }
        SecretString::new(text)
    }
}

fn checksum(key: &[u8; KEY_LEN]) -> [u8; CHECK_LEN] {
    let mut input = Zeroizing::new(Vec::with_capacity(CHECK_LABEL.len() + KEY_LEN));
    input.extend_from_slice(CHECK_LABEL);
    input.extend_from_slice(key);
    let digest = sha256(&input);
    [digest[0], digest[1], digest[2]]
}

impl fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RecoveryKey({REDACTED})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VaultErrorKind;

    #[test]
    fn encoding_is_grouped_and_round_trips_with_aliases() {
        let key = RecoveryKey::generate().unwrap();
        let text = key.encode();
        let groups: Vec<&str> = text.expose().split('-').collect();
        assert_eq!(groups.len(), 14);
        assert!(groups.iter().all(|group| group.len() == 4));
        let parsed = RecoveryKey::parse(text.expose()).unwrap();
        assert_eq!(parsed.bytes(), key.bytes());
        let relaxed = text
            .expose()
            .to_ascii_lowercase()
            .replace('-', " ")
            .replace('0', "o")
            .replace('1', "l");
        assert_eq!(RecoveryKey::parse(&relaxed).unwrap().bytes(), key.bytes());
        assert_eq!(format!("{key:?}"), "RecoveryKey([REDACTED])");
    }

    #[test]
    fn typos_and_malformed_text_are_invalid_not_wrong() {
        // A fixed key keeps the single-character typo deterministic (no 2^-24 checksum collision).
        let key = RecoveryKey(Zeroizing::new([42_u8; KEY_LEN]));
        let text = key.encode().expose().to_owned();
        let mut typo: Vec<char> = text.chars().collect();
        typo[0] = if typo[0] == 'A' { 'B' } else { 'A' };
        let typo: String = typo.into_iter().collect();
        for bad in [
            typo.as_str(),
            &text[..text.len() - 1],
            &format!("{text}A"),
            "U000-0000",
            "",
        ] {
            let error = RecoveryKey::parse(bad).unwrap_err();
            assert_eq!(error.kind(), VaultErrorKind::InvalidRecoveryKey, "{bad}");
        }
    }
}
