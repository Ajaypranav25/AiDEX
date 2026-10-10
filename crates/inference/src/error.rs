use aidex_core::ServiceErrorCode;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InferenceError {
    Invalid(String),
    NotFound(String),
    Conflict(String),
    Integrity(String),
    Io(String),
    Unavailable(String),
    Cancelled,
}
impl InferenceError {
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Invalid(_) => ServiceErrorCode::InvalidArgument,
            Self::NotFound(_) => ServiceErrorCode::NotFound,
            Self::Conflict(_) => ServiceErrorCode::Conflict,
            Self::Integrity(_) => ServiceErrorCode::PermissionDenied,
            Self::Io(_) | Self::Unavailable(_) | Self::Cancelled => ServiceErrorCode::Unavailable,
        }
    }
}
impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s)
            | Self::NotFound(s)
            | Self::Conflict(s)
            | Self::Integrity(s)
            | Self::Io(s)
            | Self::Unavailable(s) => f.write_str(s),
            Self::Cancelled => f.write_str("inference operation cancelled"),
        }
    }
}
impl std::error::Error for InferenceError {}
impl From<std::io::Error> for InferenceError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
impl From<serde_json::Error> for InferenceError {
    fn from(e: serde_json::Error) -> Self {
        Self::Integrity(e.to_string())
    }
}
