//! The trust store: which Ed25519 keys may sign bundles, and which are revoked.
//!
//! Sources, in the order [`TrustStore::builtin`] reads them:
//!
//! 1. `release-keys.json`, compiled into every binary (release public keys with
//!    validity windows, and compiled-in revocations). It holds no keys until the
//!    project provisions its release key; see `portable/packaging/README.md`.
//! 2. `AIDEX_BUILD_TRUSTED_KEYS` at **compile time** (`option_env!`): the packager sets it
//!    from `build --trust-key <file.pub.json>` so CI and developer builds trust their
//!    own (ephemeral or development) keys. Nothing is read from the run-time environment.
//! 3. `AIDEX_DEV_TRUSTED_KEYS` at **run time**, only in builds with the `dev-trust`
//!    feature, which only test builds enable (through dev-dependencies). Release
//!    bundles never honour it.
//!
//! Revocations are additive: the compiled-in list plus the drive's
//! `manifests/revoked-keys.json`. A revoked key is refused even if it is also trusted.

use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
};

use aidex_core::Timestamp;
use aidex_drive::DrivePaths;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::{
    BundleSignature,
    keys::{key_id_for, parse_public_key, validate_key_name},
    signed_message,
};
use crate::{
    BundleError, DocumentError, SignatureError,
    digest::{from_hex, is_lower_hex, to_hex},
    fsutil::write_atomic,
    manifest::parse_versioned,
};

/// Compile-time variable with extra trusted keys (`name=hex,...` or `hex,...`).
pub const BUILD_TRUSTED_KEYS_ENV: &str = "AIDEX_BUILD_TRUSTED_KEYS";
/// Run-time variable with extra trusted keys; honoured only with the `dev-trust` feature.
pub const DEV_TRUSTED_KEYS_ENV: &str = "AIDEX_DEV_TRUSTED_KEYS";
/// File name of the drive's revocation list inside `manifests/`.
pub const REVOCATIONS_FILE_NAME: &str = "revoked-keys.json";
const REVOCATIONS_SCHEMA_VERSION: u32 = 1;
const MAX_REVOCATIONS_BYTES: u64 = 1024 * 1024;
const RELEASE_KEYS: &str = include_str!("release-keys.json");

/// Where a trusted key came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    /// `release-keys.json`, compiled into the binary.
    Release,
    /// `AIDEX_BUILD_TRUSTED_KEYS` when the binary was compiled (CI or developer key).
    Build,
    /// `AIDEX_DEV_TRUSTED_KEYS` at run time (test builds only).
    Development,
    /// Added by the caller (`--trust-key`, tests).
    Explicit,
}

/// One trusted public key.
#[derive(Clone, Eq, PartialEq)]
pub struct TrustedKey {
    key_id: String,
    name: String,
    public_key: [u8; 32],
    not_before: Option<Timestamp>,
    not_after: Option<Timestamp>,
    source: KeySource,
}

impl TrustedKey {
    /// A key from its 32 public-key bytes; fails for malformed names or keys.
    pub fn new(name: &str, public_key: [u8; 32], source: KeySource) -> Result<Self, String> {
        validate_key_name(name)?;
        VerifyingKey::from_bytes(&public_key)
            .map_err(|_| format!("key `{name}` is not a valid Ed25519 public key"))?;
        Ok(Self::from_parts(name.to_owned(), public_key, source))
    }

    pub(crate) fn from_parts(name: String, public_key: [u8; 32], source: KeySource) -> Self {
        Self {
            key_id: key_id_for(&public_key),
            name,
            public_key,
            not_before: None,
            not_after: None,
            source,
        }
    }

    /// Restrict the key to bundles created inside `[not_before, not_after]`.
    pub fn with_validity(
        mut self,
        not_before: Option<Timestamp>,
        not_after: Option<Timestamp>,
    ) -> Self {
        self.not_before = not_before;
        self.not_after = not_after;
        self
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn source(&self) -> KeySource {
        self.source
    }

    pub fn not_before(&self) -> Option<&Timestamp> {
        self.not_before.as_ref()
    }

    pub fn not_after(&self) -> Option<&Timestamp> {
        self.not_after.as_ref()
    }

    pub fn public_key_hex(&self) -> String {
        to_hex(&self.public_key)
    }

    fn valid_at(&self, created_at: &Timestamp) -> bool {
        self.not_before
            .as_ref()
            .is_none_or(|start| start <= created_at)
            && self.not_after.as_ref().is_none_or(|end| created_at <= end)
    }
}

impl fmt::Debug for TrustedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrustedKey")
            .field("key_id", &self.key_id)
            .field("name", &self.name)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// A revoked key id.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revocation {
    pub key_id: String,
    #[serde(default)]
    pub revoked_at: Option<Timestamp>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// `manifests/revoked-keys.json`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevocationList {
    pub schema_version: u32,
    pub revoked: Vec<Revocation>,
}

