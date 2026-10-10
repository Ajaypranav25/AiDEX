//! Record files: one sealed, length-padded secret value per file.
//!
//! The record key is HKDF-SHA256(VMK, "aidex/vault/record/v1" || record_id) and the
//! associated data binds the format version, vault id, record id, revision, and blob
//! id, so a record cannot be moved to another name, revision, file, or vault.

use super::{B64, BlobId, FORMAT_VERSION};
use crate::{
    SecretBytes, VaultError,
    crypto::{Key, NONCE_LEN, TAG_LEN, Transcript, open, seal},
    error::IntegrityTarget,
};
use aidex_core::EntityId;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const RECORD_KEY_INFO: &[u8] = b"aidex/vault/record/v1";
/// Plaintexts are padded to a multiple of this many bytes to hide exact value lengths.
const PADDING_BLOCK: usize = 256;
const LENGTH_PREFIX: usize = 4;

/// Where a record lives and which revision the index expects.
pub(crate) struct RecordLocation<'a> {
    pub(crate) vault_id: &'a EntityId,
    pub(crate) record_id: &'a EntityId,
    pub(crate) revision: u64,
    pub(crate) blob: &'a BlobId,
}

impl RecordLocation<'_> {
    fn aad(&self) -> Vec<u8> {
        Transcript::new("aidex/vault/record/v1")
            .u32(FORMAT_VERSION)
            .str(self.vault_id.as_str())
            .str(self.record_id.as_str())
            .u64(self.revision)
            .str(self.blob.as_str())
            .finish()
    }

    fn key(&self, master: &Key) -> Result<Key, VaultError> {
        master.derive(&[RECORD_KEY_INFO, self.record_id.as_str().as_bytes()])
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordFile {
    format_version: u32,
    nonce: B64,
    ciphertext: B64,
}

/// Seal `value` for `location`; returns the record file bytes.
pub(crate) fn seal_record(
    master: &Key,
    location: &RecordLocation<'_>,
    value: &SecretBytes,
) -> Result<Vec<u8>, VaultError> {
    let length =
        u32::try_from(value.len()).map_err(|_| VaultError::InvalidSecret("value is too large"))?;
    let padded = (LENGTH_PREFIX + value.len()).div_ceil(PADDING_BLOCK) * PADDING_BLOCK;
    let mut plaintext = Zeroizing::new(Vec::with_capacity(padded));
    plaintext.extend_from_slice(&length.to_le_bytes());
    plaintext.extend_from_slice(value.expose());
    plaintext.resize(padded, 0);
    let (nonce, ciphertext) = seal(&location.key(master)?, &location.aad(), &plaintext)?;
    let file = RecordFile {
        format_version: FORMAT_VERSION,
        nonce: B64(nonce.to_vec()),
        ciphertext: B64(ciphertext),
    };
    let mut bytes =
        serde_json::to_vec(&file).map_err(|_| VaultError::Crypto("cannot encode a record file"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Authenticate and decrypt the record file `bytes` expected at `location`.
pub(crate) fn open_record(
    master: &Key,
    location: &RecordLocation<'_>,
    bytes: &[u8],
) -> Result<SecretBytes, VaultError> {
    let corrupt = |detail: &str| VaultError::CorruptRecord(detail.to_owned());
    let file: RecordFile = serde_json::from_slice(bytes)
        .map_err(|_| corrupt("record file is not valid (truncated or damaged?)"))?;
    if file.format_version != FORMAT_VERSION {
        return Err(VaultError::UnsupportedVersion {
            found: u64::from(file.format_version),
        });
    }
    let nonce = file
        .nonce
        .array::<NONCE_LEN>()
        .ok_or_else(|| corrupt("record nonce has the wrong length"))?;
    if file.ciphertext.0.len() < TAG_LEN + PADDING_BLOCK {
        return Err(corrupt("record ciphertext is truncated"));
    }
    let plaintext = open(
        &location.key(master)?,
        &nonce,
        &location.aad(),
        &file.ciphertext.0,
    )
    .ok_or(VaultError::Integrity(IntegrityTarget::Record))?;
    let prefix: [u8; LENGTH_PREFIX] = plaintext
        .get(..LENGTH_PREFIX)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| corrupt("record plaintext is truncated"))?;
    let length = u32::from_le_bytes(prefix) as usize;
    let end = LENGTH_PREFIX
        .checked_add(length)
        .filter(|&end| end <= plaintext.len())
        .ok_or_else(|| corrupt("record length prefix is out of range"))?;
    if plaintext[end..].iter().any(|&byte| byte != 0) {
        return Err(corrupt("record padding is not zero"));
    }
    Ok(SecretBytes::new(plaintext[LENGTH_PREFIX..end].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_hide_exact_lengths_and_bind_their_location() {
        let master = Key::random().unwrap();
        let vault_id = EntityId::new("vault_a").unwrap();
        let record_id = EntityId::new("sec_a").unwrap();
        let blob = BlobId::generate().unwrap();
        let location = RecordLocation {
            vault_id: &vault_id,
            record_id: &record_id,
            revision: 1,
            blob: &blob,
        };
        let short = seal_record(&master, &location, &SecretBytes::from(b"k".to_vec())).unwrap();
        let longer = seal_record(&master, &location, &SecretBytes::from(vec![b'k'; 200])).unwrap();
        assert_eq!(short.len(), longer.len(), "both pad to one 256-byte block");
        let value = open_record(&master, &location, &short).unwrap();
        assert_eq!(value.expose(), b"k");

        let other_record = EntityId::new("sec_b").unwrap();
        let other_vault = EntityId::new("vault_b").unwrap();
        let other_blob = BlobId::generate().unwrap();
        for wrong in [
            RecordLocation {
                record_id: &other_record,
                ..location
            },
            RecordLocation {
                vault_id: &other_vault,
                ..location
            },
            RecordLocation {
                revision: 2,
                ..location
            },
            RecordLocation {
                blob: &other_blob,
                ..location
            },
        ] {
            assert!(matches!(
                open_record(&master, &wrong, &short),
                Err(VaultError::Integrity(IntegrityTarget::Record))
            ));
        }
        assert!(matches!(
            open_record(&master, &location, &short[..short.len() / 2]),
            Err(VaultError::CorruptRecord(_))
        ));
        let empty = seal_record(&master, &location, &SecretBytes::from(Vec::new())).unwrap();
        assert!(open_record(&master, &location, &empty).unwrap().is_empty());
    }
}
