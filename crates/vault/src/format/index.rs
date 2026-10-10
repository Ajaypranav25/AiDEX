//! The secret index: names, kinds, timestamps, policies, and record locations,
//! sealed with an index key derived from the vault master key. It never holds values.

use super::{
    BlobId,
    header::{Header, SealedIndex},
};
use crate::{
    SecretKind, SecretMeta, SecretName, SecretPolicy, VaultError,
    crypto::{Key, open, seal},
    error::IntegrityTarget,
};
use aidex_core::{EntityId, SecretRef, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zeroize::Zeroizing;

pub(crate) const INDEX_KEY_INFO: &[u8] = b"aidex/vault/index/v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IndexEntry {
    pub(crate) record_id: EntityId,
    pub(crate) name: SecretName,
    pub(crate) kind: SecretKind,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) policy: SecretPolicy,
    pub(crate) blob: BlobId,
    pub(crate) revision: u64,
}

impl IndexEntry {
    pub(crate) fn meta(&self, vault_id: &EntityId) -> SecretMeta {
        SecretMeta {
            secret_ref: SecretRef {
                vault_id: vault_id.clone(),
                secret_id: self.record_id.clone(),
            },
            name: self.name.clone(),
            kind: self.kind,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            policy: self.policy.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Index {
    pub(crate) vault_id: EntityId,
    pub(crate) generation: u64,
    pub(crate) records: Vec<IndexEntry>,
}

impl Index {
    pub(crate) fn empty(vault_id: EntityId, generation: u64) -> Self {
        Self {
            vault_id,
            generation,
            records: Vec::new(),
        }
    }

    pub(crate) fn by_name(&self, name: &SecretName) -> Option<&IndexEntry> {
        self.records.iter().find(|entry| &entry.name == name)
    }

    pub(crate) fn by_record_id(&self, record_id: &EntityId) -> Option<&IndexEntry> {
        self.records
            .iter()
            .find(|entry| &entry.record_id == record_id)
    }

    pub(crate) fn seal(&self, key: &Key) -> Result<SealedIndex, VaultError> {
        let plaintext = Zeroizing::new(
            serde_json::to_vec(self)
                .map_err(|_| VaultError::Crypto("cannot encode the secret index"))?,
        );
        let aad = Header::index_aad(&self.vault_id, self.generation);
        let (nonce, ciphertext) = seal(key, &aad, &plaintext)?;
        Ok(SealedIndex {
            generation: self.generation,
            nonce,
            ciphertext,
        })
    }

    /// Decrypt and validate the index sealed in `header`.
    pub(crate) fn open(header: &Header, key: &Key) -> Result<Self, VaultError> {
        let sealed = &header.index;
        let aad = Header::index_aad(&header.vault_id, sealed.generation);
        let plaintext = open(key, &sealed.nonce, &aad, &sealed.ciphertext)
            .ok_or(VaultError::Integrity(IntegrityTarget::Index))?;
        // Authenticated but unparseable contents are corruption, not tampering.
        let index: Self = serde_json::from_slice(&plaintext)
            .map_err(|_| VaultError::CorruptHeader("secret index contents are invalid".into()))?;
        if index.vault_id != header.vault_id || index.generation != sealed.generation {
            return Err(VaultError::Integrity(IntegrityTarget::Index));
        }
        index.validate()?;
        Ok(index)
    }

    fn validate(&self) -> Result<(), VaultError> {
        let mut names = HashSet::new();
        let mut records = HashSet::new();
        let mut blobs = HashSet::new();
        for entry in &self.records {
            if entry.policy.validate().is_err()
                || !names.insert(&entry.name)
                || !records.insert(&entry.record_id)
                || !blobs.insert(&entry.blob)
            {
                return Err(VaultError::CorruptHeader(
                    "secret index has invalid or duplicate entries".into(),
                ));
            }
        }
        Ok(())
    }
}
