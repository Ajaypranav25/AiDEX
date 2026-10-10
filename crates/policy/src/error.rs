//! Errors returned while parsing identifiers, classifying targets, and loading policy files.

use std::{error::Error, fmt, path::PathBuf};

use aidex_core::ServiceErrorCode;

/// Longest echo of an unparsed value kept in an error message.
const MAX_ECHO: usize = 64;

/// Errors from the policy crate. Evaluation itself never fails: it always returns a
/// [`crate::Decision`], denying when facts are missing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyError {
    /// A string is not one of the closed identifiers of `kind` (action, profile, ...).
    UnknownValue { kind: &'static str, value: String },
    /// A host name or address cannot be classified as a network target.
    InvalidNetworkTarget(String),
    /// A path glob is empty, absolute, escapes its root, or does not compile.
    InvalidGlob { pattern: String, reason: String },
    /// A policy directory does not exist.
    MissingDirectory(PathBuf),
    /// A policy path is a symbolic link, junction, or not a regular file/directory.
    UnsafePath(PathBuf),
    /// A policy file exceeds the size limit.
    TooLarge { path: PathBuf, limit: u64 },
    /// Reading a policy file or directory failed.
    Io { path: PathBuf, message: String },
    /// A policy document is not valid TOML or does not match the closed schema.
    Parse { source: String, message: String },
    /// A policy document declares a schema version this build does not understand.
    UnsupportedSchema { source: String, found: i64 },
    /// A policy document parsed but is semantically invalid.
    InvalidDocument { source: String, message: String },
    /// Two rules share an id within the same layer and subject.
    DuplicateRule { layer: String, id: String },
}

impl PolicyError {
    /// Stable service code for API mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::UnknownValue { .. }
            | Self::InvalidNetworkTarget(_)
            | Self::InvalidGlob { .. }
            | Self::TooLarge { .. }
            | Self::Parse { .. }
            | Self::InvalidDocument { .. } => ServiceErrorCode::InvalidArgument,
            Self::DuplicateRule { .. } => ServiceErrorCode::Conflict,
            Self::MissingDirectory(_) => ServiceErrorCode::NotFound,
            Self::UnsafePath(_) => ServiceErrorCode::PermissionDenied,
            Self::UnsupportedSchema { .. } => ServiceErrorCode::IncompatibleVersion,
            Self::Io { .. } => ServiceErrorCode::Internal,
        }
    }

    pub(crate) fn unknown(kind: &'static str, value: &str) -> Self {
        Self::UnknownValue {
            kind,
            value: truncate(value),
        }
    }

    pub(crate) fn invalid(source: &str, message: impl Into<String>) -> Self {
        Self::InvalidDocument {
            source: source.to_owned(),
            message: message.into(),
        }
    }
}

pub(crate) fn truncate(value: &str) -> String {
    if value.chars().count() <= MAX_ECHO {
        return value.to_owned();
    }
    let mut short: String = value.chars().take(MAX_ECHO).collect();
    short.push('…');
    short
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownValue { kind, value } => write!(f, "unknown {kind} `{value}`"),
            Self::InvalidNetworkTarget(host) => {
                write!(f, "`{host}` is not a valid host name or IP address")
            }
            Self::InvalidGlob { pattern, reason } => {
                write!(f, "invalid path pattern `{pattern}`: {reason}")
            }
            Self::MissingDirectory(path) => {
                write!(f, "policy directory does not exist: {}", path.display())
            }
            Self::UnsafePath(path) => write!(
                f,
                "refusing to read policy path `{}`: it is a symbolic link, junction, or not a regular entry",
                path.display()
            ),
            Self::TooLarge { path, limit } => write!(
                f,
                "policy file `{}` exceeds the {limit}-byte limit",
                path.display()
            ),
            Self::Io { path, message } => {
                write!(f, "cannot read policy path `{}`: {message}", path.display())
            }
            Self::Parse { source, message } => {
                write!(f, "policy document `{source}` is malformed: {message}")
            }
            Self::UnsupportedSchema { source, found } => write!(
                f,
                "policy document `{source}` uses schema_version {found}; this build supports schema_version 1"
            ),
            Self::InvalidDocument { source, message } => {
                write!(f, "policy document `{source}` is invalid: {message}")
            }
            Self::DuplicateRule { layer, id } => {
                write!(
                    f,
                    "rule id `{id}` is defined more than once in the {layer} layer"
                )
            }
        }
    }
}

impl Error for PolicyError {}
