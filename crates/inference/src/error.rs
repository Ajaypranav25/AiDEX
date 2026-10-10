//! Local inference failures. Every variant is an explicit state with a stable
//! [`InferenceError::kind`] and, where the user can fix it, an
//! [`InferenceError::advice`] naming the command to run.
//!
//! Messages never contain the per-launch server key, prompts, or model output.

use std::{fmt, path::PathBuf, time::Duration};

use aidex_core::ServiceErrorCode;
use aidex_providers::ProviderError;

use crate::gguf::GgufError;

#[derive(Clone, Debug, PartialEq)]
pub enum InferenceError {
    /// A model identifier is not usable as a registry key and directory name.
    InvalidModelId {
        id: String,
        reason: &'static str,
    },
    InvalidArgument(String),
    /// The file is not a readable GGUF model.
    Gguf(GgufError),
    /// The model parses but the pinned runtime cannot run it.
    IncompatibleModel {
        reason: String,
    },
    ModelNotFound {
        id: String,
    },
    /// Import refused to replace an existing model directory or registry entry.
    AlreadyExists {
        id: String,
        path: PathBuf,
    },
    /// A registry entry or manifest on the drive is unreadable or invalid.
    Corrupt {
        path: PathBuf,
        reason: String,
    },
    /// A file's SHA-256 differs from the recorded value; it must not be used.
    HashMismatch {
        what: String,
        expected: String,
        actual: String,
    },
    /// A recorded file is missing or is not a regular file.
    MissingFile {
        what: String,
        path: PathBuf,
    },
    /// This operating system / architecture has no AiDEX runtime platform.
    UnsupportedPlatform {
        os: String,
        arch: String,
    },
    /// No installed runtime can run on this host.
    RuntimeNotInstalled {
        platform: String,
        reason: String,
    },
    /// An archive does not match any pinned catalog entry.
    RuntimeNotInCatalog {
        sha256: String,
    },
    UnsupportedArchiveFormat {
        format: String,
    },
    /// An archive entry would escape the install directory or is not a plain file.
    UnsafeArchive {
        entry: String,
        reason: String,
    },
    /// An installed runtime failed verification before launch.
    RuntimeCorrupt {
        runtime: String,
        reason: String,
    },
    /// The host lacks the memory to run the model with any useful context.
    InsufficientMemory {
        required_bytes: u64,
        available_bytes: u64,
        explanation: String,
    },
    /// The sandbox refused or failed to launch the server.
    Sandbox {
        code: ServiceErrorCode,
        message: String,
    },
    /// The server exited before it was ready (or while serving).
    ServerExited {
        code: Option<i32>,
        detail: String,
        log: Option<PathBuf>,
    },
    /// The server did not become healthy in time.
    ServerTimeout {
        after: Duration,
        log: Option<PathBuf>,
    },
    /// Every resident slot is busy and the caller gave up waiting.
    Busy {
        reason: String,
    },
    Cancelled,
    /// Offline mode refused a download.
    OfflineRefused {
        host: String,
    },
    /// A download failed.
    Download {
        reason: String,
    },
    Provider(ProviderError),
    Io {
        context: String,
        message: String,
    },
}

