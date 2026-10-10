//! The drive-rooted daemon endpoint file and its bearer capability.
//!
//! `<drive>/.aidex/runtime/daemon.json` names the loopback port and carries a
//! 256-bit OS-random capability. On Windows the capability is protected at rest
//! with the current user's DPAPI credential; on Unix the file is created 0600.
//! The file is created with `create_new`, which is the single-instance lock.

use crate::rpc::CONTRACT_VERSION;
use aidex_core::ServiceErrorCode;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

const STATE_DIR: &str = ".aidex/runtime";
const STATE_FILE: &str = "daemon.json";
const MAX_ENDPOINT_BYTES: usize = 4096;
const CAPABILITY_BYTES: usize = 32;

/// Directory holding daemon runtime metadata on the drive.
pub fn state_dir(root: &Path) -> PathBuf {
    root.join(STATE_DIR)
}

/// Path of the endpoint file for a drive root.
pub fn state_path(root: &Path) -> PathBuf {
    state_dir(root).join(STATE_FILE)
}

/// Create `.aidex/runtime`, refusing symlinked (or junctioned) components.
pub fn ensure_state_dir(root: &Path) -> Result<PathBuf, EndpointError> {
    let metadata_dir = root.join(".aidex");
    let runtime = state_dir(root);
    for path in [&metadata_dir, &runtime] {
        refuse_symlink(path)?;
        fs::create_dir_all(path).map_err(|source| EndpointError::Io {
            action: "create daemon state directory",
            source,
        })?;
    }
    Ok(runtime)
}

/// Fail if `path` exists as a symbolic link.
pub fn refuse_symlink(path: &Path) -> Result<(), EndpointError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(EndpointError::Invalid(format!(
            "managed daemon path is a symlink: {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(EndpointError::Io {
            action: "inspect managed daemon path",
            source,
        }),
    }
}

/// A 256-bit bearer capability, hex encoded. `Debug` never shows the value.
#[derive(Clone, PartialEq, Eq)]
pub struct Capability(String);

impl Capability {
    /// Draw a fresh capability from the operating-system random source.
    pub fn generate() -> Result<Self, EndpointError> {
        let mut bytes = [0u8; CAPABILITY_BYTES];
        getrandom::fill(&mut bytes).map_err(|error| EndpointError::Random(error.to_string()))?;
        Ok(Self(
            bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        ))
    }

    /// Accept only a 64-character lowercase or uppercase hex string.
    pub fn parse(hex: impl Into<String>) -> Result<Self, EndpointError> {
        let hex = hex.into();
        if hex.len() != CAPABILITY_BYTES * 2 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(EndpointError::Invalid(
                "daemon endpoint credential is malformed".into(),
            ));
        }
        Ok(Self(hex.to_ascii_lowercase()))
    }

    /// The secret value. Send it only in the `aidex.handshake` request.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Constant-time comparison with a presented capability.
    pub fn matches(&self, presented: &str) -> bool {
        let expected = self.0.as_bytes();
        let presented = presented.as_bytes();
        let mut difference = expected.len() ^ presented.len();
        for (index, byte) in expected.iter().enumerate() {
            let other = presented.get(index).copied().unwrap_or(0);
            difference |= usize::from(byte ^ other);
        }
        std::hint::black_box(difference) == 0
    }
}

impl fmt::Debug for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Capability(<redacted>)")
    }
}

/// A decoded, version-2 endpoint with its unprotected capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub version: u32,
    pub pid: u32,
    pub port: u16,
    pub token: Capability,
}

/// Endpoint file fields as stored; `token` is protected on Windows.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u32,
    pid: u32,
    port: u16,
    token: String,
}

/// Fields that every endpoint version shares, used to probe old files.
#[derive(Deserialize)]
struct Probe {
    version: u32,
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    port: Option<u16>,
}

/// Version-independent view used by stale-endpoint recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointSummary {
    pub version: u32,
    pub pid: Option<u32>,
    pub port: Option<u16>,
}

#[derive(Debug)]
pub enum EndpointError {
    /// No endpoint file: the daemon is not running.
    Missing(PathBuf),
    /// The file was written by a daemon speaking another contract version.
    Incompatible {
        found: u32,
    },
    /// The file is unreadable as an endpoint.
    Invalid(String),
    /// The capability could not be protected or unprotected.
    Protection(String),
    /// Another daemon (live or stale) already published an endpoint.
    Exists(PathBuf),
    Io {
        action: &'static str,
        source: io::Error,
    },
    Random(String),
}

