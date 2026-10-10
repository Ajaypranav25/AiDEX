//! `bundle.json`: the closed-schema manifest that describes one portable bundle.
//!
//! The packager writes it; the installer, launcher, and updater verify bundle files
//! against it. Every object rejects unknown fields, and [`BundleManifest::validate`]
//! enforces the semantic rules (safe paths, digests, required binaries) on top.

use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use aidex_core::Timestamp;
use aidex_drive::{DriveLayout, RelativePath};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    BundleError, DocumentError, Target,
    digest::{is_sha256_hex, sha256_file},
    layout::{CLI_BINARY, DAEMON_BINARY, HELPER_BINARY, LAUNCHER_BINARY, bin_path},
    path::{fold, join_relative, validate_bundle_path, validate_git_commit, validate_version},
};

/// File name of the manifest at the top of a bundle directory.
pub const MANIFEST_FILE_NAME: &str = "bundle.json";
/// The manifest schema this build writes and reads.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;
/// The only accepted `product` value.
pub const PRODUCT_NAME: &str = "aidex";
/// Upper bound on `bundle.json` size; larger files are rejected unread.
pub const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
/// Drive-relative runtime-state directory used by the daemon (see `aidex_drive::DrivePaths::runtime_state`).
pub const RUNTIME_STATE_PATH: &str = ".aidex/runtime";

const MAX_TEXT_LEN: usize = 512;

/// What a bundle file is for. Critical roles are verified on every launch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    /// The launcher binary (`bin/aidex-launcher[.exe]`; installed at the drive root as `AiDEX.exe`).
    Launcher,
    /// The `aidex` CLI.
    Cli,
    /// The `aidexd` daemon.
    Daemon,
    /// The `aidex-sandbox-exec` helper (must sit next to `aidexd`).
    Helper,
    /// Optional desktop application files (`desktop/`).
    Desktop,
    /// Optional installer application files (`installer/`).
    Installer,
    /// Native runtime libraries or executables (`bin/` or `runtime/`).
    Runtime,
    /// License texts and documentation (`licenses/` or `docs/`).
    Doc,
}

impl FileRole {
    /// Verified before every launch (the launcher refuses to run otherwise).
    pub fn is_critical(self) -> bool {
        matches!(
            self,
            Self::Launcher | Self::Cli | Self::Daemon | Self::Helper | Self::Runtime
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Launcher => "launcher",
            Self::Cli => "cli",
            Self::Daemon => "daemon",
            Self::Helper => "helper",
            Self::Desktop => "desktop",
            Self::Installer => "installer",
            Self::Runtime => "runtime",
            Self::Doc => "doc",
        }
    }

    fn allowed_prefixes(self) -> &'static [&'static str] {
        match self {
            Self::Launcher | Self::Cli | Self::Daemon | Self::Helper => &["bin/"],
            Self::Runtime => &["bin/", "runtime/"],
            Self::Desktop => &["desktop/"],
            Self::Installer => &["installer/"],
            Self::Doc => &["licenses/", "docs/"],
        }
    }

    fn requires_executable(self) -> bool {
        matches!(
            self,
            Self::Launcher | Self::Cli | Self::Daemon | Self::Helper
        )
    }
}

/// One file of the bundle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleFile {
    /// Slash-separated path relative to the bundle directory.
    pub path: String,
    /// Lowercase-hex SHA-256 of the contents.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
    /// Whether the file must be executable (Unix mode 0755 when installed).
    pub executable: bool,
    pub role: FileRole,
}

impl BundleFile {
    /// Describe an existing file of a bundle directory (hash and size from disk).
    pub fn describe(
        bundle_dir: &Path,
        path: &str,
        role: FileRole,
        executable: bool,
    ) -> Result<Self, BundleError> {
        validate_bundle_path(path).map_err(BundleError::InvalidSource)?;
        let absolute = join_relative(bundle_dir, path);
        let (sha256, size) = sha256_file(&absolute).map_err(|error| {
            BundleError::io(format!("cannot read `{}`", absolute.display()), error)
        })?;
        Ok(Self {
            path: path.to_owned(),
            sha256,
            size,
            executable,
            role,
        })
    }
}

/// License metadata for one component compiled into, or shipped with, the bundle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseEntry {
    /// Crate or component name.
    pub component: String,
    pub version: String,
    /// SPDX license expression as declared by the component, or `NOASSERTION`.
    pub spdx: String,
    /// Where the component came from (`workspace`, a registry URL, or a git URL).
    pub source: String,
    /// Bundle-relative path of the collected license text, when one was found.
    pub path: Option<String>,
}

