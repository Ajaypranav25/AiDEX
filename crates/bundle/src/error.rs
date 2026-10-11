//! Errors for bundle manifests, pointers, verification, and installation.

use std::{error::Error, fmt, io, path::PathBuf};

use aidex_core::ServiceErrorCode;
use aidex_drive::DriveError;

use crate::{InstallStep, Target};

/// Why a JSON document (`bundle.json` or `current.json`) was rejected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentError {
    /// `schema_version` is missing, not a number, or not one this build understands.
    UnsupportedSchema { found: Option<u64>, supported: u32 },
    /// Not valid JSON, or fields that do not match the closed schema.
    Malformed(String),
    /// Well-formed but semantically invalid (bad path, digest, missing role, ...).
    Invalid(String),
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema {
                found: Some(found),
                supported,
            } => write!(
                f,
                "schema version {found} is not supported (this build reads version {supported})"
            ),
            Self::UnsupportedSchema {
                found: None,
                supported,
            } => write!(
                f,
                "missing or non-numeric schema_version (this build reads version {supported})"
            ),
            Self::Malformed(reason) => write!(f, "malformed: {reason}"),
            Self::Invalid(reason) => write!(f, "invalid: {reason}"),
        }
    }
}

/// What is wrong with one bundle file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrityProblem {
    /// A listed file does not exist.
    Missing,
    /// The path exists but is not a regular file (or a parent is not a directory).
    NotRegularFile,
    /// The path or one of its parents is a symbolic link, junction, or reparse point.
    Link,
    SizeMismatch {
        expected: u64,
        actual: u64,
    },
    DigestMismatch {
        expected: String,
        actual: String,
    },
    /// A file exists where only manifest-listed files are allowed.
    Unlisted,
    /// `bundle.json` does not match the digest pinned in `app/current.json`.
    ManifestPin {
        expected: String,
        actual: String,
    },
}

impl fmt::Display for IntegrityProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("file is missing"),
            Self::NotRegularFile => f.write_str("not a regular file"),
            Self::Link => {
                f.write_str("symbolic links, junctions, and reparse points are not allowed")
            }
            Self::SizeMismatch { expected, actual } => {
                write!(
                    f,
                    "size mismatch (manifest {expected} bytes, file {actual} bytes)"
                )
            }
            Self::DigestMismatch { expected, actual } => {
                write!(f, "sha256 mismatch (manifest {expected}, file {actual})")
            }
            Self::Unlisted => f.write_str("file is not listed in bundle.json"),
            Self::ManifestPin { expected, actual } => write!(
                f,
                "bundle.json sha256 {actual} does not match {expected} pinned by app/current.json"
            ),
        }
    }
}

/// Why a bundle's signature was not accepted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignatureError {
    /// There is no `bundle.sig`, and unsigned bundles are not allowed.
    Unsigned,
    /// `bundle.sig` is unreadable, malformed, or not a regular file.
    Malformed(DocumentError),
    /// `bundle.sig` was made for a different `bundle.json`.
    ManifestMismatch { expected: String, actual: String },
    /// The signing key is not in the trust store.
    UnknownKey { key_id: String },
    /// The signing key was revoked (compiled-in or drive revocation list).
    RevokedKey {
        key_id: String,
        reason: Option<String>,
    },
    /// The bundle's `created_at` is outside the key's validity window
    /// (`window` reads like "from <start> to <end>").
    KeyNotValid {
        key_id: String,
        created_at: String,
        window: String,
    },
    /// The Ed25519 signature does not verify with the named trusted key.
    BadSignature { key_id: String },
}

impl SignatureError {
    /// Stable machine-readable name (API `data.reason`).
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Unsigned => "unsigned",
            Self::Malformed(_) => "malformed_signature",
            Self::ManifestMismatch { .. } => "signature_manifest_mismatch",
            Self::UnknownKey { .. } => "unknown_key",
            Self::RevokedKey { .. } => "revoked_key",
            Self::KeyNotValid { .. } => "key_not_valid",
            Self::BadSignature { .. } => "bad_signature",
        }
    }
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsigned => f.write_str(
                "the bundle is not signed (no bundle.sig) and unsigned bundles are not allowed",
            ),
            Self::Malformed(error) => write!(f, "bundle.sig is {error}"),
            Self::ManifestMismatch { expected, actual } => write!(
                f,
                "bundle.sig was made for bundle.json sha256 {expected}, but bundle.json has sha256 {actual}"
            ),
            Self::UnknownKey { key_id } => write!(
                f,
                "the bundle is signed by key {key_id}, which this build does not trust"
            ),
            Self::RevokedKey { key_id, reason } => match reason {
                Some(reason) => write!(f, "signing key {key_id} is revoked ({reason})"),
                None => write!(f, "signing key {key_id} is revoked"),
            },
            Self::KeyNotValid {
                key_id,
                created_at,
                window,
            } => write!(
                f,
                "signing key {key_id} is valid {window}, but the bundle was created at {created_at}"
            ),
            Self::BadSignature { key_id } => write!(
                f,
                "the bundle.json signature does not verify with key {key_id} (the manifest or signature was modified)"
            ),
        }
    }
}

