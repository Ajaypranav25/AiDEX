use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::StoreError;

pub(super) fn validate_key(key: &str) -> Result<(), StoreError> {
    if key.is_empty()
        || key.contains('\\')
        || key.contains(':')
        || Path::new(key)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(StoreError::InvalidKey);
    }
    Ok(())
}

/// Return the components of `path` that do not exist yet and would be created for it.
///
/// Ancestors that already exist belong to the host (for example macOS `/var` →
/// `/private/var`) and are resolved by the operating system; only the selected root and
/// the directories AiDEX creates beneath them are required to be real directories.
pub(super) fn missing_components(path: &Path) -> Result<Vec<PathBuf>, StoreError> {
    let mut current = PathBuf::new();
    let mut missing = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => continue,
            Component::ParentDir => return Err(StoreError::InvalidKey),
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                current.push(component.as_os_str());
            }
        }
        match fs::symlink_metadata(&current) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => missing.push(current.clone()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(missing)
}

/// Reject a selected root that is itself a symlink, or any directory created for it that
/// turned out to be a symlink (a concurrent replacement), before the root is followed.
pub(super) fn reject_symlinked_root(root: &Path, created: &[PathBuf]) -> Result<(), StoreError> {
    for path in created
        .iter()
        .map(PathBuf::as_path)
        .chain(std::iter::once(root))
    {
        reject_symlink_if_exists(path)?;
    }
    Ok(())
}

pub(super) fn reject_symlink_if_exists(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreError::InvalidKey),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn create_managed_dir(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(StoreError::InvalidKey)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}
