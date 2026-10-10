//! `DrivePaths`: absolute service directories resolved from a schema-v2 manifest.

use std::{
    ffi::OsStr,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use crate::{
    DriveError, DriveLayout, MANIFEST_FILE_NAME, RelativePath, StoredManifest,
    ops::{read_stored_manifest, resolve_root},
};

/// Drive-relative directory for daemon runtime metadata (endpoint, capability).
const RUNTIME_STATE_DIR: [&str; 2] = [".aidex", "runtime"];

/// Absolute locations of every directory declared by a drive's schema-v2 layout.
///
/// Accessors are lexical and perform no I/O; call [`DrivePaths::ensure_dir`] to create a
/// directory safely before use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrivePaths {
    root: PathBuf,
    layout: DriveLayout,
}

impl DrivePaths {
    /// Resolve `root` and read its manifest without modifying anything.
    ///
    /// Requires schema v2: a schema-v1 drive returns [`DriveError::MigrationRequired`]
    /// (run `aidex init --drive <root>`). The removable-media check is not applied
    /// because services run against drives that were already initialized.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, DriveError> {
        let root = resolve_root(Some(root.as_ref()))?;
        match read_stored_manifest(&root.join(MANIFEST_FILE_NAME))? {
            (StoredManifest::V2(manifest), _) => Ok(Self {
                root,
                layout: manifest.layout,
            }),
            (StoredManifest::V1(_), _) => Err(DriveError::MigrationRequired(root)),
        }
    }

    /// Build paths for an already-known root and layout without I/O or validation
    /// (tests and embedding). `root` is trusted as given.
    pub fn from_layout(root: PathBuf, layout: DriveLayout) -> Self {
        Self { root, layout }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn layout(&self) -> &DriveLayout {
        &self.layout
    }

    fn resolve(&self, path: &RelativePath) -> PathBuf {
        path.resolve_under(&self.root)
    }

    /// Portable runtime bundles (`public_runtime`).
    pub fn runtime(&self) -> PathBuf {
        self.resolve(&self.layout.public_runtime)
    }

    /// Model files and the model registry (`public_models`).
    pub fn models(&self) -> PathBuf {
        self.resolve(&self.layout.public_models)
    }

    /// Agent definitions and adapters (`public_agents`).
    pub fn agents(&self) -> PathBuf {
        self.resolve(&self.layout.public_agents)
    }

    /// Portable skills (`public_skills`).
    pub fn skills(&self) -> PathBuf {
        self.resolve(&self.layout.public_skills)
    }

    /// MCP server definitions (`public_mcp`).
    pub fn mcp(&self) -> PathBuf {
        self.resolve(&self.layout.public_mcp)
    }

    /// Installed plugins (`public_plugins`).
    pub fn plugins(&self) -> PathBuf {
        self.resolve(&self.layout.public_plugins)
    }

    /// User policy files (`public_policies`).
    pub fn policies(&self) -> PathBuf {
        self.resolve(&self.layout.public_policies)
    }

    /// Drive-resident and ephemeral workspaces (`workspaces`).
    pub fn workspaces(&self) -> PathBuf {
        self.resolve(&self.layout.workspaces)
    }

    /// Rebuildable cache data (`cache`).
    pub fn cache(&self) -> PathBuf {
        self.resolve(&self.layout.cache)
    }

    /// Short-lived scratch files (`tmp`).
    pub fn tmp(&self) -> PathBuf {
        self.resolve(&self.layout.tmp)
    }

    /// Service logs (`logs`).
    pub fn logs(&self) -> PathBuf {
        self.resolve(&self.layout.logs)
    }

    /// Application bundles and staged updates (`app`).
    pub fn app(&self) -> PathBuf {
        self.resolve(&self.layout.app)
    }

    /// Integrity manifests such as checksums and SBOMs (`manifests`).
    pub fn manifests(&self) -> PathBuf {
        self.resolve(&self.layout.manifests)
    }

    /// Encrypted vault storage (`private_vault`).
    pub fn vault(&self) -> PathBuf {
        self.resolve(&self.layout.private_vault)
    }

    /// Drive identity keys (`private_identity`).
    pub fn identity(&self) -> PathBuf {
        self.resolve(&self.layout.private_identity)
    }

    /// Session and event state (`private_sessions`).
    pub fn sessions(&self) -> PathBuf {
        self.resolve(&self.layout.private_sessions)
    }

    /// Private configuration (`private_config`).
    pub fn config(&self) -> PathBuf {
        self.resolve(&self.layout.private_config)
    }

    /// Portable user memory (`private_memory`).
    pub fn memory(&self) -> PathBuf {
        self.resolve(&self.layout.private_memory)
    }

    /// Per-workspace search indexes (`private_index`).
    pub fn index(&self) -> PathBuf {
        self.resolve(&self.layout.private_index)
    }

    /// Daemon runtime metadata: `<root>/.aidex/runtime`.
    pub fn runtime_state(&self) -> PathBuf {
        RUNTIME_STATE_DIR
            .iter()
            .fold(self.root.clone(), |path, segment| path.join(segment))
    }

    /// The runtime bundle directory for this host, `runtime/<os>-<arch>`, or `None`
    /// when the host platform has no normalized runtime name.
    pub fn platform_runtime(&self) -> Option<PathBuf> {
        platform_runtime_name().map(|name| self.runtime().join(name))
    }

    /// Create `dir` (and missing parents) when it is a declared layout directory, the
    /// runtime-state directory, or beneath one; returns the absolute path.
    ///
    /// `dir` may be absolute (it must lie under [`Self::root`] exactly as spelled; derive
    /// it from an accessor) or drive-relative. Traversal components, unmanaged
    /// locations, and existing symbolic links, junctions, or non-directories beneath the
    /// root are rejected.
    pub fn ensure_dir(&self, dir: &Path) -> Result<PathBuf, DriveError> {
        let unmanaged = || DriveError::UnmanagedPath(dir.to_path_buf());
        let relative = if dir.is_absolute() {
            dir.strip_prefix(&self.root).map_err(|_| unmanaged())?
        } else {
            dir
        };
        let mut segments = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(segment) => segments.push(segment),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(unmanaged());
                }
            }
        }
        let is_beneath = |base: &[&str]| {
            segments.len() >= base.len()
                && base
                    .iter()
                    .zip(&segments)
                    .all(|(expected, actual)| OsStr::new(expected) == *actual)
        };
        let managed = is_beneath(&RUNTIME_STATE_DIR)
            || self
                .layout
                .entries()
                .any(|(_, path)| is_beneath(&path.as_str().split('/').collect::<Vec<_>>()));
        if !managed {
            return Err(unmanaged());
        }
        create_dirs_beneath(&self.root, segments)
    }
}