impl Default for RevocationList {
    fn default() -> Self {
        Self {
            schema_version: REVOCATIONS_SCHEMA_VERSION,
            revoked: Vec::new(),
        }
    }
}

impl RevocationList {
    pub fn from_json(bytes: &[u8]) -> Result<Self, DocumentError> {
        let list: Self = parse_versioned(bytes, REVOCATIONS_SCHEMA_VERSION)?;
        for revocation in &list.revoked {
            validate_revocation(revocation)?;
        }
        Ok(list)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, DocumentError> {
        for revocation in &self.revoked {
            validate_revocation(revocation)?;
        }
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| DocumentError::Malformed(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Read the list; `Ok(None)` when the file does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>, BundleError> {
        let invalid = |reason: String| {
            BundleError::Trust(format!("revocation list `{}` is {reason}", path.display()))
        };
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(BundleError::io(
                    format!("cannot inspect `{}`", path.display()),
                    error,
                ));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(invalid("not a regular file".into()));
        }
        if metadata.len() > MAX_REVOCATIONS_BYTES {
            return Err(invalid(format!(
                "larger than {MAX_REVOCATIONS_BYTES} bytes"
            )));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .and_then(|file| file.take(MAX_REVOCATIONS_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|error| BundleError::io(format!("cannot read `{}`", path.display()), error))?;
        Self::from_json(&bytes)
            .map(Some)
            .map_err(|error| invalid(error.to_string()))
    }

    /// Atomically replace the file at `path`.
    pub fn store(&self, path: &Path) -> Result<(), BundleError> {
        let bytes = self
            .to_json()
            .map_err(|error| BundleError::Trust(format!("cannot encode revocations: {error}")))?;
        write_atomic(path, &bytes)
    }

    /// Add `revocation` unless its key id is already listed. Returns whether it was added.
    pub fn add(&mut self, revocation: Revocation) -> bool {
        if self
            .revoked
            .iter()
            .any(|existing| existing.key_id == revocation.key_id)
        {
            return false;
        }
        self.revoked.push(revocation);
        self.revoked.sort_by(|a, b| a.key_id.cmp(&b.key_id));
        true
    }
}

fn validate_revocation(revocation: &Revocation) -> Result<(), DocumentError> {
    if !is_lower_hex(&revocation.key_id, 8) {
        return Err(DocumentError::Invalid(format!(
            "revoked key_id `{}` must be 16 lowercase hex digits",
            revocation.key_id
        )));
    }
    if revocation
        .reason
        .as_deref()
        .is_some_and(|reason| reason.len() > 512 || reason.chars().any(char::is_control))
    {
        return Err(DocumentError::Invalid(
            "revocation reasons are at most 512 bytes without control characters".into(),
        ));
    }
    Ok(())
}

/// The drive's revocation list, `manifests/revoked-keys.json`.
pub fn revocations_path(paths: &DrivePaths) -> PathBuf {
    paths.manifests().join(REVOCATIONS_FILE_NAME)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseKeys {
    schema_version: u32,
    keys: Vec<ReleaseKey>,
    revoked: Vec<Revocation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseKey {
    key_id: String,
    name: String,
    public_key: String,
    not_before: Option<Timestamp>,
    not_after: Option<Timestamp>,
}

/// Trusted signing keys and revocations.
#[derive(Clone, Debug, Default)]
pub struct TrustStore {
    keys: BTreeMap<String, TrustedKey>,
    revoked: BTreeMap<String, Revocation>,
}

impl TrustStore {
    /// No keys at all: every signed bundle is refused as signed by an unknown key.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The keys this binary was built to trust (see the module documentation).
    pub fn builtin() -> Result<Self, BundleError> {
        let mut store = Self::from_release_document(RELEASE_KEYS)?;
        if let Some(text) = option_env!("AIDEX_BUILD_TRUSTED_KEYS") {
            for key in parse_key_list(text, KeySource::Build)
                .map_err(|error| BundleError::Trust(format!("{BUILD_TRUSTED_KEYS_ENV}: {error}")))?
            {
                store.add_key(key);
            }
        }
        #[cfg(feature = "dev-trust")]
        if let Ok(text) = std::env::var(DEV_TRUSTED_KEYS_ENV) {
            for key in parse_key_list(&text, KeySource::Development)
                .map_err(|error| BundleError::Trust(format!("{DEV_TRUSTED_KEYS_ENV}: {error}")))?
            {
                store.add_key(key);
            }
        }
        Ok(store)
    }

    /// Whether this build honours `AIDEX_DEV_TRUSTED_KEYS` (test builds only).
    pub const fn dev_trust_enabled() -> bool {
        cfg!(feature = "dev-trust")
    }

    fn from_release_document(text: &str) -> Result<Self, BundleError> {
        let invalid = |reason: String| BundleError::Trust(format!("release-keys.json: {reason}"));
        let document: ReleaseKeys =
            serde_json::from_str(text).map_err(|error| invalid(error.to_string()))?;
        if document.schema_version != 1 {
            return Err(invalid("unsupported schema_version".into()));
        }
        let mut store = Self::empty();
        for entry in document.keys {
            let public_key = parse_public_key(&entry.public_key).map_err(&invalid)?;
            let key = TrustedKey::new(&entry.name, public_key, KeySource::Release)
                .map_err(&invalid)?
                .with_validity(entry.not_before, entry.not_after);
            if key.key_id() != entry.key_id {
                return Err(invalid(format!(
                    "key_id {} does not match key `{}`",
                    entry.key_id, entry.name
                )));
            }
            store.add_key(key);
        }
        for revocation in document.revoked {
            validate_revocation(&revocation).map_err(|error| invalid(error.to_string()))?;
            store.revoke(revocation);
        }
        Ok(store)
    }

    pub fn with_key(mut self, key: TrustedKey) -> Self {
        self.add_key(key);
        self
    }

    /// Trust `key` (replacing an entry with the same key id).
    pub fn add_key(&mut self, key: TrustedKey) {
        self.keys.insert(key.key_id.clone(), key);
    }

    /// Refuse `revocation.key_id` from now on. Revocations are never removed.
    pub fn revoke(&mut self, revocation: Revocation) {
        self.revoked
            .entry(revocation.key_id.clone())
            .or_insert(revocation);
    }

    /// Add the revocations listed in the drive's `manifests/revoked-keys.json`.
    pub fn with_drive_revocations(mut self, paths: &DrivePaths) -> Result<Self, BundleError> {
        if let Some(list) = RevocationList::load(&revocations_path(paths))? {
            for revocation in list.revoked {
                self.revoke(revocation);
            }
        }
        Ok(self)
    }

    pub fn keys(&self) -> impl Iterator<Item = &TrustedKey> {
        self.keys.values()
    }

    pub fn key(&self, key_id: &str) -> Option<&TrustedKey> {
        self.keys.get(key_id)
    }

    pub fn revocations(&self) -> impl Iterator<Item = &Revocation> {
        self.revoked.values()
    }

    pub fn revocation(&self, key_id: &str) -> Option<&Revocation> {
        self.revoked.get(key_id)
    }

    /// Verify `signature` over `manifest_bytes` for a bundle created at `created_at`.
    /// Revocation is checked first, then trust, the validity window, and the signature.
    pub fn verify(
        &self,
        manifest_bytes: &[u8],
        signature: &BundleSignature,
        created_at: &Timestamp,
    ) -> Result<&TrustedKey, SignatureError> {
        let key_id = signature.key_id.clone();
        if let Some(revocation) = self.revoked.get(&key_id) {
            return Err(SignatureError::RevokedKey {
                key_id,
                reason: revocation.reason.clone(),
            });
        }
        let key = self
            .keys
            .get(&key_id)
            .ok_or_else(|| SignatureError::UnknownKey {
                key_id: key_id.clone(),
            })?;
        if !key.valid_at(created_at) {
            return Err(SignatureError::KeyNotValid {
                key_id,
                created_at: created_at.as_str().to_owned(),
                window: format!(
                    "from {} to {}",
                    key.not_before
                        .as_ref()
                        .map_or("the beginning", Timestamp::as_str),
                    key.not_after.as_ref().map_or("no end", Timestamp::as_str)
                ),
            });
        }
        let bad = || SignatureError::BadSignature {
            key_id: key_id.clone(),
        };
        let verifying = VerifyingKey::from_bytes(&key.public_key).map_err(|_| bad())?;
        let bytes = from_hex(&signature.signature)
            .and_then(|bytes| <[u8; 64]>::try_from(bytes).ok())
            .ok_or_else(bad)?;
        verifying
            .verify_strict(
                &signed_message(manifest_bytes),
                &Signature::from_bytes(&bytes),
            )
            .map_err(|_| bad())?;
        Ok(key)
    }
}

/// Parse `name=hex,hex,...` (whitespace around entries is ignored; an entry without a
/// name is called `<source>-<key id>`).
fn parse_key_list(text: &str, source: KeySource) -> Result<Vec<TrustedKey>, String> {
    let mut keys = Vec::new();
    for entry in text
        .split([',', ';'])
        .map(str::trim)
        .filter(|e| !e.is_empty())
    {
        let (name, hex) = match entry.split_once('=') {
            Some((name, hex)) => (Some(name.trim()), hex.trim()),
            None => (None, entry),
        };
        let public_key = parse_public_key(hex)?;
        let name = name.map_or_else(
            || {
                let prefix = match source {
                    KeySource::Build => "build",
                    KeySource::Development => "dev",
                    KeySource::Release | KeySource::Explicit => "key",
                };
                format!("{prefix}-{}", key_id_for(&public_key))
            },
            str::to_owned,
        );
        keys.push(TrustedKey::new(&name, public_key, source)?);
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::{KeyPair, sign_manifest};

    fn at(text: &str) -> Timestamp {
        Timestamp::new(text).unwrap()
    }

    #[test]
    fn the_compiled_release_document_parses() {
        let store = TrustStore::builtin().unwrap();
        assert!(
            store.keys().all(|key| key.source() != KeySource::Explicit),
            "{store:?}"
        );
    }

    #[test]
    fn revocation_wins_over_trust_and_validity_windows_apply() {
        let key = KeyPair::generate("windowed").unwrap();
        let manifest = b"manifest";
        let signature = sign_manifest(manifest, &key);
        let trusted = key.trusted_key(KeySource::Explicit).with_validity(
            Some(at("2026-01-01T00:00:00Z")),
            Some(at("2026-12-31T23:59:59Z")),
        );
        let mut store = TrustStore::empty().with_key(trusted);
        store
            .verify(manifest, &signature, &at("2026-06-01T00:00:00Z"))
            .unwrap();
        for outside in ["2025-12-31T23:59:59Z", "2027-01-01T00:00:00Z"] {
            assert!(matches!(
                store.verify(manifest, &signature, &at(outside)),
                Err(SignatureError::KeyNotValid { .. })
            ));
        }
        store.revoke(Revocation {
            key_id: key.key_id().into(),
            revoked_at: None,
            reason: Some("compromised".into()),
        });
        assert!(matches!(
            store.verify(manifest, &signature, &at("2026-06-01T00:00:00Z")),
            Err(SignatureError::RevokedKey {
                reason: Some(_),
                ..
            })
        ));
    }

    #[test]
    fn key_lists_and_revocation_files_are_strict() {
        let key = KeyPair::generate("listed").unwrap();
        let keys = parse_key_list(
            &format!(" ci={} , {} ", key.public_key_hex(), key.public_key_hex()),
            KeySource::Build,
        )
        .unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].name(), "ci");
        assert!(keys[1].name().starts_with("build-"));
        assert!(parse_key_list("ci=xyz", KeySource::Build).is_err());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(REVOCATIONS_FILE_NAME);
        assert_eq!(RevocationList::load(&path).unwrap(), None);
        let mut list = RevocationList::default();
        assert!(list.add(Revocation {
            key_id: key.key_id().into(),
            revoked_at: Some(at("2026-10-10T00:00:00Z")),
            reason: None,
        }));
        assert!(!list.add(Revocation {
            key_id: key.key_id().into(),
            revoked_at: None,
            reason: None,
        }));
        list.store(&path).unwrap();
        assert_eq!(RevocationList::load(&path).unwrap(), Some(list));
        fs::write(
            &path,
            r#"{"schema_version":1,"revoked":[{"key_id":"nothex"}]}"#,
        )
        .unwrap();
        assert!(RevocationList::load(&path).is_err());
    }
}
