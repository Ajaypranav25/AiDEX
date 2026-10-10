//! On-disk vault format version 1 (documented in `crates/vault/FORMAT.md`).
//!
//! - `vault.json`: the header — key slots wrapping the vault master key, the sealed
//!   secret index, and an HMAC over every field.
//! - `records/<blob>.json`: one sealed secret value per file, referenced by the index.

pub(crate) mod header;
pub(crate) mod index;
pub(crate) mod record;

use crate::{VaultError, crypto::random_array};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;

/// The only on-disk format version this build reads and writes.
pub const FORMAT_VERSION: u32 = 1;
pub(crate) const FORMAT_NAME: &str = "aidex-vault";
pub(crate) const HEADER_FILE: &str = "vault.json";
pub(crate) const RECORDS_DIR: &str = "records";
pub(crate) const RECORD_SUFFIX: &str = ".json";
/// Header files above this size are rejected before parsing.
pub(crate) const MAX_HEADER_BYTES: u64 = 8 * 1024 * 1024;
/// Record files above this size are rejected before parsing.
pub(crate) const MAX_RECORD_FILE_BYTES: u64 = 256 * 1024;

/// Canonical base64 (standard alphabet, padded) for binary JSON fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct B64(pub(crate) Vec<u8>);

impl Serialize for B64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for B64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD
            .decode(text.as_bytes())
            .map(Self)
            .map_err(|_| de::Error::custom("invalid base64"))
    }
}

impl B64 {
    /// The bytes as a fixed-size array, or `None` for any other length.
    pub(crate) fn array<const N: usize>(&self) -> Option<[u8; N]> {
        <[u8; N]>::try_from(self.0.as_slice()).ok()
    }
}

/// Random, meaningless record file name: 128 bits as 32 lowercase hex characters.
/// File names carry no secret name, record id, or timestamp.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct BlobId(String);

impl BlobId {
    pub(crate) fn generate() -> Result<Self, VaultError> {
        let bytes = random_array::<16>()?;
        Ok(Self(
            bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        ))
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        (text.len() == 32 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
            .then(|| Self(text.to_owned()))
    }

    /// Recognise `<blob>.json` record file names.
    pub(crate) fn from_file_name(name: &str) -> Option<Self> {
        name.strip_suffix(RECORD_SUFFIX).and_then(Self::parse)
    }

    pub(crate) fn file_name(&self) -> String {
        format!("{}{RECORD_SUFFIX}", self.0)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for BlobId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for BlobId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| de::Error::custom("invalid record blob id"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_ids_are_random_hex_file_names() {
        let a = BlobId::generate().unwrap();
        let b = BlobId::generate().unwrap();
        assert_ne!(a, b);
        assert_eq!(BlobId::from_file_name(&a.file_name()), Some(a.clone()));
        for bad in ["", "ABCDEF", &"g".repeat(32), &format!("{a}.tmp"), "x.json"] {
            assert_eq!(BlobId::from_file_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn base64_fields_are_canonical() {
        let value = B64(vec![1, 2, 3]);
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, "\"AQID\"");
        assert_eq!(serde_json::from_str::<B64>(&json).unwrap(), value);
        for bad in ["\"AQI\"", "\"AQID=\"", "\"A!ID\"", "\"AQJ=\""] {
            assert!(serde_json::from_str::<B64>(bad).is_err(), "{bad}");
        }
        assert_eq!(value.array::<3>(), Some([1, 2, 3]));
        assert_eq!(value.array::<4>(), None);
    }
}