impl InferenceError {
    pub(crate) fn io(context: impl Into<String>, error: &std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            message: error.to_string(),
        }
    }

    pub(crate) fn corrupt(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::Corrupt {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Stable snake_case name of the variant (wire `data.kind`).
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::InvalidModelId { .. } => "invalid_model_id",
            Self::InvalidArgument(_) => "invalid_argument",
            Self::Gguf(_) => "invalid_gguf",
            Self::IncompatibleModel { .. } => "incompatible_model",
            Self::ModelNotFound { .. } => "model_not_found",
            Self::AlreadyExists { .. } => "already_exists",
            Self::Corrupt { .. } => "corrupt",
            Self::HashMismatch { .. } => "hash_mismatch",
            Self::MissingFile { .. } => "missing_file",
            Self::UnsupportedPlatform { .. } => "unsupported_platform",
            Self::RuntimeNotInstalled { .. } => "runtime_not_installed",
            Self::RuntimeNotInCatalog { .. } => "runtime_not_in_catalog",
            Self::UnsupportedArchiveFormat { .. } => "unsupported_archive_format",
            Self::UnsafeArchive { .. } => "unsafe_archive",
            Self::RuntimeCorrupt { .. } => "runtime_corrupt",
            Self::InsufficientMemory { .. } => "insufficient_memory",
            Self::Sandbox { .. } => "sandbox",
            Self::ServerExited { .. } => "server_exited",
            Self::ServerTimeout { .. } => "server_timeout",
            Self::Busy { .. } => "busy",
            Self::Cancelled => "cancelled",
            Self::OfflineRefused { .. } => "offline_refused",
            Self::Download { .. } => "download_failed",
            Self::Provider(_) => "provider",
            Self::Io { .. } => "io",
        }
    }

    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::InvalidModelId { .. }
            | Self::InvalidArgument(_)
            | Self::Gguf(_)
            | Self::RuntimeNotInCatalog { .. }
            | Self::UnsupportedArchiveFormat { .. }
            | Self::UnsafeArchive { .. } => ServiceErrorCode::InvalidArgument,
            Self::IncompatibleModel { .. } => ServiceErrorCode::IncompatibleVersion,
            Self::ModelNotFound { .. } => ServiceErrorCode::NotFound,
            Self::AlreadyExists { .. } | Self::Busy { .. } => ServiceErrorCode::Conflict,
            Self::OfflineRefused { .. } => ServiceErrorCode::PermissionDenied,
            Self::Sandbox { code, .. } => *code,
            Self::Provider(error) => error.code(),
            Self::Corrupt { .. }
            | Self::HashMismatch { .. }
            | Self::MissingFile { .. }
            | Self::UnsupportedPlatform { .. }
            | Self::RuntimeNotInstalled { .. }
            | Self::RuntimeCorrupt { .. }
            | Self::InsufficientMemory { .. }
            | Self::ServerExited { .. }
            | Self::ServerTimeout { .. }
            | Self::Cancelled
            | Self::Download { .. }
            | Self::Io { .. } => ServiceErrorCode::Unavailable,
        }
    }

    /// What the user can do about it, when there is something to do.
    pub fn advice(&self) -> Option<String> {
        match self {
            Self::ModelNotFound { .. } => Some(
                "import a GGUF model with `aidex model import <FILE>`; list models with `aidex model list`"
                    .into(),
            ),
            Self::RuntimeNotInstalled { .. } => Some(
                "install the pinned llama.cpp runtime with `aidex runtime install <ARCHIVE>`; see `aidex runtime list`"
                    .into(),
            ),
            Self::RuntimeNotInCatalog { .. } => Some(
                "only pinned llama.cpp release archives can be installed; `aidex runtime list` shows them"
                    .into(),
            ),
            Self::AlreadyExists { .. } => Some(
                "choose another id with `--id`, or remove the existing model with `aidex model remove <ID> --yes`"
                    .into(),
            ),
            Self::HashMismatch { .. } | Self::MissingFile { .. } => Some(
                "the file changed after import; remove the model and import it again from a trusted copy"
                    .into(),
            ),
            Self::RuntimeCorrupt { .. } => Some(
                "reinstall the runtime from the pinned release archive with `aidex runtime install <ARCHIVE>`"
                    .into(),
            ),
            Self::InsufficientMemory { .. } => Some(
                "close other applications, request a smaller context with `--context`, or import a smaller quantization"
                    .into(),
            ),
            Self::UnsupportedPlatform { .. } => Some(
                "local inference is validated on Windows x64; other platforms need a pinned runtime entry".into(),
            ),
            Self::UnsupportedArchiveFormat { .. } => Some(
                "this build can install .zip runtime archives only (the Windows llama.cpp releases)".into(),
            ),
            Self::ServerExited { log: Some(log), .. } | Self::ServerTimeout { log: Some(log), .. } => {
                Some(format!("see the server log at {}", log.display()))
            }
            Self::OfflineRefused { .. } => Some(
                "copy the archive to this machine and run `aidex runtime install <ARCHIVE>`, or go online with `aidex offline off`"
                    .into(),
            ),
            _ => None,
        }
    }

    /// The equivalent provider failure (for [`aidex_providers::ModelProvider::chat`]).
    pub fn into_provider_error(self) -> ProviderError {
        match self {
            Self::Provider(error) => error,
            Self::Cancelled => ProviderError::Cancelled,
            Self::ModelNotFound { id } => ProviderError::ModelNotFound { model: id },
            Self::InvalidArgument(message) => ProviderError::InvalidRequest(message),
            other => {
                let mut reason = other.to_string();
                if let Some(advice) = other.advice() {
                    reason.push_str("; ");
                    reason.push_str(&advice);
                }
                ProviderError::Unavailable { reason }
            }
        }
    }
}

