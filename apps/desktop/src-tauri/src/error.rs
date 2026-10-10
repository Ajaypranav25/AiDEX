//! `BridgeError`: the one error shape every desktop command returns to the UI.
//!
//! Daemon errors keep the daemon's stable JSON-RPC code, its error-table name, its
//! message, and its `data`, so screens can branch on `kind` (for example
//! `method_not_found` or `vault_locked`) exactly as the CLI does. Messages come from
//! backend errors that never carry secrets.

use aidex_core::ServiceErrorCode;
use aidex_drive::DriveError;
use aidexd::{ClientError, DaemonError, ErrorCode};
use serde::Serialize;
use serde_json::Value;
use std::fmt;

/// Bridge-level kinds that are not daemon error-table entries.
pub mod kind {
    /// No AiDEX drive is resolved for this session.
    pub const NO_DRIVE: &str = "no_drive";
    /// The daemon is not running or its connection closed.
    pub const DAEMON_UNAVAILABLE: &str = "daemon_unavailable";
    /// The `aidexd` executable was not found next to the app or on `PATH`.
    pub const DAEMON_NOT_FOUND: &str = "daemon_not_found";
    /// The daemon could not be launched or did not become ready.
    pub const DAEMON_LAUNCH: &str = "daemon_launch";
    /// The daemon connection failed below the JSON-RPC layer.
    pub const IO: &str = "io";
    /// The daemon violated the wire protocol.
    pub const PROTOCOL: &str = "protocol";
    /// Too many live subscriptions in this window.
    pub const SUBSCRIPTION_LIMIT: &str = "subscription_limit";
    /// The UI asked for a reserved or malformed method name.
    pub const INVALID_METHOD: &str = "invalid_method";
    /// A bridge worker thread failed.
    pub const INTERNAL: &str = "internal";
}

/// A failure reported to the UI.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BridgeError {
    /// Stable cross-service category (`aidex_core::ServiceErrorCode`, snake_case).
    pub code: &'static str,
    /// Finer stable identifier: the daemon error-table name (`method_not_found`,
    /// `unavailable`, `vault_locked`, ...), a drive error kind (`not_removable`,
    /// `missing_manifest`, ...), or one of [`kind`].
    pub kind: String,
    /// The numeric JSON-RPC code when the daemon answered with an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_code: Option<i32>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl BridgeError {
    pub fn new(
        code: ServiceErrorCode,
        kind: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: service_code_name(code),
            kind: kind.into(),
            rpc_code: None,
            message: message.into(),
            data: None,
        }
    }

    pub fn no_drive() -> Self {
        Self::new(
            ServiceErrorCode::Unavailable,
            kind::NO_DRIVE,
            "no AiDEX drive is selected; set AIDEX_DRIVE_ROOT, run AiDEX from its drive, or choose a drive",
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::Internal, kind::INTERNAL, message)
    }

    /// Whether the failure means the connection is unusable (drop it and reconnect).
    pub fn is_connection_failure(&self) -> bool {
        matches!(
            self.kind.as_str(),
            kind::DAEMON_UNAVAILABLE | kind::IO | kind::PROTOCOL
        ) || self.rpc_code.is_some_and(|code| {
            matches!(
                ErrorCode::from_code(code),
                Some(ErrorCode::ShuttingDown | ErrorCode::SlowConsumer)
            )
        })
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): {}", self.kind, self.code, self.message)
    }
}

impl std::error::Error for BridgeError {}

impl From<ClientError> for BridgeError {
    fn from(error: ClientError) -> Self {
        let code = service_code_name(error.code());
        let message = error.to_string();
        match error {
            ClientError::Rpc {
                code: rpc_code,
                data,
                message,
            } => Self {
                code,
                kind: ErrorCode::from_code(rpc_code)
                    .map_or_else(|| format!("rpc_{rpc_code}"), |known| known.name().into()),
                rpc_code: Some(rpc_code),
                message,
                data,
            },
            ClientError::Unavailable(_) => Self {
                code,
                kind: kind::DAEMON_UNAVAILABLE.into(),
                rpc_code: None,
                message,
                data: None,
            },
            ClientError::Unauthorized(_) => Self {
                code,
                kind: ErrorCode::Unauthorized.name().into(),
                rpc_code: None,
                message,
                data: None,
            },
            ClientError::Incompatible { supported, .. } => Self {
                code,
                kind: ErrorCode::IncompatibleVersion.name().into(),
                rpc_code: None,
                message,
                data: Some(serde_json::json!({ "supported": supported })),
            },
            ClientError::Io(_) => Self {
                code,
                kind: kind::IO.into(),
                rpc_code: None,
                message,
                data: None,
            },
            ClientError::Protocol(_) => Self {
                code,
                kind: kind::PROTOCOL.into(),
                rpc_code: None,
                message,
                data: None,
            },
        }
    }
}

