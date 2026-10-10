//! Portable-drive path validation, the schema-v1 manifest, and safe drive operations.
//!
//! The root-level `aidex-drive.json` file is the sole drive manifest. Initialization
//! creates only manifest-declared directories; status never repairs missing, malformed,
//! or unsupported manifest state. Relative paths are validated before filesystem use.

use std::{
    error::Error,
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RelativePath(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathError {
    Empty,
    Absolute,
    ParentTraversal,
    InvalidComponent,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "path must not be empty",
            Self::Absolute => "path must be relative",
            Self::ParentTraversal => "parent traversal is not allowed",
            Self::InvalidComponent => "path contains an unsupported component",
        })
    }
}

impl Error for PathError {}

impl RelativePath {
    /// Parse slash-separated path segments, rejecting traversal and platform roots.
    pub fn new(path: impl AsRef<str>) -> Result<Self, PathError> {
        let path = path.as_ref();
        if path.is_empty() {
            return Err(PathError::Empty);
        }
        let parsed = Path::new(path);
        if parsed.is_absolute() || path.starts_with('/') || path.starts_with('\\') {
            return Err(PathError::Absolute);
        }
        let mut parts = Vec::new();
        for part in path.split(['/', '\\']) {
            match part {
                "" | "." => continue,
                ".." => return Err(PathError::ParentTraversal),
                segment if segment.contains(':') => return Err(PathError::InvalidComponent),
                segment => parts.push(segment),
            }
        }
        if parts.is_empty() {
            return Err(PathError::Empty);
        }
        Ok(Self(parts.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Resolve beneath a root after lexical validation; does not follow symlinks.
    pub fn resolve_under(&self, root: impl AsRef<Path>) -> PathBuf {
        let mut result = root.as_ref().to_path_buf();
        for component in Path::new(&self.0).components() {
            if let Component::Normal(segment) = component {
                result.push(segment);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{PathError, RelativePath};
    use std::path::Path;

    #[test]
    fn normalizes_separators_and_resolves_beneath_root() {
        let path = RelativePath::new(r"projects\alpha/./src").unwrap();

        assert_eq!(path.as_str(), "projects/alpha/src");
        assert_eq!(
            path.resolve_under("drive"),
            Path::new("drive")
                .join("projects")
                .join("alpha")
                .join("src")
        );
    }

    #[test]
    fn rejects_absolute_and_parent_traversal_paths() {
        assert_eq!(RelativePath::new("/etc/passwd"), Err(PathError::Absolute));
        assert!(RelativePath::new(r"C:\secrets").is_err());
        assert_eq!(
            RelativePath::new("workspace/../secrets"),
            Err(PathError::ParentTraversal)
        );
    }
}

/// Root-level file used to identify an AiDEX drive.
pub const MANIFEST_FILE_NAME: &str = "aidex-drive.json";

/// The manifest schema supported by this version of the drive crate.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// A validated, portable description of the directories owned by a drive.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DriveManifest {
    schema_version: u32,
    pub layout: DriveLayout,
}

/// Public runtime/model directories and private encrypted data directories.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DriveLayout {
    pub public_runtime: RelativePath,
    pub public_models: RelativePath,
    pub private_vault: RelativePath,
    pub private_identity: RelativePath,
    pub private_sessions: RelativePath,
    pub private_config: RelativePath,
}

impl Default for DriveLayout {
    fn default() -> Self {
        Self {
            public_runtime: RelativePath("runtime".into()),
            public_models: RelativePath("models".into()),
            private_vault: RelativePath("private/vault".into()),
            private_identity: RelativePath("private/identity".into()),
            private_sessions: RelativePath("private/sessions".into()),
            private_config: RelativePath("private/config".into()),
        }
    }
}

/// Stable errors returned while decoding a drive manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    MalformedJson,
    InvalidManifest,
    UnsupportedVersion(u32),
    InvalidPath(PathError),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedJson => f.write_str("manifest is not valid JSON"),
            Self::InvalidManifest => f.write_str("manifest does not match the supported schema"),
            Self::UnsupportedVersion(version) => {
                write!(f, "manifest schema version {version} is not supported")
            }
            Self::InvalidPath(error) => write!(f, "manifest path is invalid: {error}"),
        }
    }
}

impl Error for ManifestError {}

impl DriveManifest {
    pub fn new(layout: DriveLayout) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            layout,
        }
    }

    pub fn current() -> Self {
        Self::new(DriveLayout::default())
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn to_json(&self) -> Result<String, ManifestError> {
        serde_json::to_string(self).map_err(|_| ManifestError::InvalidManifest)
    }

    pub fn from_json(json: &str) -> Result<Self, ManifestError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawManifest {
            schema_version: u32,
            layout: serde_json::Value,
        }

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawLayout {
            public_runtime: String,
            public_models: String,
            private_vault: String,
            private_identity: String,
            private_sessions: String,
            private_config: String,
        }

        let raw: RawManifest =
            serde_json::from_str(json).map_err(|_| ManifestError::MalformedJson)?;
        if raw.schema_version > CURRENT_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedVersion(raw.schema_version));
        }
        if raw.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ManifestError::InvalidManifest);
        }
        let layout: RawLayout =
            serde_json::from_value(raw.layout).map_err(|_| ManifestError::InvalidManifest)?;

        let path = |value: String| RelativePath::new(value).map_err(ManifestError::InvalidPath);
        Ok(Self::new(DriveLayout {
            public_runtime: path(layout.public_runtime)?,
            public_models: path(layout.public_models)?,
            private_vault: path(layout.private_vault)?,
            private_identity: path(layout.private_identity)?,
            private_sessions: path(layout.private_sessions)?,
            private_config: path(layout.private_config)?,
        }))
    }
}

