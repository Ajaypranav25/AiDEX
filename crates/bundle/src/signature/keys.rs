//! Ed25519 key pairs and their JSON key files (`<name>.key` / `<name>.pub.json`).
//!
//! Key files are for development and CI only. Real release signing keys never exist
//! as files in a checkout: see `portable/packaging/README.md#signing-keys`.

use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::Path,
};

use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use super::trust::{KeySource, TrustedKey};
use crate::{
    BundleError,
    digest::{from_hex, is_lower_hex, to_hex},
};

/// The only signature algorithm.
pub const KEY_ALGORITHM: &str = "ed25519";
const KEY_FILE_SCHEMA_VERSION: u32 = 1;
const SIGNING_KEY_KIND: &str = "aidex-signing-key";
const PUBLIC_KEY_KIND: &str = "aidex-public-key";
const MAX_KEY_FILE_BYTES: u64 = 16 * 1024;

/// The key id of an Ed25519 public key: the first 8 bytes of its SHA-256, as hex.
pub fn key_id_for(public_key: &[u8; 32]) -> String {
    to_hex(&Sha256::digest(public_key)[..8])
}

/// Key names: 1-64 bytes of `[A-Za-z0-9._-]`.
pub(crate) fn validate_key_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "key name `{name}` must be 1-64 characters of [A-Za-z0-9._-]"
        ))
    }
}

/// Parse a 32-byte public key from lowercase hex.
pub(crate) fn parse_public_key(hex: &str) -> Result<[u8; 32], String> {
    if !is_lower_hex(hex, 32) {
        return Err("public keys are 64 lowercase hex digits".into());
    }
    from_hex(hex)
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| "public keys are 64 lowercase hex digits".into())
}

/// An Ed25519 signing key with a human-readable name. `Debug` never shows the secret.
pub struct KeyPair {
    name: String,
    key_id: String,
    signing: SigningKey,
}

impl KeyPair {
    /// Generate a new key from the operating system's random number generator.
    pub fn generate(name: &str) -> Result<Self, BundleError> {
        validate_key_name(name).map_err(BundleError::Trust)?;
        let mut seed = [0_u8; 32];
        getrandom::fill(&mut seed)
            .map_err(|error| BundleError::Trust(format!("no randomness for a new key: {error}")))?;
        let pair = Self::from_seed(name, &seed);
        seed.zeroize();
        Ok(pair)
    }

    fn from_seed(name: &str, seed: &[u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(seed);
        let key_id = key_id_for(signing.verifying_key().as_bytes());
        Self {
            name: name.to_owned(),
            key_id,
            signing,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    pub fn public_key_hex(&self) -> String {
        to_hex(&self.public_key())
    }

    /// Sign `message` (callers sign [`super::signed_message`] output).
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.signing.sign(message).to_bytes()
    }

    /// This key as a trust-store entry without a validity window.
    pub fn trusted_key(&self, source: KeySource) -> TrustedKey {
        TrustedKey::from_parts(self.name.clone(), self.public_key(), source)
    }

    pub fn public_file(&self) -> PublicKeyFile {
        PublicKeyFile {
            schema_version: KEY_FILE_SCHEMA_VERSION,
            kind: PUBLIC_KEY_KIND.into(),
            algorithm: KEY_ALGORITHM.into(),
            key_id: self.key_id.clone(),
            name: self.name.clone(),
            public_key: self.public_key_hex(),
        }
    }

    /// Write the secret key file at `path`, which must not exist (mode 0600 on Unix).
    pub fn store_secret(&self, path: &Path) -> Result<(), BundleError> {
        let mut file = SigningKeyFile {
            schema_version: KEY_FILE_SCHEMA_VERSION,
            kind: SIGNING_KEY_KIND.into(),
            algorithm: KEY_ALGORITHM.into(),
            key_id: self.key_id.clone(),
            name: self.name.clone(),
            secret_key: to_hex(self.signing.as_bytes()),
        };
        let result = serde_json::to_vec_pretty(&file)
            .map_err(|error| BundleError::Trust(error.to_string()))
            .and_then(|mut bytes| {
                let written = write_new(path, &bytes, true);
                bytes.zeroize();
                written
            });
        file.secret_key.zeroize();
        result
    }

    /// Read a secret key file written by [`store_secret`](Self::store_secret).
    pub fn load_secret(path: &Path) -> Result<Self, BundleError> {
        let mut bytes = read_small(path)?;
        let parsed: Result<SigningKeyFile, _> = serde_json::from_slice(&bytes);
        bytes.zeroize();
        // serde messages can echo values; name only the file.
        let mut file = parsed.map_err(|_| {
            BundleError::Trust(format!(
                "`{}` is not an AiDEX signing key file",
                path.display()
            ))
        })?;
        let result = (|| {
            if file.schema_version != KEY_FILE_SCHEMA_VERSION
                || file.kind != SIGNING_KEY_KIND
                || file.algorithm != KEY_ALGORITHM
            {
                return Err(format!(
                    "`{}` is not a version-1 ed25519 signing key file",
                    path.display()
                ));
            }
            validate_key_name(&file.name)?;
            let mut seed = from_hex(&file.secret_key)
                .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                .ok_or_else(|| format!("`{}` holds a malformed secret key", path.display()))?;
            let pair = Self::from_seed(&file.name, &seed);
            seed.zeroize();
            if pair.key_id != file.key_id {
                return Err(format!(
                    "`{}`: key_id {} does not match its key ({})",
                    path.display(),
                    file.key_id,
                    pair.key_id
                ));
            }
            Ok(pair)
        })();
        file.secret_key.zeroize();
        result.map_err(BundleError::Trust)
    }
}

impl fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPair")
            .field("name", &self.name)
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

/// `<name>.key`: a development/CI signing key. Keep it secret; never commit it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SigningKeyFile {
    pub schema_version: u32,
    pub kind: String,
    pub algorithm: String,
    pub key_id: String,
    pub name: String,
    /// The 32-byte Ed25519 seed as hex.
    secret_key: String,
}

impl fmt::Debug for SigningKeyFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKeyFile")
            .field("key_id", &self.key_id)
            .field("name", &self.name)
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}

