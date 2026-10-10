//! Drive root resolution plus the `init` and `status` operations.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    DriveError, DriveManifest, MANIFEST_FILE_NAME, RelativePath,
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

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