/// Drive-relative state locations the bundled binaries use (informational; services
/// resolve the real paths from the drive manifest through `aidex_drive::DrivePaths`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatePaths {
    /// The drive manifest the launcher verifies (`aidex-drive.json`).
    pub drive_manifest: String,
    /// Daemon runtime metadata (`.aidex/runtime`).
    pub runtime_state: String,
    /// Default drive layout (manifest key → default relative path) at build time.
    pub layout: BTreeMap<String, String>,
    /// Whether any file must be extracted to the host before running. Always `false`
    /// for bundles this build produces; launchers refuse bundles that set it.
    pub host_extraction: bool,
}

impl StatePaths {
    /// State paths for the drive schema this build targets.
    pub fn current() -> Self {
        Self {
            drive_manifest: aidex_drive::MANIFEST_FILE_NAME.to_owned(),
            runtime_state: RUNTIME_STATE_PATH.to_owned(),
            layout: DriveLayout::default()
                .entries()
                .map(|(key, path)| (key.to_owned(), path.as_str().to_owned()))
                .collect(),
            host_extraction: false,
        }
    }

    fn validate(&self) -> Result<(), DocumentError> {
        let relative = |what: &str, value: &str| {
            let parsed = RelativePath::new(value).map_err(|error| {
                DocumentError::Invalid(format!("state {what} `{value}`: {error}"))
            })?;
            if parsed.as_str() == value {
                Ok(())
            } else {
                Err(DocumentError::Invalid(format!(
                    "state {what} `{value}` is not normalized"
                )))
            }
        };
        relative("drive_manifest", &self.drive_manifest)?;
        relative("runtime_state", &self.runtime_state)?;
        for (key, value) in &self.layout {
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
                return Err(DocumentError::Invalid(format!(
                    "state layout key `{key}` is not a manifest field name"
                )));
            }
            relative(key, value)?;
        }
        Ok(())
    }
}

/// The parsed `bundle.json`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    /// Always [`MANIFEST_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Always [`PRODUCT_NAME`].
    pub product: String,
    /// Product version (the `aidex-cli` package version).
    pub version: String,
    /// Full commit id the bundle was built from, when built from a git checkout.
    pub git_commit: Option<String>,
    /// Whether the checkout had uncommitted changes (`None` when unknown).
    pub source_dirty: Option<bool>,
    pub target: Target,
    /// Rust target triple the binaries were compiled for.
    pub rust_target: String,
    /// `rustc --version` of the compiler used.
    pub rustc: Option<String>,
    /// Build time; the packager uses the commit time (or `SOURCE_DATE_EPOCH`) so
    /// rebuilds of one commit produce the same manifest.
    pub created_at: Timestamp,
    /// Minimum `aidex-drive.json` schema version the bundle can run on.
    pub min_drive_schema: u32,
    pub state: StatePaths,
    /// Every file in the bundle except `bundle.json`, sorted by path.
    pub files: Vec<BundleFile>,
    /// Components and their licenses, sorted by component and version.
    pub licenses: Vec<LicenseEntry>,
}

const REQUIRED_BINARIES: [(FileRole, &str); 4] = [
    (FileRole::Launcher, LAUNCHER_BINARY),
    (FileRole::Cli, CLI_BINARY),
    (FileRole::Daemon, DAEMON_BINARY),
    (FileRole::Helper, HELPER_BINARY),
];

impl BundleManifest {
    /// Parse and validate `bundle.json` bytes.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DocumentError> {
        let manifest: Self = parse_versioned(bytes, MANIFEST_SCHEMA_VERSION)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate and serialize as pretty JSON with a trailing newline. Field order is
    /// fixed and collections should be [`normalize`](Self::normalize)d first, so equal
    /// manifests serialize to identical bytes.
    pub fn to_json(&self) -> Result<Vec<u8>, DocumentError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| DocumentError::Malformed(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Sort files by path and licenses by component, version, and source.
    pub fn normalize(&mut self) {
        self.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.licenses.sort_by(|a, b| {
            (&a.component, &a.version, &a.source).cmp(&(&b.component, &b.version, &b.source))
        });
    }

    /// The first file with `role` (each required binary role has exactly one).
    pub fn file(&self, role: FileRole) -> Option<&BundleFile> {
        self.files.iter().find(|file| file.role == role)
    }

    /// The file listed at `path`.
    pub fn find(&self, path: &str) -> Option<&BundleFile> {
        self.files.iter().find(|file| file.path == path)
    }

    /// Files verified before every launch.
    pub fn critical_files(&self) -> impl Iterator<Item = &BundleFile> {
        self.files.iter().filter(|file| file.role.is_critical())
    }

    /// Check every semantic rule of the schema.
    pub fn validate(&self) -> Result<(), DocumentError> {
        let invalid = |reason: String| DocumentError::Invalid(reason);
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(DocumentError::UnsupportedSchema {
                found: Some(u64::from(self.schema_version)),
                supported: MANIFEST_SCHEMA_VERSION,
            });
        }
        if self.product != PRODUCT_NAME {
            return Err(invalid(format!(
                "product `{}` is not `{PRODUCT_NAME}`",
                self.product
            )));
        }
        validate_version(&self.version).map_err(invalid)?;
        if let Some(commit) = &self.git_commit {
            validate_git_commit(commit).map_err(invalid)?;
        }
        check_text("rust_target", &self.rust_target)?;
        if let Some(rustc) = &self.rustc {
            check_text("rustc", rustc)?;
        }
        if self.min_drive_schema == 0 {
            return Err(invalid("min_drive_schema must be at least 1".into()));
        }
        self.state.validate()?;
        self.validate_files()?;
        self.validate_licenses()
    }

