//! The root-level drive manifest schema and its closed JSON decoding.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::path::{PathError, RelativePath};

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
            public_runtime: RelativePath::trusted("runtime"),
            public_models: RelativePath::trusted("models"),
            private_vault: RelativePath::trusted("private/vault"),
            private_identity: RelativePath::trusted("private/identity"),
            private_sessions: RelativePath::trusted("private/sessions"),
            private_config: RelativePath::trusted("private/config"),
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

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
