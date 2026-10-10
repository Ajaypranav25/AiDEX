//! `SetupError`: the error shape every setup command returns to the UI. It matches the
//! desktop's `BridgeError` (`code`, `kind`, `message`) so `@aidex/client` decodes both.

use aidex_core::ServiceErrorCode;
use aidex_drive::DriveError;
use serde::Serialize;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SetupError {
    /// Stable cross-service category (`aidex_core::ServiceErrorCode`, snake_case).
    pub code: &'static str,
    /// Finer stable identifier (`not_removable`, `missing_manifest`, `system_volume`, ...).
    pub kind: &'static str,
    pub message: String,
}

impl SetupError {
    pub fn new(code: ServiceErrorCode, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            code: service_code_name(code),
            kind,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::Internal, "internal", message)
    }
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): {}", self.kind, self.code, self.message)
    }
}

impl std::error::Error for SetupError {}

impl From<DriveError> for SetupError {
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

/// The snake_case wire name of a service category.
fn service_code_name(code: ServiceErrorCode) -> &'static str {
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

    #[test]
    fn drive_errors_keep_their_service_code_and_a_stable_kind() {
        let error: SetupError = DriveError::LowSpace {
            available: 1,
            required: 2,
        }
        .into();
        assert_eq!(error.code, "unavailable");
        assert_eq!(error.kind, "low_space");
        let value = serde_json::to_value(SetupError::from(DriveError::NotRemovable("C:\\".into())))
            .unwrap();
        assert_eq!(value["code"], "invalid_argument");
        assert_eq!(value["kind"], "not_removable");
        assert!(value["message"].as_str().unwrap().contains("removable"));
    }
}
