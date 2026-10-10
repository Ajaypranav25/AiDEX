//! `vault.json`: strict parsing, validation, serialization, and the canonical
//! transcripts that bind key slots and the header MAC to their context.

use super::{B64, FORMAT_NAME, FORMAT_VERSION};
use crate::{
    KdfParams, VaultError,
    crypto::{KEY_LEN, MAC_LEN, NONCE_LEN, SALT_LEN, TAG_LEN, Transcript},
    kdf::ARGON2_VERSION,
};
use aidex_core::{EntityId, Timestamp};
use serde::{Deserialize, Serialize};

const ARGON2ID: &str = "argon2id";
const HKDF_SHA256: &str = "hkdf-sha256";
const WRAPPED_KEY_LEN: usize = KEY_LEN + TAG_LEN;

/// Which secret a key slot is unlocked with.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SlotKind {
    Passphrase,
    Recovery,
}

impl SlotKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passphrase => "passphrase",
            Self::Recovery => "recovery",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SlotKdf {
    Argon2id(KdfParams),
    HkdfSha256,
}

/// A key slot: the vault master key wrapped under a key-encryption key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Slot {
    pub(crate) kind: SlotKind,
    pub(crate) kdf: SlotKdf,
    pub(crate) salt: [u8; SALT_LEN],
    pub(crate) nonce: [u8; NONCE_LEN],
    pub(crate) wrapped_key: Vec<u8>,
}

/// The sealed secret index carried inside the header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SealedIndex {
    pub(crate) generation: u64,
    pub(crate) nonce: [u8; NONCE_LEN],
    pub(crate) ciphertext: Vec<u8>,
}

/// A structurally valid header. Its MAC is verified only after unlock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Header {
    pub(crate) vault_id: EntityId,
    pub(crate) created_at: Timestamp,
    pub(crate) passphrase: Slot,
    pub(crate) recovery: Option<Slot>,
    pub(crate) index: SealedIndex,
    pub(crate) mac: [u8; MAC_LEN],
}

