//! Provider outcomes that are not a successful response.
//!
//! Messages never contain credentials or server-supplied free text: HTTP failures
//! carry the status and a sanitized machine code only.

use std::{fmt, time::Duration};

use aidex_core::ServiceErrorCode;
use serde::{Deserialize, Serialize};

/// Recognized ChatGPT plan-route failures. Keep their recovery identity without
/// accepting arbitrary upstream strings across the error boundary.
pub(crate) fn chatgpt_error_status(code: &str) -> Option<u16> {
    match code {
        "subscription_sharing_user_not_eligible"
        | "subscription_sharing_route_not_supported"
        | "chatpass_v2_scope_not_authorized"
        | "chatpass_v2_invalid_authorization_context" => Some(403),
        "subscription_sharing_usage_limit_exceeded" => Some(429),
        "subscription_sharing_usage_unavailable" | "subscription_sharing_user_unavailable" => {
            Some(503)
        }
        "subscription_sharing_unsupported_capability" => Some(400),
        "subscription_sharing_invalid_user" => Some(401),
        _ => None,
    }
}

/// Why a credential could not be produced. Carries names, never secret values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CredentialError {
    /// The vault exists but is locked.
    Locked,
    /// No secret with this name exists.
    NotFound { secret_name: String },
    /// The secret's policy does not allow this provider as a consumer.
    Denied { secret_name: String },
    /// No credential source is available (for example no vault is wired).
    Unavailable { reason: String },
    /// The stored secret cannot be used as a bearer token.
    Malformed { secret_name: String },
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locked => f.write_str("the vault is locked; unlock it with `aidex vault unlock`"),
            Self::NotFound { secret_name } => {
                write!(f, "vault secret `{secret_name}` does not exist")
            }
            Self::Denied { secret_name } => write!(
                f,
                "vault secret `{secret_name}` is not allowed for this provider"
            ),
            Self::Unavailable { reason } => write!(f, "credentials are unavailable: {reason}"),
            Self::Malformed { secret_name } => write!(
                f,
                "vault secret `{secret_name}` is not a valid bearer token (printable ASCII)"
            ),
        }
    }
}

/// Stable, serializable name of a [`ProviderError`] variant.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    Unavailable,
    Unauthorized,
    CredentialUnavailable,
    RateLimited,
    ContextOverflow,
    OfflineRefused,
    PolicyDenied,
    Cancelled,
    Timeout,
    Protocol,
    Http,
    InvalidRequest,
    ModelNotFound,
}

impl ProviderErrorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Unauthorized => "unauthorized",
            Self::CredentialUnavailable => "credential_unavailable",
            Self::RateLimited => "rate_limited",
            Self::ContextOverflow => "context_overflow",
            Self::OfflineRefused => "offline_refused",
            Self::PolicyDenied => "policy_denied",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::Protocol => "protocol",
            Self::Http => "http",
            Self::InvalidRequest => "invalid_request",
            Self::ModelNotFound => "model_not_found",
        }
    }
}

impl fmt::Display for ProviderErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProviderError {
    /// The provider could not be reached or is not serving (connection refused,
    /// DNS failure, 503, provider not registered).
    Unavailable {
        reason: String,
    },
    /// The provider rejected the credential (HTTP 401/403).
    Unauthorized,
    /// The credential could not be obtained; nothing was sent.
    Credential(CredentialError),
    /// HTTP 429; `retry_after` from the `Retry-After` header when present.
    RateLimited {
        retry_after: Option<Duration>,
    },
    /// The prompt does not fit the model's context window.
    ContextOverflow {
        limit: Option<u32>,
        requested: Option<u32>,
    },
    /// Offline mode refused traffic to a remote host; no connection was opened.
    OfflineRefused {
        host: String,
    },
    /// A policy gate refused the call.
    PolicyDenied {
        reason: String,
    },
    Cancelled,
    /// A deadline or transport timeout expired.
    Timeout,
    /// The provider answered with something this client cannot interpret.
    Protocol(String),
    /// Any other HTTP failure; `code` is the provider's sanitized error code.
    Http {
        status: u16,
        code: Option<String>,
    },
    /// The request was rejected before it was sent.
    InvalidRequest(String),
    /// The provider does not serve this model.
    ModelNotFound {
        model: String,
    },
}

impl ProviderError {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    pub const fn kind(&self) -> ProviderErrorKind {
        match self {
            Self::Unavailable { .. } => ProviderErrorKind::Unavailable,
            Self::Unauthorized => ProviderErrorKind::Unauthorized,
            Self::Credential(_) => ProviderErrorKind::CredentialUnavailable,
            Self::RateLimited { .. } => ProviderErrorKind::RateLimited,
            Self::ContextOverflow { .. } => ProviderErrorKind::ContextOverflow,
            Self::OfflineRefused { .. } => ProviderErrorKind::OfflineRefused,
            Self::PolicyDenied { .. } => ProviderErrorKind::PolicyDenied,
            Self::Cancelled => ProviderErrorKind::Cancelled,
            Self::Timeout => ProviderErrorKind::Timeout,
            Self::Protocol(_) => ProviderErrorKind::Protocol,
            Self::Http { .. } => ProviderErrorKind::Http,
            Self::InvalidRequest(_) => ProviderErrorKind::InvalidRequest,
            Self::ModelNotFound { .. } => ProviderErrorKind::ModelNotFound,
        }
    }

