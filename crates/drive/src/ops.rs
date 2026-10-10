//! Drive root resolution plus the `init` and `status` operations.

use std::{
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    CURRENT_SCHEMA_VERSION, DriveError, DriveManifest, MANIFEST_FILE_NAME, RelativePath,
    StoredManifest,
    migration::migrate_v1_manifest,
    paths::create_dirs_beneath,
    platform::{available_bytes, is_windows_drive_root, require_removable},
};

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

impl DriveStatus {
    /// Whether the manifest uses an older supported schema that `aidex init` migrates.
    pub fn migration_required(&self) -> bool {
        self.schema_version
            .is_some_and(|version| version < CURRENT_SCHEMA_VERSION)
    }

    /// User-facing advice when [`Self::migration_required`] is true.
    pub fn migration_advice(&self) -> Option<String> {
        let version = self.schema_version.filter(|_| self.migration_required())?;
        Some(format!(
            "manifest schema v{version} can be migrated to v{CURRENT_SCHEMA_VERSION}; run `aidex init --drive {}` (the original manifest is kept as a backup)",
            self.root.display()
        ))
    }
}

/// Options for [`init_with`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitOptions {
    /// Require the root to be a recognized removable volume (default `true`).
    ///
    /// `false` is intended only for tests and explicitly chosen developer folders;
    /// AiDEX never disables the check implicitly. Manifest validation, migration,
    /// directory creation, and the free-space check still apply.
    pub require_removable: bool,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self {
            require_removable: true,
        }
    }
}

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

/// Initialize manifest-declared directories on a removable drive without replacing
/// existing content. A valid schema-v1 manifest is explicitly migrated to v2 first.
/// Safe to retry after interruption; malformed or unsupported manifests are never changed.
pub fn init(root: impl AsRef<Path>) -> Result<DriveStatus, DriveError> {
    init_with(root, InitOptions::default())
}

/// [`init`] with explicit options; see [`InitOptions::require_removable`].
pub fn init_with(root: impl AsRef<Path>, options: InitOptions) -> Result<DriveStatus, DriveError> {
    let root = resolve_root(Some(root.as_ref()))?;
    if options.require_removable {
        require_removable(&root)?;
    }
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
/// A schema-v1 drive succeeds and reports [`DriveStatus::migration_required`].
pub fn status(root: impl AsRef<Path>) -> Result<DriveStatus, DriveError> {
    let root = resolve_root(Some(root.as_ref()))?;
    require_removable(&root)?;
    let writable = writable_probe(&root).is_ok();
    let free_bytes = available_bytes(&root).ok();
    status_at(&root, writable, free_bytes)
}

pub(crate) fn initialize_at(root: &Path) -> Result<(), DriveError> {
    let manifest_path = root.join(MANIFEST_FILE_NAME);
    let manifest = match read_stored_manifest(&manifest_path) {
        Ok((StoredManifest::V2(manifest), _)) => manifest,
        Ok((StoredManifest::V1(legacy), original)) => {
            writable_probe(root)?;
            migrate_v1_manifest(root, &original, &legacy)?
        }
        Err(DriveError::MissingManifest(_)) => DriveManifest::current(),
        Err(error) => return Err(error),
    };
    writable_probe(root)?;

    for (_, relative) in manifest.layout.entries() {
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
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            if let (StoredManifest::V1(_), _) = read_stored_manifest(&manifest_path)? {
                return Err(DriveError::MigrationRequired(root.to_path_buf()));
            }
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
    create_dirs_beneath(root, relative.as_str().split('/').map(OsStr::new)).map(drop)
}

fn status_at(
    root: &Path,
    writable: bool,
    free_bytes: Option<u64>,
) -> Result<DriveStatus, DriveError> {
    let (manifest, _) = read_stored_manifest(&root.join(MANIFEST_FILE_NAME))?;
    Ok(DriveStatus {
        root: root.to_path_buf(),
        writable,
        free_bytes,
        schema_version: Some(manifest.schema_version()),
    })
}

/// Read and decode a regular-file manifest at any supported schema version, returning
/// the exact bytes alongside it. Never modifies the file.
pub(crate) fn read_stored_manifest(path: &Path) -> Result<(StoredManifest, Vec<u8>), DriveError> {
    let corrupt = |reason: String| DriveError::CorruptManifest(path.to_path_buf(), reason);
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(DriveError::MissingManifest(path.to_path_buf()));
        }
        Err(error) => return Err(corrupt(error.to_string())),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(corrupt("manifest is not a regular file".into()));
    }
    let bytes = fs::read(path).map_err(|error| corrupt(error.to_string()))?;
    let content =
        std::str::from_utf8(&bytes).map_err(|_| corrupt("manifest is not valid UTF-8".into()))?;
    let manifest =
        StoredManifest::from_json(content).map_err(|error| corrupt(error.to_string()))?;
    Ok((manifest, bytes))
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

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
