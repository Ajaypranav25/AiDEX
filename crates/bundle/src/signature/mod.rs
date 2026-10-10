//! `bundle.sig`: a detached Ed25519 signature over the exact bytes of `bundle.json`.
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "algorithm": "ed25519",
//!   "key_id": "<16 lowercase hex: first 8 bytes of SHA-256(public key)>",
//!   "manifest_sha256": "<sha256 of the signed bundle.json>",
//!   "signature": "<128 lowercase hex>"
//! }
//! ```
//!
//! The signed message is [`SIGNATURE_CONTEXT`] followed by the `bundle.json` bytes
//! exactly as stored (no re-serialization, so no canonical-JSON rules are needed).
//! Because `bundle.json` pins the size and SHA-256 of every file, the signature
//! authenticates the whole bundle. `bundle.sig` sits next to `bundle.json`, is never
//! listed in it, and is verified against a [`TrustStore`] of keys compiled into the
//! binary plus revocations (compiled-in and `manifests/revoked-keys.json`).

mod keys;
mod trust;

use std::{
    fs::{self, File},
    io::{ErrorKind, Read},
    path::Path,
};

use aidex_core::Timestamp;
use serde::{Deserialize, Serialize};

pub use keys::{KEY_ALGORITHM, KeyPair, PublicKeyFile, SigningKeyFile, key_id_for};
pub use trust::{
    BUILD_TRUSTED_KEYS_ENV, DEV_TRUSTED_KEYS_ENV, KeySource, REVOCATIONS_FILE_NAME, Revocation,
    RevocationList, TrustStore, TrustedKey, revocations_path,
};

use crate::{
    BundleError, DocumentError, SignatureError,
    digest::{is_lower_hex, is_sha256_hex, sha256_hex},
    fsutil::write_atomic,
    manifest::parse_versioned,
};

/// File name of the detached signature next to `bundle.json`.
pub const SIGNATURE_FILE_NAME: &str = "bundle.sig";
/// The `bundle.sig` schema this build writes and reads.
pub const SIGNATURE_SCHEMA_VERSION: u32 = 1;
/// Domain-separation prefix of every signed message, so a bundle signature can never
/// be replayed as a signature over another kind of AiDEX document.
pub const SIGNATURE_CONTEXT: &[u8] = b"AIDEX-BUNDLE-SIG-V1\n";
const MAX_SIGNATURE_BYTES: u64 = 4096;

/// The parsed `bundle.sig`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleSignature {
    /// Always [`SIGNATURE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Always [`KEY_ALGORITHM`] (`ed25519`).
    pub algorithm: String,
    /// Identifier of the signing key ([`key_id_for`] of its public key).
    pub key_id: String,
    /// SHA-256 of the signed `bundle.json` (diagnostics; the signature is what counts).
    pub manifest_sha256: String,
    /// The 64-byte Ed25519 signature as lowercase hex.
    pub signature: String,
}