impl EndpointError {
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Missing(_) | Self::Io { .. } => ServiceErrorCode::Unavailable,
            Self::Incompatible { .. } => ServiceErrorCode::IncompatibleVersion,
            Self::Invalid(_) => ServiceErrorCode::InvalidArgument,
            Self::Protection(_) => ServiceErrorCode::PermissionDenied,
            Self::Exists(_) => ServiceErrorCode::Conflict,
            Self::Random(_) => ServiceErrorCode::Internal,
        }
    }
}

impl fmt::Display for EndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(path) => write!(f, "no daemon endpoint at {}", path.display()),
            Self::Incompatible { found } => write!(
                f,
                "daemon endpoint uses IPC contract version {found}; this build requires version {CONTRACT_VERSION}"
            ),
            Self::Invalid(message) => write!(f, "invalid daemon endpoint: {message}"),
            Self::Protection(message) => f.write_str(message),
            Self::Exists(path) => write!(
                f,
                "daemon endpoint already exists at {}; run `aidex daemon start` to recover a stale endpoint",
                path.display()
            ),
            Self::Io { action, source } => write!(f, "cannot {action}: {source}"),
            Self::Random(message) => write!(f, "operating-system random source failed: {message}"),
        }
    }
}

impl std::error::Error for EndpointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn read_bytes(root: &Path) -> Result<Option<Vec<u8>>, EndpointError> {
    let path = state_path(root);
    match fs::read(&path) {
        Ok(bytes) if bytes.len() > MAX_ENDPOINT_BYTES => {
            Err(EndpointError::Invalid("endpoint file is oversized".into()))
        }
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(EndpointError::Io {
            action: "read daemon endpoint",
            source,
        }),
    }
}

/// Read only the version-independent fields, without touching the capability.
pub fn summarize(root: &Path) -> Result<Option<EndpointSummary>, EndpointError> {
    let Some(bytes) = read_bytes(root)? else {
        return Ok(None);
    };
    let probe: Probe = serde_json::from_slice(&bytes)
        .map_err(|error| EndpointError::Invalid(error.to_string()))?;
    Ok(Some(EndpointSummary {
        version: probe.version,
        pid: probe.pid,
        port: probe.port,
    }))
}

/// Read and unprotect a version-2 endpoint. Other versions are `Incompatible`.
pub fn read(root: &Path) -> Result<Endpoint, EndpointError> {
    let Some(bytes) = read_bytes(root)? else {
        return Err(EndpointError::Missing(state_path(root)));
    };
    let probe: Probe = serde_json::from_slice(&bytes)
        .map_err(|error| EndpointError::Invalid(error.to_string()))?;
    if probe.version != CONTRACT_VERSION {
        return Err(EndpointError::Incompatible {
            found: probe.version,
        });
    }
    let stored: Stored = serde_json::from_slice(&bytes)
        .map_err(|error| EndpointError::Invalid(error.to_string()))?;
    if stored.port == 0 {
        return Err(EndpointError::Invalid("endpoint port is zero".into()));
    }
    let token = Capability::parse(protection::unprotect(&stored.token)?)?;
    Ok(Endpoint {
        version: stored.version,
        pid: stored.pid,
        port: stored.port,
        token,
    })
}

fn encode(endpoint: &Endpoint) -> Result<Vec<u8>, EndpointError> {
    let stored = Stored {
        version: endpoint.version,
        pid: endpoint.pid,
        port: endpoint.port,
        token: protection::protect(endpoint.token.expose())?,
    };
    serde_json::to_vec(&stored).map_err(|error| EndpointError::Invalid(error.to_string()))
}

/// Atomically claim the endpoint path (`create_new`) and persist the endpoint.
pub(crate) fn publish(root: &Path, endpoint: &Endpoint) -> Result<(), EndpointError> {
    ensure_state_dir(root)?;
    let encoded = encode(endpoint)?;
    let path = state_path(root);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|source| {
        if source.kind() == io::ErrorKind::AlreadyExists {
            EndpointError::Exists(path.clone())
        } else {
            EndpointError::Io {
                action: "create daemon endpoint",
                source,
            }
        }
    })?;
    let written = file
        .write_all(&encoded)
        .and_then(|()| file.sync_all())
        .map_err(|source| EndpointError::Io {
            action: "persist daemon endpoint",
            source,
        });
    drop(file);
    if written.is_err() {
        let _ = fs::remove_file(&path);
    }
    written
}

/// Remove the endpoint file; a missing file is not an error.
pub(crate) fn remove(root: &Path) -> Result<(), EndpointError> {
    match fs::remove_file(state_path(root)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(EndpointError::Io {
            action: "remove daemon endpoint",
            source,
        }),
    }
}

