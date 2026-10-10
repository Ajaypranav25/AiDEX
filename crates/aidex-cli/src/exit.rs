//! Stable process exit codes and the error type every command returns.
//!
//! The numeric values are a public contract documented in `apps/cli/README.md`;
//! they never change meaning. Add new codes only at the end.

use aidex_drive::DriveError;
use aidexd::{ClientError, DaemonError, EndpointError, ErrorCode};
use std::{fmt, process::ExitCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Success,
    /// Any failure without a more specific code.
    Failure,
    /// Invalid command line or arguments rejected by the daemon.
    Usage,
    /// No daemon, unreachable daemon, or the daemon closed the connection.
    DaemonUnavailable,
    /// Drive missing, not removable, unwritable, or its manifest is invalid.
    Drive,
    /// Vault locked or authentication (including daemon capability) failed.
    Auth,
    /// Denied by policy or permissions.
    PolicyDenied,
    /// No usable model or provider.
    ModelUnavailable,
    NotFound,
    /// Cancelled or interrupted.
    Cancelled,
    /// CLI and daemon (or data) versions are incompatible.
    Incompatible,
}

impl Exit {
    pub const ALL: [Self; 11] = [
        Self::Success,
        Self::Failure,
        Self::Usage,
        Self::DaemonUnavailable,
        Self::Drive,
        Self::Auth,
        Self::PolicyDenied,
        Self::ModelUnavailable,
        Self::NotFound,
        Self::Cancelled,
        Self::Incompatible,
    ];

    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failure => 1,
            Self::Usage => 2,
            Self::DaemonUnavailable => 3,
            Self::Drive => 4,
            Self::Auth => 5,
            Self::PolicyDenied => 6,
            Self::ModelUnavailable => 7,
            Self::NotFound => 8,
            Self::Cancelled => 9,
            Self::Incompatible => 10,
        }
    }

    /// Stable name used as `error.code` in `--json` output.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Usage => "usage",
            Self::DaemonUnavailable => "daemon_unavailable",
            Self::Drive => "drive_error",
            Self::Auth => "auth_failed",
            Self::PolicyDenied => "policy_denied",
            Self::ModelUnavailable => "model_unavailable",
            Self::NotFound => "not_found",
            Self::Cancelled => "cancelled",
            Self::Incompatible => "incompatible_version",
        }
    }

    /// Exit code for a daemon JSON-RPC error.
    pub fn for_rpc(kind: Option<ErrorCode>) -> Self {
        match kind {
            Some(ErrorCode::InvalidParams | ErrorCode::InvalidArgument) => Self::Usage,
            Some(ErrorCode::NotFound) => Self::NotFound,
            Some(ErrorCode::PermissionDenied | ErrorCode::PolicyDenied) => Self::PolicyDenied,
            Some(
                ErrorCode::Unauthorized | ErrorCode::VaultLocked | ErrorCode::HandshakeRequired,
            ) => Self::Auth,
            Some(ErrorCode::ModelUnavailable) => Self::ModelUnavailable,
            Some(ErrorCode::Cancelled) => Self::Cancelled,
            Some(ErrorCode::IncompatibleVersion | ErrorCode::MethodNotFound) => Self::Incompatible,
            Some(
                ErrorCode::Unavailable
                | ErrorCode::ShuttingDown
                | ErrorCode::ServerBusy
                | ErrorCode::SlowConsumer,
            ) => Self::DaemonUnavailable,
            Some(
                ErrorCode::ParseError
                | ErrorCode::InvalidRequest
                | ErrorCode::InternalError
                | ErrorCode::Conflict
                | ErrorCode::MessageTooLarge,
            )
            | None => Self::Failure,
        }
    }
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        ExitCode::from(exit.code())
    }
}

/// A command failure: exit code, message, and an optional next step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliError {
    pub exit: Exit,
    pub message: String,
    pub hint: Option<String>,
}

impl CliError {
    pub fn new(exit: Exit, message: impl Into<String>) -> Self {
        Self {
            exit,
            message: message.into(),
            hint: None,
        }
    }

    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

impl From<DriveError> for CliError {
    fn from(error: DriveError) -> Self {
        Self::new(Exit::Drive, error.to_string())
    }
}

impl From<ClientError> for CliError {
    fn from(error: ClientError) -> Self {
        let exit = match &error {
            ClientError::Unavailable(_) | ClientError::Io(_) => Exit::DaemonUnavailable,
            ClientError::Unauthorized(_) => Exit::Auth,
            ClientError::Incompatible { .. } => Exit::Incompatible,
            ClientError::Rpc { .. } => Exit::for_rpc(error.rpc_kind()),
            ClientError::Protocol(_) => Exit::Failure,
        };
        let cli = Self::new(exit, error.to_string());
        match exit {
            Exit::Incompatible => cli.with_hint(
                "the running daemon and this CLI speak different IPC versions; stop the daemon with the CLI that started it, then run `aidex daemon start`",
            ),
            _ => cli,
        }
    }
}

impl From<DaemonError> for CliError {
    fn from(error: DaemonError) -> Self {
        match error {
            DaemonError::Client(client) => client.into(),
            DaemonError::DriveUnavailable(_) => Self::new(Exit::Drive, error.to_string()),
            DaemonError::NotReady(_) | DaemonError::StopTimeout(_) => {
                Self::new(Exit::DaemonUnavailable, error.to_string())
            }
            DaemonError::Endpoint(EndpointError::Incompatible { .. }) => {
                Self::new(Exit::Incompatible, error.to_string())
            }
            other => Self::new(Exit::Failure, other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn exit_codes_are_the_documented_stable_table() {
        let codes: Vec<u8> = Exit::ALL.iter().map(|exit| exit.code()).collect();
        assert_eq!(codes, (0..=10).collect::<Vec<u8>>());
        let names: HashSet<_> = Exit::ALL.iter().map(|exit| exit.name()).collect();
        assert_eq!(names.len(), Exit::ALL.len());
        assert_eq!(Exit::DaemonUnavailable.code(), 3);
        assert_eq!(Exit::Incompatible.code(), 10);
    }

    #[test]
    fn client_errors_map_to_actionable_exit_codes() {
        let rpc = |kind: ErrorCode| ClientError::Rpc {
            code: kind.code(),
            message: "m".into(),
            data: None,
        };
        for (error, expected) in [
            (
                ClientError::Unavailable("down".into()),
                Exit::DaemonUnavailable,
            ),
            (ClientError::Unauthorized("no".into()), Exit::Auth),
            (
                ClientError::Incompatible {
                    message: "v1".into(),
                    supported: vec![1],
                },
                Exit::Incompatible,
            ),
            (ClientError::Protocol("bad".into()), Exit::Failure),
            (rpc(ErrorCode::NotFound), Exit::NotFound),
            (rpc(ErrorCode::InvalidParams), Exit::Usage),
            (rpc(ErrorCode::PolicyDenied), Exit::PolicyDenied),
            (rpc(ErrorCode::VaultLocked), Exit::Auth),
            (rpc(ErrorCode::ModelUnavailable), Exit::ModelUnavailable),
            (rpc(ErrorCode::Cancelled), Exit::Cancelled),
            (rpc(ErrorCode::MethodNotFound), Exit::Incompatible),
            (rpc(ErrorCode::ShuttingDown), Exit::DaemonUnavailable),
            (rpc(ErrorCode::InternalError), Exit::Failure),
            (
                ClientError::Rpc {
                    code: -1,
                    message: "unknown".into(),
                    data: None,
                },
                Exit::Failure,
            ),
        ] {
            assert_eq!(CliError::from(error).exit, expected);
        }
    }
}