impl BundleSignature {
    pub fn from_json(bytes: &[u8]) -> Result<Self, DocumentError> {
        let signature: Self = parse_versioned(bytes, SIGNATURE_SCHEMA_VERSION)?;
        signature.validate()?;
        Ok(signature)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, DocumentError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| DocumentError::Malformed(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    fn validate(&self) -> Result<(), DocumentError> {
        let invalid = |reason: &str| Err(DocumentError::Invalid(reason.to_owned()));
        if self.algorithm != KEY_ALGORITHM {
            return invalid("algorithm must be `ed25519`");
        }
        if !is_lower_hex(&self.key_id, 8) {
            return invalid("key_id must be 16 lowercase hex digits");
        }
        if !is_sha256_hex(&self.manifest_sha256) {
            return invalid("manifest_sha256 must be 64 lowercase hex digits");
        }
        if !is_lower_hex(&self.signature, 64) {
            return invalid("signature must be 128 lowercase hex digits");
        }
        Ok(())
    }
}

/// The message that is signed for `manifest_bytes`.
pub fn signed_message(manifest_bytes: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(SIGNATURE_CONTEXT.len() + manifest_bytes.len());
    message.extend_from_slice(SIGNATURE_CONTEXT);
    message.extend_from_slice(manifest_bytes);
    message
}

/// Sign `bundle.json` bytes with `key`.
pub fn sign_manifest(manifest_bytes: &[u8], key: &KeyPair) -> BundleSignature {
    let signature = key.sign(&signed_message(manifest_bytes));
    BundleSignature {
        schema_version: SIGNATURE_SCHEMA_VERSION,
        algorithm: KEY_ALGORITHM.to_owned(),
        key_id: key.key_id().to_owned(),
        manifest_sha256: sha256_hex(manifest_bytes),
        signature: crate::digest::to_hex(&signature),
    }
}

/// Read `bundle.sig` from a bundle directory: `Ok(None)` when there is none. A link,
/// a non-file, an oversized or malformed file is a [`SignatureError::Malformed`].
pub fn read_signature(bundle_dir: &Path) -> Result<Option<BundleSignature>, BundleError> {
    let path = bundle_dir.join(SIGNATURE_FILE_NAME);
    let malformed = |reason: String| BundleError::Signature {
        bundle: bundle_dir.to_path_buf(),
        error: SignatureError::Malformed(DocumentError::Malformed(reason)),
    };
    let metadata = match fs::symlink_metadata(&path) {
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
        return Err(malformed("not a regular file".into()));
    }
    if metadata.len() > MAX_SIGNATURE_BYTES {
        return Err(malformed(format!(
            "larger than {MAX_SIGNATURE_BYTES} bytes"
        )));
    }
    let mut bytes = Vec::new();
    File::open(&path)
        .and_then(|file| file.take(MAX_SIGNATURE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| BundleError::io(format!("cannot read `{}`", path.display()), error))?;
    BundleSignature::from_json(&bytes)
        .map(Some)
        .map_err(|error| BundleError::Signature {
            bundle: bundle_dir.to_path_buf(),
            error: SignatureError::Malformed(error),
        })
}

/// Atomically write `bundle.sig` into a bundle directory.
pub fn write_signature(bundle_dir: &Path, signature: &BundleSignature) -> Result<(), BundleError> {
    let bytes = signature
        .to_json()
        .map_err(|error| BundleError::Trust(format!("cannot encode bundle.sig: {error}")))?;
    write_atomic(&bundle_dir.join(SIGNATURE_FILE_NAME), &bytes)
}

/// Which bundles a verification accepts.
#[derive(Clone, Debug, Default)]
pub struct SignaturePolicy {
    pub trust: TrustStore,
    /// Accept a bundle without `bundle.sig` (development only; never the default).
    /// A present but invalid or untrusted signature is refused regardless.
    pub allow_unsigned: bool,
}

impl SignaturePolicy {
    /// Require a valid signature by a key in `trust`.
    pub fn require(trust: TrustStore) -> Self {
        Self {
            trust,
            allow_unsigned: false,
        }
    }

    /// Like [`require`](Self::require), but also accept bundles that carry no signature.
    pub fn allow_unsigned(trust: TrustStore) -> Self {
        Self {
            trust,
            allow_unsigned: true,
        }
    }
}

/// The outcome of an accepted signature check.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SignatureState {
    /// Signed by a trusted, unrevoked key valid at the bundle's `created_at`.
    Verified {
        key_id: String,
        key_name: String,
        key_source: KeySource,
    },
    /// No `bundle.sig`, accepted only because the policy allows unsigned bundles.
    UnsignedAllowed,
}

impl SignatureState {
    pub fn key_id(&self) -> Option<&str> {
        match self {
            Self::Verified { key_id, .. } => Some(key_id),
            Self::UnsignedAllowed => None,
        }
    }

    pub fn is_signed(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }
}

/// Check the signature of the bundle in `bundle_dir` whose `bundle.json` bytes are
/// `manifest_bytes` (already read and pinned by the caller) and whose manifest says
/// it was created at `created_at`.
pub fn verify_signature(
    bundle_dir: &Path,
    manifest_bytes: &[u8],
    created_at: &Timestamp,
    policy: &SignaturePolicy,
) -> Result<SignatureState, BundleError> {
    let fail = |error| BundleError::Signature {
        bundle: bundle_dir.to_path_buf(),
        error,
    };
    let Some(signature) = read_signature(bundle_dir)? else {
        return if policy.allow_unsigned {
            Ok(SignatureState::UnsignedAllowed)
        } else {
            Err(fail(SignatureError::Unsigned))
        };
    };
    let actual = sha256_hex(manifest_bytes);
    if signature.manifest_sha256 != actual {
        return Err(fail(SignatureError::ManifestMismatch {
            expected: signature.manifest_sha256.clone(),
            actual,
        }));
    }
    let key = policy
        .trust
        .verify(manifest_bytes, &signature, created_at)
        .map_err(fail)?;
    Ok(SignatureState::Verified {
        key_id: key.key_id().to_owned(),
        key_name: key.name().to_owned(),
        key_source: key.source(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn created() -> Timestamp {
        Timestamp::new("2026-10-10T00:00:00Z").unwrap()
    }

    #[test]
    fn signatures_verify_only_for_the_signed_bytes_and_trusted_keys() {
        let key = KeyPair::generate("unit").unwrap();
        let trust = TrustStore::empty().with_key(key.trusted_key(KeySource::Explicit));
        let manifest = b"{\"schema_version\":1}\n";
        let signature = sign_manifest(manifest, &key);
        assert_eq!(
            BundleSignature::from_json(&signature.to_json().unwrap()).unwrap(),
            signature
        );
        let verified = trust.verify(manifest, &signature, &created()).unwrap();
        assert_eq!(verified.key_id(), key.key_id());

        // One flipped byte in the manifest.
        let mut tampered = manifest.to_vec();
        tampered[3] ^= 1;
        assert!(matches!(
            trust.verify(&tampered, &signature, &created()),
            Err(SignatureError::BadSignature { .. })
        ));
        // A signature by a key nobody trusts.
        let stranger = KeyPair::generate("stranger").unwrap();
        assert!(matches!(
            trust.verify(manifest, &sign_manifest(manifest, &stranger), &created()),
            Err(SignatureError::UnknownKey { .. })
        ));
        // A trusted key id with someone else's signature bytes.
        let mut forged = sign_manifest(manifest, &stranger);
        forged.key_id = key.key_id().to_owned();
        assert!(matches!(
            trust.verify(manifest, &forged, &created()),
            Err(SignatureError::BadSignature { .. })
        ));
    }

    #[test]
    fn the_signature_document_is_closed_and_validated() {
        let key = KeyPair::generate("unit").unwrap();
        let signature = sign_manifest(b"x", &key);
        let mut value: serde_json::Value =
            serde_json::from_slice(&signature.to_json().unwrap()).unwrap();
        value["extra"] = true.into();
        assert!(matches!(
            BundleSignature::from_json(value.to_string().as_bytes()),
            Err(DocumentError::Malformed(_))
        ));
        for (field, bad) in [
            ("algorithm", "rsa"),
            ("key_id", "XYZ"),
            ("signature", "00"),
            ("manifest_sha256", "abc"),
        ] {
            let mut value: serde_json::Value =
                serde_json::from_slice(&signature.to_json().unwrap()).unwrap();
            value[field] = bad.into();
            assert!(
                matches!(
                    BundleSignature::from_json(value.to_string().as_bytes()),
                    Err(DocumentError::Invalid(_))
                ),
                "{field}"
            );
        }
    }
}