impl Serialize for RelativePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = String::deserialize(deserializer)?;
        Self::new(path).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod manifest_tests;

const MIN_FREE_BYTES: u64 = 10 * 1024 * 1024;

/// Filesystem health for a resolved AiDEX drive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriveStatus {
    /// Canonical drive root.
    pub root: PathBuf,
    /// Whether a create-new write probe succeeded and was removed.
    pub writable: bool,
    /// Available bytes when reported by the platform.
    pub free_bytes: Option<u64>,
    /// Manifest schema version; status only succeeds when it is supported.
    pub schema_version: Option<u32>,
}

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

/// Resolve an explicit drive root or `AIDEX_DRIVE_ROOT` from a portable launcher.
pub fn resolve_root(explicit: Option<&Path>) -> Result<PathBuf, DriveError> {
    let root = match explicit {
        Some(path) => path.to_path_buf(),
        None => std::env::var_os("AIDEX_DRIVE_ROOT")
            .map(PathBuf::from)
            .ok_or_else(|| {
                DriveError::Discovery(
                    "no active drive; pass `--drive PATH` or set AIDEX_DRIVE_ROOT in the portable launcher"
                        .into(),
                )
            })?,
    };
    if is_windows_drive_root(&root) && !cfg!(windows) {
        return Err(DriveError::InvalidRoot(format!(
            "Windows drive path `{}` cannot be accessed on this host",
            root.display()
        )));
    }
    let metadata = fs::metadata(&root)
        .map_err(|error| DriveError::InvalidRoot(format!("{}: {error}", root.display())))?;
    if !metadata.is_dir() {
        return Err(DriveError::InvalidRoot(format!(
            "{} is not a directory",
            root.display()
        )));
    }
    fs::canonicalize(&root)
        .map_err(|error| DriveError::InvalidRoot(format!("{}: {error}", root.display())))
}

/// Initialize manifest-declared directories without replacing existing content.
/// Safe to retry after interruption; existing invalid manifests are never changed.
pub fn init(root: impl AsRef<Path>) -> Result<DriveStatus, DriveError> {
    let root = resolve_root(Some(root.as_ref()))?;
    require_removable(&root)?;
    let free = available_bytes(&root)?;
    if free < MIN_FREE_BYTES {
        return Err(DriveError::LowSpace {
            available: free,
            required: MIN_FREE_BYTES,
        });
    }
    initialize_at(&root)?;
    status_at(&root, true, Some(free))
}

