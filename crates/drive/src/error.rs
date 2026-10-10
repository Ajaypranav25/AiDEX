//! Errors returned by drive resolution, initialization, and health checks.

use std::{error::Error, fmt, path::PathBuf};

/// Errors from drive resolution, initialization, or health checks.
#[derive(Debug)]
pub enum DriveError {
    InvalidRoot(String),
    NotRemovable(String),
    ReadOnly(String),
    LowSpace { available: u64, required: u64 },
    MissingManifest(PathBuf),
    CorruptManifest(PathBuf, String),
    Io(String),
    Discovery(String),
}

impl fmt::Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoot(message) => write!(f, "invalid drive root: {message}"),
            Self::NotRemovable(root) => {
                write!(f, "target is not a recognized removable drive: {root}")
            }
            Self::ReadOnly(root) => write!(f, "drive is not writable: {root}"),
            Self::LowSpace {
                available,
                required,
            } => write!(
                f,
                "drive has {available} bytes free; at least {required} bytes are required"
            ),
            Self::MissingManifest(path) => write!(
                f,
                "AiDEX manifest is missing: {} (run `aidex init --drive <path>`)",
                path.display()
            ),
            Self::CorruptManifest(path, reason) => write!(
                f,
                "AiDEX manifest is corrupt or unsupported: {} ({reason})",
                path.display()
            ),
            Self::Io(message) | Self::Discovery(message) => f.write_str(message),
        }
    }
}

impl Error for DriveError {}
