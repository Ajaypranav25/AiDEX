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

/// Reject symlinks in every existing component before following or creating a selected root.
pub(super) fn reject_symlink_components(path: &Path) -> Result<(), StoreError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => continue,
            Component::ParentDir => return Err(StoreError::InvalidKey),
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                current.push(component.as_os_str());
            }
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::InvalidKey);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
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
