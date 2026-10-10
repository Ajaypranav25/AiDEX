//! Typed client failures.

use crate::{
    endpoint::EndpointError,
    errors::{ErrorCode, RpcError},
};
use aidex_core::ServiceErrorCode;
use serde_json::Value;
use std::{fmt, io};

#[derive(Debug)]
pub enum ClientError {
    /// No endpoint, an unreachable endpoint, or the daemon closed the connection.
    Unavailable(String),
    /// The capability was rejected or could not be unlocked for this user.
    Unauthorized(String),
    /// The daemon speaks a different IPC contract version.
    Incompatible {
        message: String,
        supported: Vec<u32>,
    },
    /// The daemon answered with a JSON-RPC error (see [`ErrorCode`]).
    Rpc {
        code: i32,
        message: String,
        data: Option<Value>,
    },
    Io(io::Error),
    /// The daemon violated the wire protocol.
    Protocol(String),
}

impl ClientError {
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Unavailable(_) | Self::Io(_) => ServiceErrorCode::Unavailable,
            Self::Unauthorized(_) => ServiceErrorCode::PermissionDenied,
            Self::Incompatible { .. } => ServiceErrorCode::IncompatibleVersion,
            Self::Rpc { code, .. } => ErrorCode::from_code(*code)
                .map_or(ServiceErrorCode::Internal, ErrorCode::service_code),
            Self::Protocol(_) => ServiceErrorCode::Internal,
        }
    }

    /// The error-table entry of an `Rpc` error.
    pub fn rpc_kind(&self) -> Option<ErrorCode> {
        match self {
            Self::Rpc { code, .. } => ErrorCode::from_code(*code),
            _ => None,
        }
    }

    pub(super) fn from_rpc(error: RpcError) -> Self {
        match error.kind() {
            Some(ErrorCode::Unauthorized) => Self::Unauthorized(error.message),
            Some(ErrorCode::IncompatibleVersion) => Self::Incompatible {
                supported: error
                    .data
                    .as_ref()
                    .and_then(|data| data.get("supported"))
                    .and_then(|value| serde_json::from_value(value.clone()).ok())
                    .unwrap_or_default(),
                message: error.message,
            },
            _ => Self::Rpc {
                code: error.code,
                message: error.message,
                data: error.data,
            },
        }
    }

    pub(super) fn from_endpoint(error: EndpointError) -> Self {
        match error {
            EndpointError::Missing(path) => Self::Unavailable(format!(
                "daemon is not running (no endpoint at {})",
                path.display()
            )),
            EndpointError::Incompatible { found } => Self::Incompatible {
                message: error.to_string(),
                supported: vec![found],
            },
            EndpointError::Protection(message) => Self::Unauthorized(message),
            EndpointError::Io { .. } => Self::Unavailable(error.to_string()),
            other => Self::Protocol(other.to_string()),
        }
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) | Self::Incompatible { message, .. } => f.write_str(message),
            Self::Unauthorized(message) => write!(f, "daemon rejected this client: {message}"),
            Self::Rpc { code, message, .. } => match ErrorCode::from_code(*code) {
                Some(kind) => write!(f, "{}: {message}", kind.name()),
                None => write!(f, "daemon error {code}: {message}"),
            },
            Self::Io(error) => write!(f, "daemon connection failed: {error}"),
            Self::Protocol(message) => write!(f, "daemon protocol error: {message}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}
