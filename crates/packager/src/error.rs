//! Packager failures.

use std::{fmt, io};

use aidex_bundle::BundleError;
use aidex_drive::DriveError;

/// A failed packager command; printed as `aidex-packager: <message>` (exit 1).
#[derive(Debug)]
pub struct PackagerError {
    pub message: String,
}

impl PackagerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn io(context: impl fmt::Display, error: io::Error) -> Self {
        Self::new(format!("{context}: {error}"))
    }
}

impl fmt::Display for PackagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PackagerError {}

impl From<BundleError> for PackagerError {
    fn from(error: BundleError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<DriveError> for PackagerError {
    fn from(error: DriveError) -> Self {
        Self::new(error.to_string())
    }
}
