//! Sandbox error type with stable service codes.

use std::{error::Error, fmt};

use aidex_core::ServiceErrorCode;

use crate::{Requirement, SandboxLevel};

/// Every failure the sandbox reports. Messages never contain environment values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SandboxError {
    /// The request is malformed or internally inconsistent.
    InvalidRequest(String),
    /// Level 0 (direct) was requested without the explicit `allow_direct` opt-in.
    DirectNotAllowed,
    /// The requested isolation cannot be enforced on this host. Nothing was launched.
    Unsupported {
        level: SandboxLevel,
        missing: Vec<Requirement>,
    },
    /// The `aidex-sandbox-exec` helper is missing or unusable.
    HelperUnavailable(String),
    /// The program could not be resolved to an absolute executable path.
    ProgramNotFound(String),
    /// The plan was produced for a different backend or helper than this sandbox.
    PlanMismatch(String),
    /// The registry is closed (shutdown, emergency stop, or drive removal in progress).
    RegistryClosed,
    /// The operating system refused to start the helper process.
    Spawn(String),
    /// The helper could not apply the negotiated restrictions; the target did not run
    /// unconfined.
    EnforcementFailed { stage: String, message: String },
    /// The operation is not available on this platform.
    NotSupported(&'static str),
    /// Any other operating-system failure while managing a process.
    Io(String),
}

impl SandboxError {
    /// Stable service code for API mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::InvalidRequest(_) | Self::PlanMismatch(_) => ServiceErrorCode::InvalidArgument,
            Self::ProgramNotFound(_) => ServiceErrorCode::NotFound,
            Self::DirectNotAllowed => ServiceErrorCode::PermissionDenied,
            Self::Unsupported { .. }
            | Self::HelperUnavailable(_)
            | Self::RegistryClosed
            | Self::EnforcementFailed { .. }
            | Self::NotSupported(_) => ServiceErrorCode::Unavailable,
            Self::Spawn(_) | Self::Io(_) => ServiceErrorCode::Internal,
        }
    }

    /// Stable machine-readable identifier, finer grained than [`Self::code`].
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "sandbox.invalid_request",
            Self::DirectNotAllowed => "sandbox.direct_not_allowed",
            Self::Unsupported { .. } => "sandbox.unsupported",
            Self::HelperUnavailable(_) => "sandbox.helper_unavailable",
            Self::ProgramNotFound(_) => "sandbox.program_not_found",
            Self::PlanMismatch(_) => "sandbox.plan_mismatch",
            Self::RegistryClosed => "sandbox.registry_closed",
            Self::Spawn(_) => "sandbox.spawn_failed",
            Self::EnforcementFailed { .. } => "sandbox.enforcement_failed",
            Self::NotSupported(_) => "sandbox.not_supported",
            Self::Io(_) => "sandbox.io",
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidRequest(message.into())
    }

    pub(crate) fn io(context: &str, error: &std::io::Error) -> Self {
        Self::Io(format!("{context}: {error}"))
    }
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "invalid sandbox request: {message}"),
            Self::DirectNotAllowed => {
                f.write_str("direct execution (level 0) requires an explicit allow_direct opt-in")
            }
            Self::Unsupported { level, missing } => {
                write!(
                    f,
                    "sandbox level {level} cannot be enforced on this host; missing: "
                )?;
                for (index, requirement) in missing.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(requirement.as_str())?;
                }
                Ok(())
            }
            Self::HelperUnavailable(message) => {
                write!(f, "sandbox helper is unavailable: {message}")
            }
            Self::ProgramNotFound(message) => write!(f, "program not found: {message}"),
            Self::PlanMismatch(message) => write!(f, "enforcement plan mismatch: {message}"),
            Self::RegistryClosed => {
                f.write_str("process registry is closed; no new sandboxed processes may start")
            }
            Self::Spawn(message) => write!(f, "failed to start sandbox helper: {message}"),
            Self::EnforcementFailed { stage, message } => {
                write!(f, "sandbox enforcement failed during {stage}: {message}")
            }
            Self::NotSupported(operation) => {
                write!(f, "{operation} is not supported on this platform")
            }
            Self::Io(message) => write!(f, "sandbox I/O error: {message}"),
        }
    }
}

impl Error for SandboxError {}