impl fmt::Display for InferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidModelId { id, reason } => write!(f, "invalid model id `{id}`: {reason}"),
            Self::InvalidArgument(message) => f.write_str(message),
            Self::Gguf(error) => write!(f, "not a usable GGUF model: {error}"),
            Self::IncompatibleModel { reason } => {
                write!(f, "the model cannot run on the pinned runtime: {reason}")
            }
            Self::ModelNotFound { id } => write!(f, "local model `{id}` is not registered"),
            Self::AlreadyExists { id, path } => write!(
                f,
                "model `{id}` already exists at {}; import never overwrites existing models",
                path.display()
            ),
            Self::Corrupt { path, reason } => write!(f, "{} is invalid: {reason}", path.display()),
            Self::HashMismatch {
                what,
                expected,
                actual,
            } => write!(
                f,
                "{what} failed verification: SHA-256 is {actual}, expected {expected}; refusing to use it"
            ),
            Self::MissingFile { what, path } => {
                write!(f, "{what} is missing: {}", path.display())
            }
            Self::UnsupportedPlatform { os, arch } => {
                write!(f, "local inference has no runtime platform for {os}/{arch}")
            }
            Self::RuntimeNotInstalled { platform, reason } => write!(
                f,
                "no usable llama.cpp runtime is installed for {platform}: {reason}"
            ),
            Self::RuntimeNotInCatalog { sha256 } => write!(
                f,
                "archive with SHA-256 {sha256} is not a pinned runtime in the catalog"
            ),
            Self::UnsupportedArchiveFormat { format } => {
                write!(
                    f,
                    "runtime archives in {format} format cannot be installed yet"
                )
            }
            Self::UnsafeArchive { entry, reason } => {
                write!(f, "refusing archive entry `{entry}`: {reason}")
            }
            Self::RuntimeCorrupt { runtime, reason } => {
                write!(f, "runtime {runtime} failed verification: {reason}")
            }
            Self::InsufficientMemory { explanation, .. } => {
                write!(f, "not enough memory to run the model: {explanation}")
            }
            Self::Sandbox { message, .. } => write!(f, "cannot launch llama-server: {message}"),
            Self::ServerExited { code, detail, .. } => match code {
                Some(code) => write!(f, "llama-server exited with code {code}: {detail}"),
                None => write!(f, "llama-server stopped: {detail}"),
            },
            Self::ServerTimeout { after, .. } => write!(
                f,
                "llama-server did not become ready within {} s",
                after.as_secs()
            ),
            Self::Busy { reason } => write!(f, "local inference is busy: {reason}"),
            Self::Cancelled => f.write_str("cancelled"),
            Self::OfflineRefused { host } => {
                write!(f, "offline mode refused the download from {host}")
            }
            Self::Download { reason } => write!(f, "download failed: {reason}"),
            Self::Provider(error) => error.fmt(f),
            Self::Io { context, message } => write!(f, "{context}: {message}"),
        }
    }
}

impl std::error::Error for InferenceError {}

impl From<GgufError> for InferenceError {
    fn from(error: GgufError) -> Self {
        Self::Gguf(error)
    }
}

impl From<ProviderError> for InferenceError {
    fn from(error: ProviderError) -> Self {
        Self::Provider(error)
    }
}

impl From<aidex_sandbox::SandboxError> for InferenceError {
    fn from(error: aidex_sandbox::SandboxError) -> Self {
        Self::Sandbox {
            code: error.code(),
            message: error.to_string(),
        }
    }
}

impl From<aidex_drive::DriveError> for InferenceError {
    fn from(error: aidex_drive::DriveError) -> Self {
        Self::Io {
            context: "drive".into(),
            message: error.to_string(),
        }
    }
}

pub type Result<T, E = InferenceError> = std::result::Result<T, E>;
