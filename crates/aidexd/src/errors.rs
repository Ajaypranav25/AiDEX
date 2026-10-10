//! Stable JSON-RPC error codes for the AiDEX IPC contract.
//!
//! The numeric values are part of contract version 2 and never change meaning.
//! New codes may be appended; existing codes are never renumbered or reused.
//! The table is documented in `protocol/commands/README.md`.

use aidex_core::{ServiceError, ServiceErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Every error code the daemon may return, with its stable wire number.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ErrorCode {
    /// `-32700`: the line was not valid UTF-8 JSON.
    ParseError,
    /// `-32600`: valid JSON that is not an acceptable JSON-RPC 2.0 message.
    InvalidRequest,
    /// `-32601`: no handler is registered for the method.
    MethodNotFound,
    /// `-32602`: the method exists but its parameters are invalid.
    InvalidParams,
    /// `-32603`: the daemon failed internally; maps `ServiceErrorCode::Internal`.
    InternalError,
    /// `-32001`: maps `ServiceErrorCode::InvalidArgument`.
    InvalidArgument,
    /// `-32002`: maps `ServiceErrorCode::NotFound`.
    NotFound,
    /// `-32003`: maps `ServiceErrorCode::PermissionDenied`.
    PermissionDenied,
    /// `-32004`: maps `ServiceErrorCode::Conflict`.
    Conflict,
    /// `-32005`: maps `ServiceErrorCode::IncompatibleVersion`.
    IncompatibleVersion,
    /// `-32006`: maps `ServiceErrorCode::Unavailable`.
    Unavailable,
    /// `-32010`: the handshake capability was wrong.
    Unauthorized,
    /// `-32011`: the first message was not an `aidex.handshake` request.
    HandshakeRequired,
    /// `-32012`: a message exceeded the 1 MiB line limit.
    MessageTooLarge,
    /// `-32013`: the connection or subscription limit was reached.
    ServerBusy,
    /// `-32014`: the daemon is stopping or its drive was removed.
    ShuttingDown,
    /// `-32015`: the client did not drain its bounded outbound queue.
    SlowConsumer,
    /// `-32020`: reserved for the vault: locked or credential required.
    VaultLocked,
    /// `-32021`: reserved for the broker/policy engine: denied by policy.
    PolicyDenied,
    /// `-32022`: reserved for models/providers: no usable model or provider.
    ModelUnavailable,
    /// `-32023`: the operation was cancelled or interrupted.
    Cancelled,
}

impl ErrorCode {
    /// All codes in table order.
    pub const ALL: [Self; 21] = [
        Self::ParseError,
        Self::InvalidRequest,
        Self::MethodNotFound,
        Self::InvalidParams,
        Self::InternalError,
        Self::InvalidArgument,
        Self::NotFound,
        Self::PermissionDenied,
        Self::Conflict,
        Self::IncompatibleVersion,
        Self::Unavailable,
        Self::Unauthorized,
        Self::HandshakeRequired,
        Self::MessageTooLarge,
        Self::ServerBusy,
        Self::ShuttingDown,
        Self::SlowConsumer,
        Self::VaultLocked,
        Self::PolicyDenied,
        Self::ModelUnavailable,
        Self::Cancelled,
    ];

    /// The stable numeric wire code.
    pub const fn code(self) -> i32 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            Self::InvalidArgument => -32001,
            Self::NotFound => -32002,
            Self::PermissionDenied => -32003,
            Self::Conflict => -32004,
            Self::IncompatibleVersion => -32005,
            Self::Unavailable => -32006,
            Self::Unauthorized => -32010,
            Self::HandshakeRequired => -32011,
            Self::MessageTooLarge => -32012,
            Self::ServerBusy => -32013,
            Self::ShuttingDown => -32014,
            Self::SlowConsumer => -32015,
            Self::VaultLocked => -32020,
            Self::PolicyDenied => -32021,
            Self::ModelUnavailable => -32022,
            Self::Cancelled => -32023,
        }
    }

    /// Stable snake_case name, used in CLI JSON output and documentation.
    pub const fn name(self) -> &'static str {
        match self {
            Self::ParseError => "parse_error",
            Self::InvalidRequest => "invalid_request",
            Self::MethodNotFound => "method_not_found",
            Self::InvalidParams => "invalid_params",
            Self::InternalError => "internal",
            Self::InvalidArgument => "invalid_argument",
            Self::NotFound => "not_found",
            Self::PermissionDenied => "permission_denied",
            Self::Conflict => "conflict",
            Self::IncompatibleVersion => "incompatible_version",
            Self::Unavailable => "unavailable",
            Self::Unauthorized => "unauthorized",
            Self::HandshakeRequired => "handshake_required",
            Self::MessageTooLarge => "message_too_large",
            Self::ServerBusy => "server_busy",
            Self::ShuttingDown => "shutting_down",
            Self::SlowConsumer => "slow_consumer",
            Self::VaultLocked => "vault_locked",
            Self::PolicyDenied => "policy_denied",
            Self::ModelUnavailable => "model_unavailable",
            Self::Cancelled => "cancelled",
        }
    }

    /// Look up a wire code; unknown codes return `None`.
    pub fn from_code(code: i32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.code() == code)
    }

    /// The closest cross-service error category from `aidex-core`.
    pub const fn service_code(self) -> ServiceErrorCode {
        match self {
            Self::ParseError
            | Self::InvalidRequest
            | Self::InvalidParams
            | Self::InvalidArgument
            | Self::MessageTooLarge => ServiceErrorCode::InvalidArgument,
            Self::MethodNotFound | Self::NotFound => ServiceErrorCode::NotFound,
            Self::PermissionDenied
            | Self::Unauthorized
            | Self::HandshakeRequired
            | Self::VaultLocked
            | Self::PolicyDenied => ServiceErrorCode::PermissionDenied,
            Self::Conflict => ServiceErrorCode::Conflict,
            Self::IncompatibleVersion => ServiceErrorCode::IncompatibleVersion,
            Self::Unavailable
            | Self::ServerBusy
            | Self::ShuttingDown
            | Self::SlowConsumer
            | Self::ModelUnavailable
            | Self::Cancelled => ServiceErrorCode::Unavailable,
            Self::InternalError => ServiceErrorCode::Internal,
        }
    }
}

