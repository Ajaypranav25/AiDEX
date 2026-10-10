//! A small durable file-backed key-value store with validated logical keys.

use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum StoreError {
    InvalidKey,
    Io(io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey => f.write_str("storage key must be a non-empty safe relative path"),
            Self::Io(error) => write!(f, "storage I/O error: {error}"),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidKey => None,
            Self::Io(error) => Some(error),
        }
    }
}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Stores values as files beneath a caller-selected directory.
#[derive(Clone, Debug)]
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let path = self.path_for(key)?;
        match fs::read(path) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    /// Write or replace a value. The store does not promise crash-atomic writes.
    pub fn put(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        let path = self.path_for(key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, value)?;
        Ok(())
    }

    /// Remove a value, returning whether a file existed.
    pub fn delete(&self, key: &str) -> Result<bool, StoreError> {
        match fs::remove_file(self.path_for(key)?) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// List keys represented by regular files, recursively, in sorted order.
    pub fn keys(&self) -> Result<Vec<String>, StoreError> {
        let mut keys = Vec::new();
        collect_files(&self.root, &self.root, &mut keys)?;
        keys.sort();
        Ok(keys)
    }

    fn path_for(&self, key: &str) -> Result<PathBuf, StoreError> {
        if key.is_empty() || key.starts_with('/') || key.starts_with('\\') || key.contains(':') {
            return Err(StoreError::InvalidKey);
        }
        let mut path = self.root.clone();
        for segment in key.split(['/', '\\']) {
            if segment.is_empty() || segment == "." || segment == ".." {
                return Err(StoreError::InvalidKey);
            }
            path.push(segment);
        }
        Ok(path)
    }
}

fn collect_files(root: &Path, directory: &Path, keys: &mut Vec<String>) -> Result<(), StoreError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_files(root, &entry.path(), keys)?;
        } else if kind.is_file() {
            let entry_path = entry.path();
            let relative = entry_path
                .strip_prefix(root)
                .map_err(|_| StoreError::InvalidKey)?;
            let key = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            keys.push(key);
        }
    }
    Ok(())
}