/// Coarse error classes with stable meaning for exit-code and API mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundleErrorKind {
    /// The drive itself is missing, uninitialized, unsupported, or unsafe.
    Drive,
    /// No bundle (or none for this target) is installed.
    NotInstalled,
    /// The bundle or pointer is for another platform, schema, or drive version.
    Incompatible,
    /// Files, pointer, or manifest do not match what was recorded; nothing was run.
    Integrity,
    /// The bundle is unsigned (and that is not allowed), or its signature is not valid
    /// for a trusted, unrevoked key; nothing was run.
    Untrusted,
    /// A drive-root path holds a file that is not an AiDEX launcher.
    Occupied,
    /// The install source or archive is not a usable bundle.
    InvalidInput,
    /// An install observer stopped the install.
    Aborted,
    /// An unexpected filesystem error.
    Io,
}

/// Errors from manifests, pointers, verification, and installation.
#[derive(Debug)]
pub enum BundleError {
    Drive(DriveError),
    Manifest {
        path: PathBuf,
        error: DocumentError,
    },
    Pointer {
        path: PathBuf,
        error: DocumentError,
    },
    /// `app/update-journal.json` is unreadable or malformed.
    Journal {
        path: PathBuf,
        error: DocumentError,
    },
    /// `app/current.json` does not exist.
    NotInstalled {
        pointer: PathBuf,
    },
    TargetNotInstalled {
        target: Target,
        installed: Vec<String>,
    },
    /// This build runs on a platform without a bundle target name.
    UnsupportedHost,
    WrongTarget {
        expected: Target,
        found: Target,
    },
    VersionMismatch {
        pointer: String,
        manifest: String,
    },
    DriveSchemaTooOld {
        required: u32,
        found: u32,
    },
    /// The bundle declares that it must be extracted to the host, which is unsupported.
    HostExtractionUnsupported,
    Integrity {
        bundle: PathBuf,
        path: String,
        problem: IntegrityProblem,
    },
    Signature {
        bundle: PathBuf,
        error: SignatureError,
    },
    /// A trust-store document, key file, or built-in trust configuration is invalid.
    Trust(String),
    Occupied {
        path: PathBuf,
    },
    InvalidSource(String),
    Archive(String),
    /// An archive entry that could escape or confuse extraction (traversal, absolute
    /// path, link, device, duplicate, or outside the bundle directory).
    ArchiveEntry {
        name: String,
        reason: String,
    },
    /// An archive over the entry-count or size limits.
    ArchiveLimit(String),
    Aborted(InstallStep),
    Io {
        context: String,
        error: io::Error,
    },
}