#[cfg(windows)]
mod protection {
    use super::EndpointError;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    /// Keeps PowerShell from opening a console window for a detached daemon.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn dpapi(script: &str, input: &str, action: &str) -> Result<String, EndpointError> {
        let output = Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .env("AIDEX_DAEMON_TOKEN", input)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|error| {
                EndpointError::Protection(format!(
                    "cannot {action} daemon credential with Windows DPAPI: {error}"
                ))
            })?;
        if !output.status.success() {
            return Err(EndpointError::Protection(format!(
                "cannot {action} daemon credential with Windows DPAPI: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    pub(super) fn protect(token: &str) -> Result<String, EndpointError> {
        let protected = dpapi(
            "Add-Type -AssemblyName System.Security; $plain = [System.Text.Encoding]::UTF8.GetBytes($env:AIDEX_DAEMON_TOKEN); $protected = [System.Security.Cryptography.ProtectedData]::Protect($plain, $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser); [Console]::Out.Write([Convert]::ToBase64String($protected))",
            token,
            "protect",
        )?;
        if protected.is_empty() || protected == token {
            return Err(EndpointError::Protection(
                "Windows DPAPI returned an invalid protected credential".into(),
            ));
        }
        Ok(protected)
    }

    pub(super) fn unprotect(protected: &str) -> Result<String, EndpointError> {
        dpapi(
            "Add-Type -AssemblyName System.Security; $protected = [Convert]::FromBase64String($env:AIDEX_DAEMON_TOKEN); $plain = [System.Security.Cryptography.ProtectedData]::Unprotect($protected, $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser); [Console]::Out.Write([System.Text.Encoding]::UTF8.GetString($plain))",
            protected,
            "unlock",
        )
    }
}

#[cfg(not(windows))]
mod protection {
    use super::EndpointError;

    /// Unix relies on the 0600 owner-only endpoint file.
    pub(super) fn protect(token: &str) -> Result<String, EndpointError> {
        Ok(token.to_owned())
    }

    pub(super) fn unprotect(token: &str) -> Result<String, EndpointError> {
        Ok(token.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(token: &str) -> Endpoint {
        Endpoint {
            version: CONTRACT_VERSION,
            pid: std::process::id(),
            port: 4321,
            token: Capability::parse(token).unwrap(),
        }
    }

    #[test]
    fn capability_is_random_hex_redacted_and_compared_exactly() {
        let first = Capability::generate().unwrap();
        let second = Capability::generate().unwrap();
        assert_ne!(first, second);
        assert_eq!(first.expose().len(), 64);
        assert!(first.matches(first.expose()));
        assert!(!first.matches(second.expose()));
        assert!(!first.matches(&first.expose()[..63]));
        assert!(!first.matches(""));
        assert!(!format!("{first:?}").contains(first.expose()));
        assert!(!format!("{:?}", endpoint(first.expose())).contains(first.expose()));
        assert!(Capability::parse("xyz").is_err());
    }

    #[test]
    fn endpoint_credential_is_protected_at_rest_and_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let endpoint = endpoint(&"b".repeat(64));
        publish(root.path(), &endpoint).unwrap();
        let stored = fs::read_to_string(state_path(root.path())).unwrap();
        if cfg!(windows) {
            assert!(!stored.contains(endpoint.token.expose()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(state_path(root.path()))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(read(root.path()).unwrap(), endpoint);
        assert!(matches!(
            publish(root.path(), &endpoint),
            Err(EndpointError::Exists(_))
        ));
        remove(root.path()).unwrap();
        remove(root.path()).unwrap();
        assert!(matches!(read(root.path()), Err(EndpointError::Missing(_))));
    }

    #[test]
    fn version_one_endpoints_are_reported_incompatible_but_summarized() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(state_dir(root.path())).unwrap();
        fs::write(
            state_path(root.path()),
            r#"{"version":1,"pid":9,"port":5000,"token":"opaque"}"#,
        )
        .unwrap();
        assert!(matches!(
            read(root.path()),
            Err(EndpointError::Incompatible { found: 1 })
        ));
        assert_eq!(
            summarize(root.path()).unwrap(),
            Some(EndpointSummary {
                version: 1,
                pid: Some(9),
                port: Some(5000)
            })
        );
        fs::write(state_path(root.path()), b"not json").unwrap();
        assert!(matches!(read(root.path()), Err(EndpointError::Invalid(_))));
        assert!(summarize(root.path()).is_err());
    }
}
