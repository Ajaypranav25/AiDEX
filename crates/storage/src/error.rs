use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum StoreError {
    InvalidKey,
    Corrupt(String),
    Migration(String),
    LowSpace(String),
    ReadOnly(String),
    DriveUnavailable(String),
    Io(io::Error),
    Database(rusqlite::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey => f.write_str("storage key must be a non-empty safe relative path"),
            Self::Corrupt(message) => write!(f, "storage corruption detected: {message}"),
            Self::Migration(message) => write!(f, "storage migration failed: {message}"),
            Self::LowSpace(message) => {
                write!(
                    f,
                    "storage write failed because the drive is full: {message}"
                )
            }
            Self::ReadOnly(message) => write!(f, "storage drive is read-only: {message}"),
            Self::DriveUnavailable(message) => {
                write!(f, "storage drive is unavailable: {message}")
            }
            Self::Io(error) => write!(f, "storage I/O error: {error}"),
            Self::Database(error) => write!(f, "storage database error: {error}"),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        let message = error.to_string();
        match error.kind() {
            io::ErrorKind::StorageFull => Self::LowSpace(message),
            io::ErrorKind::ReadOnlyFilesystem => Self::ReadOnly(message),
            io::ErrorKind::NotFound => Self::DriveUnavailable(message),
            _ => Self::Io(error),
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(code, _) = &error {
            let message = error.to_string();
            match code.code {
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => {
                    return Self::Corrupt(message);
                }
                rusqlite::ErrorCode::DiskFull => return Self::LowSpace(message),
                rusqlite::ErrorCode::ReadOnly | rusqlite::ErrorCode::PermissionDenied => {
                    return Self::ReadOnly(message);
                }
                rusqlite::ErrorCode::CannotOpen | rusqlite::ErrorCode::NotFound => {
                    return Self::DriveUnavailable(message);
                }
                _ => {}
            }
        }
        Self::Database(error)
    }
}
