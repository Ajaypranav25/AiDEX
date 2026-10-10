//! Lexical paths and the versioned on-drive manifest contract.
//!
//! Schema version 1 separates public runtime and model directories from private
//! encrypted-data directories for the vault, identity, sessions, and config.
//! Paths are validated as relative paths before a manifest is accepted. The
//! reader accepts only the current version; it never guesses migrations for
//! older or newer schemas.

use std::{
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
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
