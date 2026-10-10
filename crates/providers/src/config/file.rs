//! Bounded reads and crash-safe atomic writes of small TOML files.

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use aidex_core::ServiceErrorCode;
use serde::{Serialize, de::DeserializeOwned};

/// Largest configuration file accepted.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigError {
    Io {
        path: PathBuf,
        message: String,
    },
    /// The file is not valid for its closed schema. The message never quotes
    /// values from the file.
    Invalid {
        path: PathBuf,
        message: String,
    },
    UnsupportedVersion {
        path: PathBuf,
        found: u32,
        supported: u32,
    },
    /// A managed path is a symbolic link or not a regular file.
    UnsafePath(PathBuf),
}

impl ConfigError {
    pub const fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Io { .. } => ServiceErrorCode::Internal,
            Self::Invalid { .. } | Self::UnsafePath(_) => ServiceErrorCode::InvalidArgument,
            Self::UnsupportedVersion { .. } => ServiceErrorCode::IncompatibleVersion,
        }
    }

    fn io(path: &Path, error: &io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "{}: {message}", path.display()),
            Self::Invalid { path, message } => {
                write!(f, "{} is invalid: {message}", path.display())
            }
            Self::UnsupportedVersion {
                path,
                found,
                supported,
            } => write!(
                f,
                "{} has version {found}; this build supports version {supported}",
                path.display()
            ),
            Self::UnsafePath(path) => write!(
                f,
                "{} must be a regular file, not a link or directory",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Read and parse `path`; `Ok(None)` when it does not exist.
pub(crate) fn read_toml<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, ConfigError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(ConfigError::UnsafePath(path.to_path_buf()));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ConfigError::io(path, &error)),
    }
    let mut text = String::new();
    File::open(path)
        .and_then(|file| file.take(MAX_CONFIG_BYTES + 1).read_to_string(&mut text))
        .map_err(|error| ConfigError::io(path, &error))?;
    if text.len() as u64 > MAX_CONFIG_BYTES {
        return Err(ConfigError::Invalid {
            path: path.to_path_buf(),
            message: "file is larger than 1 MiB".into(),
        });
    }
    toml::from_str(&text)
        .map(Some)
        .map_err(|error| ConfigError::Invalid {
            path: path.to_path_buf(),
            message: describe_toml_error(&error, &text),
        })
}

/// `toml`'s `Display` quotes the offending source line, which could echo a
/// credential pasted into the file by mistake; report the position and the
/// message with quoted literals removed instead.
fn describe_toml_error(error: &toml::de::Error, text: &str) -> String {
    let message = redact_quoted(error.message());
    match error.span() {
        Some(span) => {
            let line = text[..span.start.min(text.len())].matches('\n').count() + 1;
            format!("line {line}: {message}")
        }
        None => message,
    }
}

fn redact_quoted(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut in_quote = false;
    for ch in message.chars() {
        if ch == '"' {
            if in_quote {
                out.push_str("\"…\"");
            }
            in_quote = !in_quote;
        } else if !in_quote {
            out.push(ch);
        }
    }
    out.trim_end().to_owned()
}

/// Serialize `value` and atomically replace `path` (temp file, fsync, rename,
/// and a directory fsync on Unix). The parent directory must already exist.
pub(crate) fn write_toml<T: Serialize>(path: &Path, value: &T) -> Result<(), ConfigError> {
    let text = toml::to_string(value).map_err(|error| ConfigError::Invalid {
        path: path.to_path_buf(),
        message: format!("cannot encode configuration: {error}"),
    })?;
    if let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.file_type().is_file()
    {
        return Err(ConfigError::UnsafePath(path.to_path_buf()));
    }
    let parent = path
        .parent()
        .ok_or_else(|| ConfigError::UnsafePath(path.to_path_buf()))?;
    let mut suffix = [0_u8; 8];
    getrandom::fill(&mut suffix).map_err(|error| ConfigError::Io {
        path: path.to_path_buf(),
        message: format!("randomness unavailable: {error}"),
    })?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = parent.join(format!(
        ".{name}.{}.tmp",
        suffix
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        sync_dir(parent)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(ConfigError::io(path, &error));
    }
    Ok(())
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_files_report_position_without_quoting_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.toml");
        fs::write(&path, "version = 1\nname = \"sk-pasted-secret\"\n").unwrap();
        #[derive(serde::Deserialize, Debug)]
        #[serde(deny_unknown_fields)]
        #[allow(dead_code)]
        struct Strict {
            version: u32,
            name: u32,
        }
        let error = read_toml::<Strict>(&path).unwrap_err().to_string();
        assert!(error.contains("line 2"), "{error}");
        assert!(!error.contains("sk-pasted-secret"), "{error}");
    }

    #[test]
    fn atomic_writes_replace_and_leave_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.toml");
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Value {
            version: u32,
        }
        write_toml(&path, &Value { version: 1 }).unwrap();
        write_toml(&path, &Value { version: 2 }).unwrap();
        assert_eq!(
            read_toml::<Value>(&path).unwrap(),
            Some(Value { version: 2 })
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(
            read_toml::<Value>(&dir.path().join("missing.toml")).unwrap(),
            None
        );
    }
}
