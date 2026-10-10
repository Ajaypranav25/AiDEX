//! Drive directory layouts: the current schema-v2 layout, the legacy v1 layout, and
//! the overlap rules every written layout must satisfy.

use std::collections::HashMap;

use serde::Serialize;

use crate::{ManifestError, RelativePath};

/// Root-level names AiDEX reserves for drive metadata; layout entries never claim them.
pub(crate) const RESERVED_ROOT_NAMES: [&str; 4] = [
    crate::MANIFEST_FILE_NAME,
    crate::V1_BACKUP_FILE_NAME,
    crate::migration::MIGRATION_TEMP_FILE_NAME,
    ".aidex",
];

/// Directories owned by a schema-v2 drive. Fields are relative to the drive root.
///
/// `public_*` and top-level operational directories hold data that is safe to keep
/// unencrypted; `private_*` directories hold state that services must protect.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DriveLayout {
    pub public_runtime: RelativePath,
    pub public_models: RelativePath,
    pub private_vault: RelativePath,
    pub private_identity: RelativePath,
    pub private_sessions: RelativePath,
    pub private_config: RelativePath,
    pub public_agents: RelativePath,
    pub public_skills: RelativePath,
    pub public_mcp: RelativePath,
    pub public_plugins: RelativePath,
    pub public_policies: RelativePath,
    pub workspaces: RelativePath,
    pub cache: RelativePath,
    pub tmp: RelativePath,
    pub logs: RelativePath,
    pub app: RelativePath,
    pub manifests: RelativePath,
    pub private_memory: RelativePath,
    pub private_index: RelativePath,
}

impl Default for DriveLayout {
    fn default() -> Self {
        DriveLayoutV1::default().with_v2_defaults()
    }
}

impl DriveLayout {
    /// Every declared directory as `(manifest field name, path)`, in schema order.
    pub fn entries(&self) -> impl Iterator<Item = (&'static str, &RelativePath)> {
        [
            ("public_runtime", &self.public_runtime),
            ("public_models", &self.public_models),
            ("private_vault", &self.private_vault),
            ("private_identity", &self.private_identity),
            ("private_sessions", &self.private_sessions),
            ("private_config", &self.private_config),
            ("public_agents", &self.public_agents),
            ("public_skills", &self.public_skills),
            ("public_mcp", &self.public_mcp),
            ("public_plugins", &self.public_plugins),
            ("public_policies", &self.public_policies),
            ("workspaces", &self.workspaces),
            ("cache", &self.cache),
            ("tmp", &self.tmp),
            ("logs", &self.logs),
            ("app", &self.app),
            ("manifests", &self.manifests),
            ("private_memory", &self.private_memory),
            ("private_index", &self.private_index),
        ]
        .into_iter()
    }

    /// Reject entries that name the same directory or claim reserved metadata names.
    ///
    /// Paths are compared case-insensitively, ignoring trailing dots and spaces per
    /// segment, because common removable filesystems (exFAT, FAT32, NTFS) treat those
    /// spellings as one directory. Nesting one entry beneath another is allowed.
    pub fn validate(&self) -> Result<(), ManifestError> {
        let mut seen: HashMap<String, &'static str> = HashMap::new();
        for (name, path) in self.entries() {
            let key = comparison_key(path);
            let first_segment = key.split('/').next().unwrap_or_default();
            if RESERVED_ROOT_NAMES
                .iter()
                .any(|reserved| reserved.eq_ignore_ascii_case(first_segment))
            {
                return Err(ManifestError::ReservedPath(name));
            }
            if let Some(first) = seen.insert(key, name) {
                return Err(ManifestError::DuplicatePath {
                    first,
                    second: name,
                });
            }
        }
        Ok(())
    }
}

fn comparison_key(path: &RelativePath) -> String {
    path.as_str()
        .split('/')
        .map(|segment| segment.trim_end_matches(['.', ' ']).to_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}

/// The six directories declared by a schema-v1 manifest; readable for migration only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriveLayoutV1 {
    pub public_runtime: RelativePath,
    pub public_models: RelativePath,
    pub private_vault: RelativePath,
    pub private_identity: RelativePath,
    pub private_sessions: RelativePath,
    pub private_config: RelativePath,
}

impl Default for DriveLayoutV1 {
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

impl DriveLayoutV1 {
    /// The explicit v1 → v2 layout transform: keep all six v1 paths unchanged and add
    /// the v2 directories at their defaults. The result is not yet validated.
    pub fn with_v2_defaults(self) -> DriveLayout {
        DriveLayout {
            public_runtime: self.public_runtime,
            public_models: self.public_models,
            private_vault: self.private_vault,
            private_identity: self.private_identity,
            private_sessions: self.private_sessions,
            private_config: self.private_config,
            public_agents: RelativePath::trusted("agents"),
            public_skills: RelativePath::trusted("skills"),
            public_mcp: RelativePath::trusted("mcp"),
            public_plugins: RelativePath::trusted("plugins"),
            public_policies: RelativePath::trusted("policies"),
            workspaces: RelativePath::trusted("workspaces"),
            cache: RelativePath::trusted("cache"),
            tmp: RelativePath::trusted("tmp"),
            logs: RelativePath::trusted("logs"),
            app: RelativePath::trusted("app"),
            manifests: RelativePath::trusted("manifests"),
            private_memory: RelativePath::trusted("private/memory"),
            private_index: RelativePath::trusted("private/index"),
        }
    }
}