#[derive(Deserialize)]
struct Probe {
    format: Option<String>,
    format_version: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderFile {
    format: String,
    format_version: u32,
    vault_id: EntityId,
    created_at: Timestamp,
    slots: Vec<SlotFile>,
    index: IndexFile,
    mac: B64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotFile {
    kind: SlotKind,
    kdf: KdfFile,
    salt: B64,
    nonce: B64,
    wrapped_key: B64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KdfFile {
    algorithm: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memory_kib: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    iterations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parallelism: Option<u32>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexFile {
    generation: u64,
    nonce: B64,
    ciphertext: B64,
}

fn corrupt(detail: impl Into<String>) -> VaultError {
    VaultError::CorruptHeader(detail.into())
}

impl Header {
    /// Parse and structurally validate `vault.json`. Unknown versions are reported
    /// before strict parsing so newer vaults are never misread.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, VaultError> {
        let probe: Probe = serde_json::from_slice(bytes)
            .map_err(|_| corrupt("not a JSON object (truncated or damaged?)"))?;
        if probe.format.as_deref() != Some(FORMAT_NAME) {
            return Err(corrupt("not an AiDEX vault header"));
        }
        match probe.format_version {
            Some(version) if version == u64::from(FORMAT_VERSION) => {}
            Some(found) => return Err(VaultError::UnsupportedVersion { found }),
            None => return Err(corrupt("format_version is missing")),
        }
        let file: HeaderFile =
            serde_json::from_slice(bytes).map_err(|error| corrupt(error.to_string()))?;
        let mut passphrase = None;
        let mut recovery = None;
        for slot in file.slots {
            let slot = Slot::from_file(slot)?;
            let target = match slot.kind {
                SlotKind::Passphrase => &mut passphrase,
                SlotKind::Recovery => &mut recovery,
            };
            if target.replace(slot).is_some() {
                return Err(corrupt("duplicate key slot"));
            }
        }
        let passphrase = passphrase.ok_or_else(|| corrupt("passphrase key slot is missing"))?;
        let index = IndexFile::into_sealed(file.index)?;
        let mac = file
            .mac
            .array::<MAC_LEN>()
            .ok_or_else(|| corrupt("header MAC has the wrong length"))?;
        Ok(Self {
            vault_id: file.vault_id,
            created_at: file.created_at,
            passphrase,
            recovery,
            index,
            mac,
        })
    }

    /// Pretty JSON with a trailing newline.
    pub(crate) fn to_bytes(&self) -> Result<Vec<u8>, VaultError> {
        let slots = std::iter::once(&self.passphrase)
            .chain(self.recovery.as_ref())
            .map(Slot::to_file)
            .collect();
        let file = HeaderFile {
            format: FORMAT_NAME.to_owned(),
            format_version: FORMAT_VERSION,
            vault_id: self.vault_id.clone(),
            created_at: self.created_at.clone(),
            slots,
            index: IndexFile {
                generation: self.index.generation,
                nonce: B64(self.index.nonce.to_vec()),
                ciphertext: B64(self.index.ciphertext.clone()),
            },
            mac: B64(self.mac.to_vec()),
        };
        let mut bytes = serde_json::to_vec_pretty(&file)
            .map_err(|error| corrupt(format!("cannot encode header: {error}")))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// HMAC input: every header field except the MAC itself, in a fixed order.
    pub(crate) fn mac_input(&self) -> Vec<u8> {
        let slots: Vec<&Slot> = std::iter::once(&self.passphrase)
            .chain(self.recovery.as_ref())
            .collect();
        let mut transcript = Transcript::new("aidex/vault/header-mac/v1")
            .str(FORMAT_NAME)
            .u32(FORMAT_VERSION)
            .str(self.vault_id.as_str())
            .str(self.created_at.as_str())
            .u64(slots.len() as u64);
        for slot in slots {
            transcript = slot
                .describe(transcript)
                .bytes(&slot.nonce)
                .bytes(&slot.wrapped_key);
        }
        transcript
            .u64(self.index.generation)
            .bytes(&self.index.nonce)
            .bytes(&self.index.ciphertext)
            .finish()
    }

    /// Associated data for wrapping the master key in `slot`.
    pub(crate) fn slot_aad(
        vault_id: &EntityId,
        slot_kind: SlotKind,
        kdf: &SlotKdf,
        salt: &[u8],
    ) -> Vec<u8> {
        let transcript = Transcript::new("aidex/vault/slot/v1")
            .u32(FORMAT_VERSION)
            .str(vault_id.as_str());
        describe_slot(transcript, slot_kind, kdf, salt).finish()
    }

    /// Associated data for the sealed index of `generation`.
    pub(crate) fn index_aad(vault_id: &EntityId, generation: u64) -> Vec<u8> {
        Transcript::new("aidex/vault/index/v1")
            .u32(FORMAT_VERSION)
            .str(vault_id.as_str())
            .u64(generation)
            .finish()
    }
}

fn describe_slot(transcript: Transcript, kind: SlotKind, kdf: &SlotKdf, salt: &[u8]) -> Transcript {
    let transcript = transcript.str(kind.as_str());
    let transcript = match kdf {
        SlotKdf::Argon2id(params) => transcript
            .str(ARGON2ID)
            .u32(ARGON2_VERSION)
            .u32(params.memory_kib())
            .u32(params.iterations())
            .u32(params.parallelism()),
        SlotKdf::HkdfSha256 => transcript.str(HKDF_SHA256),
    };
    transcript.bytes(salt)
}

impl Slot {
    fn describe(&self, transcript: Transcript) -> Transcript {
        describe_slot(transcript, self.kind, &self.kdf, &self.salt)
    }

    fn from_file(file: SlotFile) -> Result<Self, VaultError> {
        let kdf = match (file.kind, file.kdf.algorithm.as_str()) {
            (SlotKind::Passphrase, ARGON2ID) => {
                let KdfFile {
                    version: Some(ARGON2_VERSION),
                    memory_kib: Some(memory_kib),
                    iterations: Some(iterations),
                    parallelism: Some(parallelism),
                    ..
                } = file.kdf
                else {
                    return Err(corrupt(
                        "argon2id slot parameters are incomplete or unsupported",
                    ));
                };
                SlotKdf::Argon2id(
                    KdfParams::new(memory_kib, iterations, parallelism)
                        .map_err(|error| corrupt(error.to_string()))?,
                )
            }
            (SlotKind::Recovery, HKDF_SHA256) => {
                let kdf = &file.kdf;
                if kdf.version.is_some()
                    || kdf.memory_kib.is_some()
                    || kdf.iterations.is_some()
                    || kdf.parallelism.is_some()
                {
                    return Err(corrupt("hkdf-sha256 slots take no parameters"));
                }
                SlotKdf::HkdfSha256
            }
            _ => {
                return Err(corrupt(
                    "key slot uses an unsupported key-derivation function",
                ));
            }
        };
        let salt = file
            .salt
            .array::<SALT_LEN>()
            .ok_or_else(|| corrupt("key slot salt has the wrong length"))?;
        let nonce = file
            .nonce
            .array::<NONCE_LEN>()
            .ok_or_else(|| corrupt("key slot nonce has the wrong length"))?;
        if file.wrapped_key.0.len() != WRAPPED_KEY_LEN {
            return Err(corrupt("wrapped key has the wrong length"));
        }
        Ok(Self {
            kind: file.kind,
            kdf,
            salt,
            nonce,
            wrapped_key: file.wrapped_key.0,
        })
    }

    fn to_file(&self) -> SlotFile {
        let kdf = match self.kdf {
            SlotKdf::Argon2id(params) => KdfFile {
                algorithm: ARGON2ID.to_owned(),
                version: Some(ARGON2_VERSION),
                memory_kib: Some(params.memory_kib()),
                iterations: Some(params.iterations()),
                parallelism: Some(params.parallelism()),
            },
            SlotKdf::HkdfSha256 => KdfFile {
                algorithm: HKDF_SHA256.to_owned(),
                version: None,
                memory_kib: None,
                iterations: None,
                parallelism: None,
            },
        };
        SlotFile {
            kind: self.kind,
            kdf,
            salt: B64(self.salt.to_vec()),
            nonce: B64(self.nonce.to_vec()),
            wrapped_key: B64(self.wrapped_key.clone()),
        }
    }
}

impl IndexFile {
    fn into_sealed(self) -> Result<SealedIndex, VaultError> {
        let nonce = self
            .nonce
            .array::<NONCE_LEN>()
            .ok_or_else(|| corrupt("index nonce has the wrong length"))?;
        if self.ciphertext.0.len() < TAG_LEN {
            return Err(corrupt("sealed index is truncated"));
        }
        Ok(SealedIndex {
            generation: self.generation,
            nonce,
            ciphertext: self.ciphertext.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VaultErrorKind;

    fn sample() -> Header {
        let slot = |kind, kdf| Slot {
            kind,
            kdf,
            salt: [1; SALT_LEN],
            nonce: [2; NONCE_LEN],
            wrapped_key: vec![3; WRAPPED_KEY_LEN],
        };
        Header {
            vault_id: EntityId::new("vault_test").unwrap(),
            created_at: Timestamp::from_unix_millis(0).unwrap(),
            passphrase: slot(
                SlotKind::Passphrase,
                SlotKdf::Argon2id(KdfParams::default()),
            ),
            recovery: Some(slot(SlotKind::Recovery, SlotKdf::HkdfSha256)),
            index: SealedIndex {
                generation: 4,
                nonce: [5; NONCE_LEN],
                ciphertext: vec![6; 40],
            },
            mac: [7; MAC_LEN],
        }
    }

    #[test]
    fn headers_round_trip_through_json() {
        let header = sample();
        let bytes = header.to_bytes().unwrap();
        assert_eq!(Header::parse(&bytes).unwrap(), header);
        let mut without_recovery = header.clone();
        without_recovery.recovery = None;
        let bytes = without_recovery.to_bytes().unwrap();
        assert_eq!(Header::parse(&bytes).unwrap(), without_recovery);
    }

    #[test]
    fn mac_input_covers_every_field() {
        let base = sample();
        let mut variants = Vec::new();
        let mut changed = base.clone();
        changed.vault_id = EntityId::new("vault_other").unwrap();
        variants.push(changed);
        let mut changed = base.clone();
        changed.created_at = Timestamp::from_unix_millis(1).unwrap();
        variants.push(changed);
        let mut changed = base.clone();
        changed.recovery = None;
        variants.push(changed);
        let mut changed = base.clone();
        changed.passphrase.kdf = SlotKdf::Argon2id(KdfParams::new(8, 1, 1).unwrap());
        variants.push(changed);
        let mut changed = base.clone();
        changed.passphrase.wrapped_key[0] ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.index.generation += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.index.ciphertext.push(0);
        variants.push(changed);
        for variant in variants {
            assert_ne!(variant.mac_input(), base.mac_input());
        }
        let mut mac_only = base.clone();
        mac_only.mac = [0; MAC_LEN];
        assert_eq!(mac_only.mac_input(), base.mac_input());
    }

    #[test]
    fn malformed_and_unknown_headers_fail_explicitly() {
        let good = String::from_utf8(sample().to_bytes().unwrap()).unwrap();
        let cases = [
            (String::new(), VaultErrorKind::Corrupt),
            (good[..good.len() / 2].to_owned(), VaultErrorKind::Corrupt),
            ("[]".to_owned(), VaultErrorKind::Corrupt),
            (
                r#"{"format":"other","format_version":1}"#.to_owned(),
                VaultErrorKind::Corrupt,
            ),
            (
                good.replace("\"format_version\": 1", "\"format_version\": 2"),
                VaultErrorKind::UnsupportedVersion,
            ),
            (
                good.replace("\"generation\": 4", "\"generation\": 4, \"extra\": 1"),
                VaultErrorKind::Corrupt,
            ),
            (
                good.replace("\"recovery\"", "\"passphrase\""),
                VaultErrorKind::Corrupt,
            ),
            (
                good.replace("\"hkdf-sha256\"", "\"scrypt\""),
                VaultErrorKind::Corrupt,
            ),
            (
                good.replace("\"memory_kib\": 65536", "\"memory_kib\": 4294967295"),
                VaultErrorKind::Corrupt,
            ),
        ];
        for (text, expected) in cases {
            let error = Header::parse(text.as_bytes()).unwrap_err();
            assert_eq!(error.kind(), expected, "{text}");
        }
    }
}