/// Inspect drive health without creating directories or repairing a manifest.
pub fn status(root: impl AsRef<Path>) -> Result<DriveStatus, DriveError> {
    let root = resolve_root(Some(root.as_ref()))?;
    require_removable(&root)?;
    let writable = writable_probe(&root).is_ok();
    let free_bytes = available_bytes(&root).ok();
    status_at(&root, writable, free_bytes)
}

fn initialize_at(root: &Path) -> Result<(), DriveError> {
    let manifest_path = root.join(MANIFEST_FILE_NAME);
    let manifest = match read_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(DriveError::MissingManifest(_)) => DriveManifest::current(),
        Err(error) => return Err(error),
    };
    writable_probe(root)?;

    for relative in [
        &manifest.layout.public_runtime,
        &manifest.layout.public_models,
        &manifest.layout.private_vault,
        &manifest.layout.private_identity,
        &manifest.layout.private_sessions,
        &manifest.layout.private_config,
    ] {
        create_owned_dir(root, relative)?;
    }

    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&manifest_path)
    {
        Ok(mut file) => {
            let json = manifest
                .to_json()
                .map_err(|error| DriveError::Io(format!("cannot serialize manifest: {error}")))?;
            file.write_all(json.as_bytes())
                .and_then(|_| file.sync_all())
                .map_err(|error| {
                    DriveError::Io(format!(
                        "cannot finish manifest `{}`: {error}",
                        manifest_path.display()
                    ))
                })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_manifest(&manifest_path)?;
        }
        Err(error) => {
            return Err(DriveError::Io(format!(
                "cannot create manifest `{}`: {error}",
                manifest_path.display()
            )));
        }
    }
    Ok(())
}

fn create_owned_dir(root: &Path, relative: &RelativePath) -> Result<(), DriveError> {
    let mut current = root.to_path_buf();
    for component in Path::new(relative.as_str()).components() {
        let Component::Normal(segment) = component else {
            return Err(DriveError::InvalidRoot(format!(
                "invalid manifest directory `{}`",
                relative.as_str()
            )));
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(DriveError::Io(format!(
                    "refusing to use non-directory or symbolic link `{}`",
                    current.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|error| {
                    DriveError::Io(format!(
                        "cannot create AiDEX directory `{}`: {error}",
                        current.display()
                    ))
                })?;
            }
            Err(error) => {
                return Err(DriveError::Io(format!(
                    "cannot inspect AiDEX directory `{}`: {error}",
                    current.display()
                )));
            }
        }
    }
    Ok(())
}

fn status_at(
    root: &Path,
    writable: bool,
    free_bytes: Option<u64>,
) -> Result<DriveStatus, DriveError> {
    let manifest = read_manifest(&root.join(MANIFEST_FILE_NAME))?;
    Ok(DriveStatus {
        root: root.to_path_buf(),
        writable,
        free_bytes,
        schema_version: Some(manifest.schema_version()),
    })
}

fn read_manifest(path: &Path) -> Result<DriveManifest, DriveError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(DriveError::MissingManifest(path.to_path_buf()));
        }
        Err(error) => {
            return Err(DriveError::CorruptManifest(
                path.to_path_buf(),
                error.to_string(),
            ));
        }
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(DriveError::CorruptManifest(
            path.to_path_buf(),
            "manifest is not a regular file".into(),
        ));
    }
    let content = fs::read_to_string(path)
        .map_err(|error| DriveError::CorruptManifest(path.to_path_buf(), error.to_string()))?;
    DriveManifest::from_json(&content)
        .map_err(|error| DriveError::CorruptManifest(path.to_path_buf(), error.to_string()))
}

fn writable_probe(root: &Path) -> Result<(), DriveError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = root.join(format!(".aidex-write-probe-{}-{stamp}", std::process::id()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut file| file.write_all(b"aidex"))
        .map_err(|error| DriveError::ReadOnly(format!("{}: {error}", root.display())))?;
    fs::remove_file(&path).map_err(|error| {
        DriveError::ReadOnly(format!("cannot remove temporary write probe: {error}"))
    })
}