impl From<DriveError> for BridgeError {
    fn from(error: DriveError) -> Self {
        let kind = match &error {
            DriveError::InvalidRoot(_) => "invalid_root",
            DriveError::NotRemovable(_) => "not_removable",
            DriveError::ReadOnly(_) => "read_only",
            DriveError::LowSpace { .. } => "low_space",
            DriveError::MissingManifest(_) => "missing_manifest",
            DriveError::CorruptManifest(..) => "corrupt_manifest",
            DriveError::Io(_) => "drive_io",
            DriveError::Discovery(_) => "discovery",
            DriveError::MigrationRequired(_) => "migration_required",
            DriveError::MigrationConflict(_) => "migration_conflict",
            DriveError::MigrationFailed(_) => "migration_failed",
            DriveError::UnmanagedPath(_) => "unmanaged_path",
            DriveError::UnsafeComponent(_) => "unsafe_component",
        };
        Self::new(error.code(), kind, error.to_string())
    }
}

impl From<DaemonError> for BridgeError {
    fn from(error: DaemonError) -> Self {
        match error {
            DaemonError::Client(client) => client.into(),
            other => {
                let kind = match &other {
                    DaemonError::AlreadyRunning { .. } => "already_running",
                    DaemonError::EndpointBusy(_) => "endpoint_busy",
                    DaemonError::DriveUnavailable(_) => "drive_unavailable",
                    DaemonError::StopTimeout(_) => "stop_timeout",
                    DaemonError::Services(_) => "services",
                    _ => kind::DAEMON_LAUNCH,
                };
                Self::new(other.code(), kind, other.to_string())
            }
        }
    }
}

/// The snake_case wire name of a service category.
pub fn service_code_name(code: ServiceErrorCode) -> &'static str {
    match code {
        ServiceErrorCode::InvalidArgument => "invalid_argument",
        ServiceErrorCode::NotFound => "not_found",
        ServiceErrorCode::PermissionDenied => "permission_denied",
        ServiceErrorCode::Conflict => "conflict",
        ServiceErrorCode::IncompatibleVersion => "incompatible_version",
        ServiceErrorCode::Unavailable => "unavailable",
        ServiceErrorCode::Internal => "internal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn daemon_rpc_errors_keep_their_stable_code_kind_message_and_data() {
        let error: BridgeError = ClientError::Rpc {
            code: ErrorCode::MethodNotFound.code(),
            message: "method `vault.status` is not available".into(),
            data: Some(json!({ "method": "vault.status" })),
        }
        .into();
        assert_eq!(error.code, "not_found");
        assert_eq!(error.kind, "method_not_found");
        assert_eq!(error.rpc_code, Some(-32601));
        assert_eq!(error.message, "method `vault.status` is not available");
        assert_eq!(error.data, Some(json!({ "method": "vault.status" })));
        assert!(!error.is_connection_failure());

        let unknown: BridgeError = ClientError::Rpc {
            code: -32999,
            message: "future".into(),
            data: None,
        }
        .into();
        assert_eq!(unknown.kind, "rpc_-32999");
        assert_eq!(unknown.code, "internal");
    }

    #[test]
    fn connection_level_failures_are_marked_for_reconnection() {
        let unavailable: BridgeError =
            ClientError::Unavailable("daemon is not running".into()).into();
        assert_eq!(unavailable.code, "unavailable");
        assert_eq!(unavailable.kind, kind::DAEMON_UNAVAILABLE);
        assert!(unavailable.is_connection_failure());

        let io: BridgeError = ClientError::Io(std::io::Error::other("reset")).into();
        assert!(io.is_connection_failure());
        let shutting_down: BridgeError = ClientError::Rpc {
            code: ErrorCode::ShuttingDown.code(),
            message: "stopping".into(),
            data: None,
        }
        .into();
        assert!(shutting_down.is_connection_failure());

        let unauthorized: BridgeError = ClientError::Unauthorized("bad capability".into()).into();
        assert_eq!(unauthorized.code, "permission_denied");
        assert_eq!(unauthorized.kind, "unauthorized");
        assert!(!unauthorized.is_connection_failure());

        let incompatible: BridgeError = ClientError::Incompatible {
            message: "contract v1".into(),
            supported: vec![1],
        }
        .into();
        assert_eq!(incompatible.code, "incompatible_version");
        assert_eq!(incompatible.kind, "incompatible_version");
        assert_eq!(incompatible.data, Some(json!({ "supported": [1] })));
    }

    #[test]
    fn drive_errors_map_to_their_service_code_and_a_stable_kind() {
        let error: BridgeError = DriveError::NotRemovable("C:\\".into()).into();
        assert_eq!(error.code, "invalid_argument");
        assert_eq!(error.kind, "not_removable");
        let error: BridgeError = DriveError::MigrationRequired("H:\\".into()).into();
        assert_eq!(error.code, "incompatible_version");
        assert_eq!(error.kind, "migration_required");
        assert!(error.message.contains("aidex init"));
    }

    #[test]
    fn errors_serialize_without_absent_optional_fields() {
        let value = serde_json::to_value(BridgeError::no_drive()).unwrap();
        assert_eq!(value["code"], "unavailable");
        assert_eq!(value["kind"], "no_drive");
        assert!(value.get("rpc_code").is_none());
        assert!(value.get("data").is_none());
    }
}