    fn validate_files(&self) -> Result<(), DocumentError> {
        let invalid = |reason: String| DocumentError::Invalid(reason);
        if self.files.is_empty() {
            return Err(invalid("files must not be empty".into()));
        }
        let mut files = HashSet::new();
        let mut directories = HashSet::new();
        for file in &self.files {
            validate_bundle_path(&file.path).map_err(invalid)?;
            let folded = fold(&file.path);
            if folded == MANIFEST_FILE_NAME {
                return Err(invalid(format!("`{}` is reserved", file.path)));
            }
            if !is_sha256_hex(&file.sha256) {
                return Err(invalid(format!(
                    "`{}` has an invalid sha256 (expected 64 lowercase hex digits)",
                    file.path
                )));
            }
            if !file
                .role
                .allowed_prefixes()
                .iter()
                .any(|prefix| file.path.starts_with(prefix))
            {
                return Err(invalid(format!(
                    "`{}` has role {} and must be under {}",
                    file.path,
                    file.role.as_str(),
                    file.role.allowed_prefixes().join(" or ")
                )));
            }
            if file.role.requires_executable() && !file.executable {
                return Err(invalid(format!(
                    "`{}` has role {} and must be executable",
                    file.path,
                    file.role.as_str()
                )));
            }
            let mut ancestor = folded.as_str();
            while let Some((parent, _)) = ancestor.rsplit_once('/') {
                directories.insert(parent.to_owned());
                ancestor = parent;
            }
            if !files.insert(folded) {
                return Err(invalid(format!(
                    "`{}` is listed more than once (paths are compared case-insensitively)",
                    file.path
                )));
            }
        }
        if let Some(conflict) = files.iter().find(|path| directories.contains(*path)) {
            return Err(invalid(format!(
                "`{conflict}` is listed both as a file and as a directory"
            )));
        }
        for (role, stem) in REQUIRED_BINARIES {
            let expected = bin_path(stem, self.target);
            let listed: Vec<_> = self.files.iter().filter(|file| file.role == role).collect();
            if listed.len() != 1 || listed[0].path != expected {
                return Err(invalid(format!(
                    "a {} bundle must list exactly one {} file, at `{expected}`",
                    self.target,
                    role.as_str()
                )));
            }
        }
        Ok(())
    }

    fn validate_licenses(&self) -> Result<(), DocumentError> {
        for license in &self.licenses {
            check_text("license component", &license.component)?;
            check_text("license version", &license.version)?;
            check_text("license spdx", &license.spdx)?;
            check_text("license source", &license.source)?;
            if let Some(path) = &license.path {
                let listed = self
                    .find(path)
                    .is_some_and(|file| file.role == FileRole::Doc);
                if !listed {
                    return Err(DocumentError::Invalid(format!(
                        "license text `{path}` for {} is not a listed doc file",
                        license.component
                    )));
                }
            }
        }
        Ok(())
    }
}

fn check_text(what: &str, value: &str) -> Result<(), DocumentError> {
    if value.is_empty() || value.len() > MAX_TEXT_LEN || value.chars().any(char::is_control) {
        return Err(DocumentError::Invalid(format!(
            "{what} must be 1..={MAX_TEXT_LEN} bytes without control characters"
        )));
    }
    Ok(())
}

/// Parse a JSON object whose `schema_version` must equal `supported`, then decode it
/// strictly. The version is checked first so a newer document reports
/// [`DocumentError::UnsupportedSchema`] rather than an unknown-field error.
pub(crate) fn parse_versioned<T: DeserializeOwned>(
    bytes: &[u8],
    supported: u32,
) -> Result<T, DocumentError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| DocumentError::Malformed(error.to_string()))?;
    if !value.is_object() {
        return Err(DocumentError::Malformed("expected a JSON object".into()));
    }
    let found = value.get("schema_version").and_then(Value::as_u64);
    if found != Some(u64::from(supported)) {
        return Err(DocumentError::UnsupportedSchema { found, supported });
    }
    serde_json::from_value(value).map_err(|error| DocumentError::Malformed(error.to_string()))
}