/// `<name>.pub.json`: the public half of a key, safe to publish and to commit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicKeyFile {
    pub schema_version: u32,
    pub kind: String,
    pub algorithm: String,
    pub key_id: String,
    pub name: String,
    /// The 32-byte Ed25519 public key as lowercase hex.
    pub public_key: String,
}

impl PublicKeyFile {
    pub fn load(path: &Path) -> Result<Self, BundleError> {
        let bytes = read_small(path)?;
        let file: Self = serde_json::from_slice(&bytes).map_err(|error| {
            BundleError::Trust(format!(
                "`{}` is not an AiDEX public key file: {error}",
                path.display()
            ))
        })?;
        file.to_trusted(KeySource::Explicit)
            .map_err(|error| BundleError::Trust(format!("`{}`: {error}", path.display())))?;
        Ok(file)
    }

    /// Write the file at `path`, which must not exist.
    pub fn store(&self, path: &Path) -> Result<(), BundleError> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| BundleError::Trust(error.to_string()))?;
        bytes.push(b'\n');
        write_new(path, &bytes, false)
    }

    /// This public key as a trust-store entry.
    pub fn to_trusted(&self, source: KeySource) -> Result<TrustedKey, String> {
        if self.schema_version != KEY_FILE_SCHEMA_VERSION
            || self.kind != PUBLIC_KEY_KIND
            || self.algorithm != KEY_ALGORITHM
        {
            return Err("not a version-1 ed25519 public key file".into());
        }
        validate_key_name(&self.name)?;
        let public_key = parse_public_key(&self.public_key)?;
        let key = TrustedKey::from_parts(self.name.clone(), public_key, source);
        if key.key_id() != self.key_id {
            return Err(format!(
                "key_id {} does not match the public key ({})",
                self.key_id,
                key.key_id()
            ));
        }
        Ok(key)
    }
}

fn read_small(path: &Path) -> Result<Vec<u8>, BundleError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| BundleError::io(format!("cannot read `{}`", path.display()), error))?;
    if !metadata.is_file() || metadata.len() > MAX_KEY_FILE_BYTES {
        return Err(BundleError::Trust(format!(
            "`{}` is not a key file (regular file of at most {MAX_KEY_FILE_BYTES} bytes)",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_KEY_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| BundleError::io(format!("cannot read `{}`", path.display()), error))?;
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8], secret: bool) -> Result<(), BundleError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = secret;
    let result = options.open(path).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    result.map_err(|error| {
        if error.kind() == ErrorKind::AlreadyExists {
            BundleError::Trust(format!(
                "`{}` already exists; refusing to overwrite a key file",
                path.display()
            ))
        } else {
            BundleError::io(format!("cannot write `{}`", path.display()), error)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_files_round_trip_and_never_show_the_secret() {
        let dir = tempfile::tempdir().unwrap();
        let pair = KeyPair::generate("dev-1").unwrap();
        let secret = dir.path().join("dev-1.key");
        let public = dir.path().join("dev-1.pub.json");
        pair.store_secret(&secret).unwrap();
        pair.public_file().store(&public).unwrap();
        assert!(pair.store_secret(&secret).is_err(), "never overwrites");

        let loaded = KeyPair::load_secret(&secret).unwrap();
        assert_eq!(loaded.key_id(), pair.key_id());
        assert_eq!(loaded.public_key(), pair.public_key());
        let file = PublicKeyFile::load(&public).unwrap();
        assert_eq!(file.key_id, pair.key_id());
        assert_eq!(file.public_key, pair.public_key_hex());

        let text = fs::read_to_string(&secret).unwrap();
        let hex_secret = serde_json::from_str::<serde_json::Value>(&text).unwrap()["secret_key"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(!format!("{pair:?}").contains(&hex_secret));
        assert!(!format!("{loaded:?}").contains(&hex_secret));

        // A public file whose id does not match its key is rejected.
        let mut wrong = file.clone();
        wrong.key_id = "0".repeat(16);
        assert!(wrong.to_trusted(KeySource::Explicit).is_err());
        assert!(KeyPair::generate("bad name").is_err());
    }

    #[test]
    fn key_ids_are_stable_fingerprints() {
        let id = key_id_for(&[7; 32]);
        assert_eq!(id.len(), 16);
        assert_eq!(id, key_id_for(&[7; 32]));
        assert_ne!(id, key_id_for(&[8; 32]));
    }
}