impl BundleError {
    pub(crate) fn io(context: impl Into<String>, error: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            error,
        }
    }

    pub(crate) fn integrity(
        bundle: impl Into<PathBuf>,
        path: impl Into<String>,
        problem: IntegrityProblem,
    ) -> Self {
        Self::Integrity {
            bundle: bundle.into(),
            path: path.into(),
            problem,
        }
    }

    pub fn kind(&self) -> BundleErrorKind {
        match self {
            Self::Drive(_) => BundleErrorKind::Drive,
            Self::NotInstalled { .. } | Self::TargetNotInstalled { .. } => {
                BundleErrorKind::NotInstalled
            }
            Self::Manifest {
                error: DocumentError::UnsupportedSchema { .. },
                ..
            }
            | Self::Pointer {
                error: DocumentError::UnsupportedSchema { .. },
                ..
            }
            | Self::UnsupportedHost
            | Self::WrongTarget { .. }
            | Self::DriveSchemaTooOld { .. }
            | Self::HostExtractionUnsupported => BundleErrorKind::Incompatible,
            Self::Manifest { .. }
            | Self::Pointer { .. }
            | Self::Journal { .. }
            | Self::VersionMismatch { .. }
            | Self::Integrity { .. } => BundleErrorKind::Integrity,
            Self::Signature { .. } => BundleErrorKind::Untrusted,
            Self::Occupied { .. } => BundleErrorKind::Occupied,
            Self::Trust(_)
            | Self::InvalidSource(_)
            | Self::Archive(_)
            | Self::ArchiveEntry { .. }
            | Self::ArchiveLimit(_) => BundleErrorKind::InvalidInput,
            Self::Aborted(_) => BundleErrorKind::Aborted,
            Self::Io { .. } => BundleErrorKind::Io,
        }
    }

    /// Stable service code for API mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Drive(error) => error.code(),
            _ => match self.kind() {
                BundleErrorKind::NotInstalled => ServiceErrorCode::NotFound,
                BundleErrorKind::Incompatible => ServiceErrorCode::IncompatibleVersion,
                BundleErrorKind::Integrity
                | BundleErrorKind::Untrusted
                | BundleErrorKind::Occupied => ServiceErrorCode::Conflict,
                BundleErrorKind::InvalidInput => ServiceErrorCode::InvalidArgument,
                BundleErrorKind::Aborted => ServiceErrorCode::Unavailable,
                BundleErrorKind::Drive | BundleErrorKind::Io => ServiceErrorCode::Internal,
            },
        }
    }
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Drive(error) => write!(f, "{error}"),
            Self::Manifest { path, error } => {
                write!(f, "bundle manifest `{}` is {error}", path.display())
            }
            Self::Pointer { path, error } => {
                write!(f, "bundle pointer `{}` is {error}", path.display())
            }
            Self::Journal { path, error } => {
                write!(f, "update journal `{}` is {error}", path.display())
            }
            Self::NotInstalled { pointer } => write!(
                f,
                "no AiDEX bundle is installed on this drive (`{}` does not exist)",
                pointer.display()
            ),
            Self::TargetNotInstalled { target, installed } => {
                if installed.is_empty() {
                    write!(f, "no AiDEX bundle for {target} is installed on this drive")
                } else {
                    write!(
                        f,
                        "no AiDEX bundle for {target} is installed on this drive (installed: {})",
                        installed.join(", ")
                    )
                }
            }
            Self::UnsupportedHost => f.write_str(
                "this host platform has no AiDEX bundle target (supported names: windows-x64, linux-x64, linux-arm64, macos-arm64, macos-x64)",
            ),
            Self::WrongTarget { expected, found } => {
                write!(f, "the bundle is built for {found}, but this host needs {expected}")
            }
            Self::VersionMismatch { pointer, manifest } => write!(
                f,
                "app/current.json selects version {pointer}, but the bundle manifest says {manifest}"
            ),
            Self::DriveSchemaTooOld { required, found } => write!(
                f,
                "the bundle requires drive schema v{required}, but the drive uses v{found}; run `aidex init --drive <root>` with a current AiDEX to migrate it"
            ),
            Self::HostExtractionUnsupported => f.write_str(
                "the bundle requires extraction to the host, which this launcher does not support",
            ),
            Self::Integrity {
                bundle,
                path,
                problem,
            } => write!(
                f,
                "integrity check failed for `{path}` in `{}`: {problem}",
                bundle.display()
            ),
            Self::Signature { bundle, error } => write!(
                f,
                "signature check failed for `{}`: {error}",
                bundle.display()
            ),
            Self::Trust(reason) => write!(f, "invalid trust configuration: {reason}"),
            Self::Occupied { path } => write!(
                f,
                "`{}` exists and is not an AiDEX launcher; refusing to replace it (move it aside and retry)",
                path.display()
            ),
            Self::InvalidSource(reason) => write!(f, "invalid bundle source: {reason}"),
            Self::Archive(reason) => write!(f, "invalid bundle archive: {reason}"),
            Self::ArchiveEntry { name, reason } => {
                write!(f, "unsafe bundle archive entry `{name}`: {reason}")
            }
            Self::ArchiveLimit(reason) => {
                write!(f, "bundle archive exceeds the extraction limits: {reason}")
            }
            Self::Aborted(step) => write!(f, "install stopped before step `{step}`"),
            Self::Io { context, error } => write!(f, "{context}: {error}"),
        }
    }
}

impl Error for BundleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Drive(error) => Some(error),
            Self::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl From<DriveError> for BundleError {
    fn from(error: DriveError) -> Self {
        Self::Drive(error)
    }
}
