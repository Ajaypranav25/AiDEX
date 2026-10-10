//! The root-level drive manifest: closed JSON decoding of every supported schema
//! version and canonical encoding of the current one.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    DriveLayout, DriveLayoutV1,
    path::{PathError, RelativePath},
};

/// Root-level file used to identify an AiDEX drive.
pub const MANIFEST_FILE_NAME: &str = "aidex-drive.json";

/// The manifest schema written by this version of the drive crate.
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

/// The legacy schema that is still read and can be migrated by `init`.
pub const V1_SCHEMA_VERSION: u32 = 1;

/// A validated schema-v2 description of the directories owned by a drive.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DriveManifest {
    schema_version: u32,
    pub layout: DriveLayout,
}

/// A decoded schema-v1 manifest. It is never written; `init` migrates it to v2.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriveManifestV1 {
    pub layout: DriveLayoutV1,
}

/// A manifest decoded at whichever supported schema version it was stored with.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "decoded once per drive operation and immediately destructured"
)]
pub enum StoredManifest {
    V1(DriveManifestV1),
    V2(DriveManifest),
}

/// Stable errors returned while decoding or validating a drive manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    MalformedJson,
    InvalidManifest,
    UnsupportedVersion(u32),
    InvalidPath(PathError),
    /// Two layout entries name the same directory.
    DuplicatePath {
        first: &'static str,
        second: &'static str,
    },
    /// A layout entry claims a root name reserved for AiDEX drive metadata.
    ReservedPath(&'static str),
    /// The manifest uses an older supported schema and must be migrated first.
    MigrationRequired(u32),
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
            Self::DuplicatePath { first, second } => write!(
                f,
                "layout entries `{first}` and `{second}` name the same directory"
            ),
            Self::ReservedPath(name) => write!(
                f,
                "layout entry `{name}` uses a root name reserved for AiDEX drive metadata"
            ),
            Self::MigrationRequired(version) => write!(
                f,
                "manifest schema version {version} must be migrated to version {CURRENT_SCHEMA_VERSION}"
            ),
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

    /// Canonical pretty-printed JSON; the layout is validated first so an invalid
    /// manifest is never produced.
    pub fn to_json(&self) -> Result<String, ManifestError> {
        self.layout.validate()?;
        serde_json::to_string_pretty(self)
            .map(|json| json + "\n")
            .map_err(|_| ManifestError::InvalidManifest)
    }

    /// Decode a current-schema manifest. Schema v1 reports
    /// [`ManifestError::MigrationRequired`]; use [`StoredManifest::from_json`] to read it.
    pub fn from_json(json: &str) -> Result<Self, ManifestError> {
        match StoredManifest::from_json(json)? {
            StoredManifest::V2(manifest) => Ok(manifest),
            StoredManifest::V1(_) => Err(ManifestError::MigrationRequired(V1_SCHEMA_VERSION)),
        }
    }
}

impl DriveManifestV1 {
    /// Explicit v1 → v2 transform: preserve the six v1 paths, add v2 defaults, and
    /// validate the result (for example, a v1 path that collides with a new default).
    pub fn migrate_to_v2(&self) -> Result<DriveManifest, ManifestError> {
        let manifest = DriveManifest::new(self.layout.clone().with_v2_defaults());
        manifest.layout.validate()?;
        Ok(manifest)
    }
}

impl StoredManifest {
    /// Decode schema v1 or v2 with closed schemas (unknown fields are rejected).
    /// Version 0 is invalid; versions newer than [`CURRENT_SCHEMA_VERSION`] are unsupported.
    pub fn from_json(json: &str) -> Result<Self, ManifestError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawManifest {
            schema_version: u32,
            layout: serde_json::Value,
        }

        let raw: RawManifest =
            serde_json::from_str(json).map_err(|_| ManifestError::MalformedJson)?;
        match raw.schema_version {
            V1_SCHEMA_VERSION => decode_v1(raw.layout).map(Self::V1),
            CURRENT_SCHEMA_VERSION => decode_v2(raw.layout).map(Self::V2),
            0 => Err(ManifestError::InvalidManifest),
            version => Err(ManifestError::UnsupportedVersion(version)),
        }
    }

    pub fn schema_version(&self) -> u32 {
        match self {
            Self::V1(_) => V1_SCHEMA_VERSION,
            Self::V2(manifest) => manifest.schema_version(),
        }
    }
}

fn path(value: String) -> Result<RelativePath, ManifestError> {
    RelativePath::new(value).map_err(ManifestError::InvalidPath)
}

fn decode_v1(layout: serde_json::Value) -> Result<DriveManifestV1, ManifestError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawLayoutV1 {
        public_runtime: String,
        public_models: String,
        private_vault: String,
        private_identity: String,
        private_sessions: String,
        private_config: String,
    }

    let raw: RawLayoutV1 =
        serde_json::from_value(layout).map_err(|_| ManifestError::InvalidManifest)?;
    Ok(DriveManifestV1 {
        layout: DriveLayoutV1 {
            public_runtime: path(raw.public_runtime)?,
            public_models: path(raw.public_models)?,
            private_vault: path(raw.private_vault)?,
            private_identity: path(raw.private_identity)?,
            private_sessions: path(raw.private_sessions)?,
            private_config: path(raw.private_config)?,
        },
    })
}

fn decode_v2(layout: serde_json::Value) -> Result<DriveManifest, ManifestError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawLayoutV2 {
        public_runtime: String,
        public_models: String,
        private_vault: String,
        private_identity: String,
        private_sessions: String,
        private_config: String,
        public_agents: String,
        public_skills: String,
        public_mcp: String,
        public_plugins: String,
        public_policies: String,
        workspaces: String,
        cache: String,
        tmp: String,
        logs: String,
        app: String,
        manifests: String,
        private_memory: String,
        private_index: String,
    }

    let raw: RawLayoutV2 =
        serde_json::from_value(layout).map_err(|_| ManifestError::InvalidManifest)?;
    let manifest = DriveManifest::new(DriveLayout {
        public_runtime: path(raw.public_runtime)?,
        public_models: path(raw.public_models)?,
        private_vault: path(raw.private_vault)?,
        private_identity: path(raw.private_identity)?,
        private_sessions: path(raw.private_sessions)?,
        private_config: path(raw.private_config)?,
        public_agents: path(raw.public_agents)?,
        public_skills: path(raw.public_skills)?,
        public_mcp: path(raw.public_mcp)?,
        public_plugins: path(raw.public_plugins)?,
        public_policies: path(raw.public_policies)?,
        workspaces: path(raw.workspaces)?,
        cache: path(raw.cache)?,
        tmp: path(raw.tmp)?,
        logs: path(raw.logs)?,
        app: path(raw.app)?,
        manifests: path(raw.manifests)?,
        private_memory: path(raw.private_memory)?,
        private_index: path(raw.private_index)?,
    });
    manifest.layout.validate()?;
    Ok(manifest)
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