impl From<ServiceErrorCode> for ErrorCode {
    fn from(code: ServiceErrorCode) -> Self {
        match code {
            ServiceErrorCode::InvalidArgument => Self::InvalidArgument,
            ServiceErrorCode::NotFound => Self::NotFound,
            ServiceErrorCode::PermissionDenied => Self::PermissionDenied,
            ServiceErrorCode::Conflict => Self::Conflict,
            ServiceErrorCode::IncompatibleVersion => Self::IncompatibleVersion,
            ServiceErrorCode::Unavailable => Self::Unavailable,
            ServiceErrorCode::Internal => Self::InternalError,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name(), self.code())
    }
}

/// A JSON-RPC 2.0 error object.
///
/// Messages are shown to local users; never put secrets or credentials in
/// `message` or `data`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(kind: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: kind.code(),
            message: message.into(),
            data: None,
        }
    }

    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// The table entry for this error, or `None` for codes this build does not know.
    pub fn kind(&self) -> Option<ErrorCode> {
        ErrorCode::from_code(self.code)
    }

    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ParseError, message)
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message)
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            ErrorCode::MethodNotFound,
            format!("method `{method}` is not available"),
        )
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }

    /// Map a domain error category to its stable wire code.
    pub fn service(code: ServiceErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCode::from(code), message)
    }
}

impl From<ServiceError> for RpcError {
    fn from(error: ServiceError) -> Self {
        let mut rpc = Self::service(error.code, error.message);
        if let Some(request_id) = error.request_id {
            rpc.data = Some(serde_json::json!({ "request_id": request_id.as_str() }));
        }
        rpc
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            Some(kind) => write!(f, "{}: {}", kind.name(), self.message),
            None => write!(f, "error {}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for RpcError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn wire_codes_and_names_are_unique_and_round_trip() {
        let codes: HashSet<_> = ErrorCode::ALL.iter().map(|c| c.code()).collect();
        let names: HashSet<_> = ErrorCode::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(codes.len(), ErrorCode::ALL.len());
        assert_eq!(names.len(), ErrorCode::ALL.len());
        for code in ErrorCode::ALL {
            assert_eq!(ErrorCode::from_code(code.code()), Some(code));
        }
        assert_eq!(ErrorCode::from_code(-1), None);
    }

    #[test]
    fn standard_and_service_codes_are_stable() {
        assert_eq!(ErrorCode::ParseError.code(), -32700);
        assert_eq!(ErrorCode::InvalidRequest.code(), -32600);
        assert_eq!(ErrorCode::MethodNotFound.code(), -32601);
        assert_eq!(ErrorCode::InvalidParams.code(), -32602);
        assert_eq!(ErrorCode::InternalError.code(), -32603);
        for (service, wire) in [
            (ServiceErrorCode::InvalidArgument, -32001),
            (ServiceErrorCode::NotFound, -32002),
            (ServiceErrorCode::PermissionDenied, -32003),
            (ServiceErrorCode::Conflict, -32004),
            (ServiceErrorCode::IncompatibleVersion, -32005),
            (ServiceErrorCode::Unavailable, -32006),
            (ServiceErrorCode::Internal, -32603),
        ] {
            let code = ErrorCode::from(service);
            assert_eq!(code.code(), wire);
            assert_eq!(code.service_code(), service);
        }
    }

    #[test]
    fn service_errors_convert_without_losing_category() {
        let error = ServiceError {
            code: ServiceErrorCode::NotFound,
            message: "workspace missing".into(),
            request_id: None,
        };
        let rpc = RpcError::from(error);
        assert_eq!(rpc.kind(), Some(ErrorCode::NotFound));
        assert_eq!(rpc.to_string(), "not_found: workspace missing");
        let json = serde_json::to_value(&rpc).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"code": -32002, "message": "workspace missing"})
        );
    }
}