    /// Cross-service category for API mapping.
    pub const fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Unavailable { .. }
            | Self::RateLimited { .. }
            | Self::Timeout
            | Self::Cancelled
            | Self::Http { .. }
            | Self::Protocol(_) => ServiceErrorCode::Unavailable,
            Self::Unauthorized
            | Self::Credential(_)
            | Self::OfflineRefused { .. }
            | Self::PolicyDenied { .. } => ServiceErrorCode::PermissionDenied,
            Self::ContextOverflow { .. } | Self::InvalidRequest(_) => {
                ServiceErrorCode::InvalidArgument
            }
            Self::ModelNotFound { .. } => ServiceErrorCode::NotFound,
        }
    }

    /// Whether the same call may succeed later (or on another provider) without
    /// any change by the user: unavailability, rate limits, timeouts, and HTTP
    /// 408/425/429/5xx. Authentication, credential, privacy, cancellation,
    /// protocol, and request errors are never retryable.
    /// ChatGPT plan-route errors also stop automatic provider/billing fallback;
    /// temporary availability may be retried explicitly later.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Unavailable { .. } | Self::RateLimited { .. } | Self::Timeout => true,
            Self::Http { status, code } => {
                // Plan failures must stop this inference attempt rather than
                // switch to another provider/billing path. A caller may retry
                // temporary plan availability explicitly at a later time.
                if let Some(code) = code
                    && chatgpt_error_status(code).is_some()
                {
                    return false;
                }
                matches!(*status, 408 | 425 | 429) || (*status >= 500 && *status <= 599)
            }
            _ => false,
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { reason } => write!(f, "provider unavailable: {reason}"),
            Self::Unauthorized => f.write_str("provider rejected the credential"),
            Self::Credential(error) => error.fmt(f),
            Self::RateLimited {
                retry_after: Some(after),
            } => write!(
                f,
                "provider rate limit reached; retry after {}s",
                after.as_secs()
            ),
            Self::RateLimited { retry_after: None } => f.write_str("provider rate limit reached"),
            Self::ContextOverflow { limit, requested } => {
                f.write_str("request exceeds the model context window")?;
                match (requested, limit) {
                    (Some(requested), Some(limit)) => {
                        write!(f, " ({requested} tokens requested, limit {limit})")
                    }
                    (None, Some(limit)) => write!(f, " (limit {limit})"),
                    _ => Ok(()),
                }
            }
            Self::OfflineRefused { host } => write!(
                f,
                "offline mode refused remote traffic to {host}; run `aidex offline off` to allow it"
            ),
            Self::PolicyDenied { reason } => write!(f, "denied by policy: {reason}"),
            Self::Cancelled => f.write_str("request cancelled"),
            Self::Timeout => f.write_str("provider request timed out"),
            Self::Protocol(message) => write!(f, "provider protocol error: {message}"),
            Self::Http { status, code } => match code {
                Some(code) => write!(f, "provider returned HTTP {status} ({code})"),
                None => write!(f, "provider returned HTTP {status}"),
            },
            Self::InvalidRequest(message) => write!(f, "invalid chat request: {message}"),
            Self::ModelNotFound { model } => {
                write!(f, "provider does not serve model `{model}`")
            }
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<CredentialError> for ProviderError {
    fn from(error: CredentialError) -> Self {
        Self::Credential(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_transient_failures_are_retryable() {
        for retryable in [
            ProviderError::unavailable("down"),
            ProviderError::RateLimited { retry_after: None },
            ProviderError::Timeout,
            ProviderError::Http {
                status: 502,
                code: None,
            },
            ProviderError::Http {
                status: 429,
                code: None,
            },
        ] {
            assert!(retryable.is_retryable(), "{retryable}");
        }
        for terminal in [
            ProviderError::Unauthorized,
            ProviderError::Credential(CredentialError::Locked),
            ProviderError::OfflineRefused { host: "h".into() },
            ProviderError::Cancelled,
            ProviderError::Protocol("x".into()),
            ProviderError::Http {
                status: 400,
                code: None,
            },
            ProviderError::ContextOverflow {
                limit: Some(1),
                requested: Some(2),
            },
            ProviderError::InvalidRequest("x".into()),
        ] {
            assert!(!terminal.is_retryable(), "{terminal}");
        }
    }

    #[test]
    fn kinds_are_stable_snake_case() {
        assert_eq!(
            ProviderError::OfflineRefused { host: "h".into() }
                .kind()
                .as_str(),
            "offline_refused"
        );
        assert_eq!(
            serde_json::to_value(ProviderErrorKind::CredentialUnavailable).unwrap(),
            "credential_unavailable"
        );
        assert_eq!(
            ProviderError::Credential(CredentialError::Locked).code(),
            ServiceErrorCode::PermissionDenied
        );
    }
}
