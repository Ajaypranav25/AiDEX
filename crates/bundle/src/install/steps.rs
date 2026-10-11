//! The individual install steps, for callers (the updater) that run them in their own
//! order with their own journal. [`super::install_with`] composes the same steps.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use aidex_drive::DrivePaths;

use super::{place, stage as staging};
use crate::{
    BundleError, BundleManifest, InstallSource, VerifiedBundle, VerifyScope,
    digest::sha256_hex,
    fsutil::sync_dir,
    path::version_dir_name,
    verify::{read_manifest_bytes, verify_files},
};

pub use super::place::{LauncherPlacement, PlacementAction};

/// Copy (directory source) or extract (archive source) `source` into the empty
/// directory `staging` and return the staged bundle directory. Nothing is verified.
pub fn stage(source: &InstallSource, staging: &Path) -> Result<PathBuf, BundleError> {
    staging::stage(source, staging)
}

/// Apply the manifest's executable flags (Unix modes 0755 / 0644) to staged files.
pub fn apply_modes(bundle_dir: &Path, manifest: &BundleManifest) -> Result<(), BundleError> {
    staging::apply_modes(bundle_dir, manifest)
}

/// Fail before anything is committed if the bundle's launcher is not a marked AiDEX
/// launcher or a drive-root launcher destination holds a file AiDEX may not replace.
pub fn preflight_launchers(
    paths: &DrivePaths,
    bundle_dir: &Path,
    manifest: &BundleManifest,
) -> Result<(), BundleError> {
    place::preflight(paths, bundle_dir, manifest)
}

/// Write the drive-root launchers of an installed bundle (idempotent: identical files
/// are left alone; a replaced launcher is kept as `<name>.previous`).
pub fn place_launchers(
    paths: &DrivePaths,
    bundle_dir: &Path,
    manifest: &BundleManifest,
) -> Result<Vec<LauncherPlacement>, BundleError> {
    place::place_launchers(paths, bundle_dir, manifest)
}

/// Where a verified bundle goes under `app/versions/`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitPlan {
    /// A verified identical copy is already installed under this name.
    Reuse(String),
    /// Move the staged bundle to this (currently absent) name.
    Move(String),
}

impl CommitPlan {
    pub fn name(&self) -> &str {
        match self {
            Self::Reuse(name) | Self::Move(name) => name,
        }
    }

    pub fn is_reuse(&self) -> bool {
        matches!(self, Self::Reuse(_))
    }
}

/// Choose the version directory for a verified bundle: `<version>-<target>` when free,
/// a verified identical copy when one exists, otherwise a digest-suffixed name, so a
/// committed directory is never modified.
pub fn plan_commit(versions: &Path, bundle: &VerifiedBundle) -> Result<CommitPlan, BundleError> {
    let manifest = &bundle.manifest;
    let base = version_dir_name(&manifest.version, manifest.target);
    let suffixed = format!("{base}-{}", &bundle.manifest_sha256[..12]);
    let candidates = std::iter::once(base)
        .chain(std::iter::once(suffixed.clone()))
        .chain((2..=8).map(|n| format!("{suffixed}-{n}")));
    for name in candidates {
        let destination = versions.join(&name);
        match fs::symlink_metadata(&destination) {
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(CommitPlan::Move(name));
            }
            Err(error) => {
                return Err(BundleError::io(
                    format!("cannot inspect `{}`", destination.display()),
                    error,
                ));
            }
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                let identical = read_manifest_bytes(&destination)
                    .is_ok_and(|bytes| sha256_hex(&bytes) == bundle.manifest_sha256);
                if identical && verify_files(&destination, manifest, VerifyScope::All).is_ok() {
                    return Ok(CommitPlan::Reuse(name));
                }
                // A different build, or a damaged copy: leave it and try the next name.
            }
            Ok(_) => {}
        }
    }
    Err(BundleError::Occupied {
        path: versions.join(suffixed),
    })
}

/// Carry out `plan`: rename `staged` into `versions/` (same volume) for a move; a
/// reuse leaves everything in place.
pub fn execute_commit(
    staged: &Path,
    versions: &Path,
    plan: &CommitPlan,
) -> Result<(), BundleError> {
    let CommitPlan::Move(name) = plan else {
        return Ok(());
    };
    let destination = versions.join(name);
    fs::rename(staged, &destination).map_err(|error| {
        BundleError::io(
            format!(
                "cannot move the staged bundle to `{}`",
                destination.display()
            ),
            error,
        )
    })?;
    sync_dir(versions);
    Ok(())
}