fn available_bytes(root: &Path) -> Result<u64, DriveError> {
    #[cfg(windows)]
    let output = {
        let letter = windows_drive_letter(root).unwrap_or_default().to_string();
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-Volume -DriveLetter $env:AIDEX_DRIVE_LETTER -ErrorAction Stop).SizeRemaining",
            ])
            .env("AIDEX_DRIVE_LETTER", letter)
            .output()
    };
    #[cfg(not(windows))]
    let output = Command::new("df").args(["-Pk"]).arg(root).output();
    let output = output
        .map_err(|error| DriveError::Discovery(format!("cannot query free space: {error}")))?;
    if !output.status.success() {
        return Err(DriveError::Discovery(
            "cannot determine free drive space".into(),
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    #[cfg(windows)]
    let bytes = text.trim().parse::<u64>().ok();
    #[cfg(not(windows))]
    let bytes = text
        .lines()
        .last()
        .and_then(|line| line.split_whitespace().nth(3))
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024));
    bytes.ok_or_else(|| DriveError::Discovery("cannot parse free drive space".into()))
}

fn require_removable(root: &Path) -> Result<(), DriveError> {
    #[cfg(target_os = "linux")]
    {
        let canonical =
            fs::canonicalize(root).map_err(|error| DriveError::Discovery(error.to_string()))?;
        let mounts = fs::read_to_string("/proc/mounts").map_err(|error| {
            DriveError::Discovery(format!("cannot identify removable filesystem: {error}"))
        })?;
        let mount = mounts
            .lines()
            .filter_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                let point = fields.get(1)?.replace("\\040", " ");
                let point_path = PathBuf::from(point);
                canonical
                    .starts_with(&point_path)
                    .then(|| (fields[0], point_path))
            })
            .max_by_key(|(_, point)| point.components().count());
        let (device, _) =
            mount.ok_or_else(|| DriveError::NotRemovable(root.display().to_string()))?;
        let block = device
            .strip_prefix("/dev/")
            .ok_or_else(|| DriveError::NotRemovable(root.display().to_string()))?;
        let block = block.trim_end_matches(|character: char| character.is_ascii_digit());
        if fs::read_to_string(PathBuf::from("/sys/block").join(block).join("removable"))
            .ok()
            .is_some_and(|value| value.trim() == "1")
        {
            return Ok(());
        }
        Err(DriveError::NotRemovable(root.display().to_string()))
    }
    #[cfg(windows)]
    {
        if !is_windows_drive_root(root) {
            return Err(DriveError::InvalidRoot(format!(
                "{} is not a drive-letter root",
                root.display()
            )));
        }
        let letter = windows_drive_letter(root).unwrap_or_default().to_string();
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance Win32_LogicalDisk -Filter (\"DeviceID='\" + $env:AIDEX_DRIVE_LETTER + \":'\")).DriveType",
            ])
            .env("AIDEX_DRIVE_LETTER", letter)
            .output()
            .map_err(|error| {
                DriveError::Discovery(format!("cannot identify removable drive: {error}"))
            })?;
        if output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "2" {
            return Ok(());
        }
        Err(DriveError::NotRemovable(root.display().to_string()))
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("diskutil")
            .args(["info", &root.to_string_lossy()])
            .output()
            .map_err(|error| {
                DriveError::Discovery(format!("cannot identify removable drive: {error}"))
            })?;
        let info = String::from_utf8_lossy(&output.stdout);
        if output.status.success()
            && info
                .lines()
                .any(|line| line.contains("Removable Media:") && line.contains("Removable"))
        {
            return Ok(());
        }
        Err(DriveError::NotRemovable(root.display().to_string()))
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = root;
        Err(DriveError::Discovery(
            "removable-drive detection is unsupported on this platform".into(),
        ))
    }
}

/// Recognize `X:\\` or `X:/` roots, including canonical Windows extended paths.
pub fn is_windows_drive_root(path: &Path) -> bool {
    windows_drive_letter(path).is_some()
}

fn windows_drive_letter(path: &Path) -> Option<char> {
    let value = path.to_string_lossy();
    let value = value.strip_prefix(r"\\?\").unwrap_or(&value);
    let bytes = value.as_bytes();
    (bytes.len() == 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
    .then(|| value.chars().next().expect("three-byte drive root"))
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod ops_tests;