/// Create each missing directory below a trusted `root`, refusing any existing
/// component that is a symbolic link, junction, or non-directory.
pub(crate) fn create_dirs_beneath<'a>(
    root: &Path,
    segments: impl IntoIterator<Item = &'a OsStr>,
) -> Result<PathBuf, DriveError> {
    let mut current = root.to_path_buf();
    for segment in segments {
        current.push(segment);
        let created = match fs::symlink_metadata(&current) {
            Ok(_) => false,
            Err(error) if error.kind() == ErrorKind::NotFound => match fs::create_dir(&current) {
                Ok(()) => true,
                // A concurrent creator won the race; inspect what it created.
                Err(error) if error.kind() == ErrorKind::AlreadyExists => false,
                Err(error) => {
                    return Err(DriveError::Io(format!(
                        "cannot create AiDEX directory `{}`: {error}",
                        current.display()
                    )));
                }
            },
            Err(error) => {
                return Err(DriveError::Io(format!(
                    "cannot inspect AiDEX directory `{}`: {error}",
                    current.display()
                )));
            }
        };
        if !created {
            let metadata = fs::symlink_metadata(&current).map_err(|error| {
                DriveError::Io(format!(
                    "cannot inspect AiDEX directory `{}`: {error}",
                    current.display()
                ))
            })?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(DriveError::UnsafeComponent(current));
            }
        }
    }
    Ok(current)
}

/// The normalized runtime directory name for the compiled host platform.
pub fn platform_runtime_name() -> Option<&'static str> {
    runtime_name_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn runtime_name_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("windows-x64"),
        ("linux", "x86_64") => Some("linux-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("macos", "aarch64") => Some("macos-arm64"),
        ("macos", "x86_64") => Some("macos-x64"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
