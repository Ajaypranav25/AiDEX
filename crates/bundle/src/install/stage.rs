//! Staging: copy or extract a bundle into the drive's own staging directory.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    BundleError, BundleManifest, InstallSource, MANIFEST_FILE_NAME,
    fsutil::{copy_synced, set_executable},
    path::join_relative as join,
    verify::{parse_manifest, read_manifest_bytes},
};

/// Stage `source` inside `staging` and return the staged bundle directory.
pub(super) fn stage(source: &InstallSource, staging: &Path) -> Result<PathBuf, BundleError> {
    match source {
        InstallSource::Directory(dir) => copy_bundle(dir, &staging.join("bundle")),
        #[cfg(feature = "archive")]
        InstallSource::Archive(archive) => crate::archive::extract_archive(
            archive,
            staging,
            &crate::archive::ExtractLimits::default(),
        ),
    }
}

/// Copy `bundle.json` and exactly the files it lists; anything else in the source
/// directory is ignored, so the staged bundle holds only manifest-listed files.
fn copy_bundle(source: &Path, destination: &Path) -> Result<PathBuf, BundleError> {
    let bytes = read_manifest_bytes(source)?;
    let manifest = parse_manifest(source, &bytes)?;
    let create = |dir: &Path| {
        fs::create_dir_all(dir)
            .map_err(|error| BundleError::io(format!("cannot create `{}`", dir.display()), error))
    };
    create(destination)?;
    let manifest_path = destination.join(MANIFEST_FILE_NAME);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&manifest_path)
        .and_then(|mut file| {
            file.write_all(&bytes)?;
            file.sync_all()
        })
        .map_err(|error| {
            BundleError::io(format!("cannot write `{}`", manifest_path.display()), error)
        })?;
    for file in &manifest.files {
        let from = join(source, &file.path);
        let to = join(destination, &file.path);
        if let Some(parent) = to.parent() {
            create(parent)?;
        }
        copy_synced(&from, &to, file.executable)?;
    }
    Ok(destination.to_path_buf())
}

/// Apply the manifest's executable flags (Unix modes 0755 / 0644) to staged files.
pub(super) fn apply_modes(bundle_dir: &Path, manifest: &BundleManifest) -> Result<(), BundleError> {
    for file in &manifest.files {
        let path = join(bundle_dir, &file.path);
        // Missing files and links are reported by verification, which follows.
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
            set_executable(&path, file.executable).map_err(|error| {
                BundleError::io(
                    format!("cannot set permissions on `{}`", path.display()),
                    error,
                )
            })?;
        }
    }
    Ok(())
}
