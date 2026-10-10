//! Placing launchers at the drive root without ever replacing a non-AiDEX file.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use aidex_drive::DrivePaths;
use serde::Serialize;

use crate::{
    BundleError, BundleManifest, FileRole, Target,
    digest::{sha256_file, sha256_hex},
    entry::{is_aidex_launcher, unix_entry_script},
    fsutil::{copy_synced, set_executable, sync_dir},
    layout::{
        LAUNCHER_BINARY, PREVIOUS_SUFFIX, UNIX_ROOT_LAUNCHER, WINDOWS_ROOT_LAUNCHER,
        unix_launcher_dir, with_suffix,
    },
    path::join_relative as join,
};

/// Suffix of a launcher written next to its destination before the final rename.
const NEW_SUFFIX: &str = ".aidex-new";

/// What happened to one launcher path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementAction {
    Created,
    /// An older AiDEX launcher was replaced and kept as `<name>.previous`.
    Replaced,
    /// The same launcher was already in place.
    Unchanged,
}

/// One launcher written by an install.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LauncherPlacement {
    pub path: PathBuf,
    pub action: PlacementAction,
    /// Where the replaced launcher was kept.
    pub previous: Option<PathBuf>,
}

/// Launcher destinations for `target`: `AiDEX.exe` at the root on Windows; the
/// launcher binary under `app/launcher/<target>/` plus the root `aidex` script elsewhere.
fn destinations(paths: &DrivePaths, target: Target) -> Vec<PathBuf> {
    if target.is_windows() {
        vec![paths.root().join(WINDOWS_ROOT_LAUNCHER)]
    } else {
        vec![
            unix_launcher_dir(paths, target).join(LAUNCHER_BINARY),
            paths.root().join(UNIX_ROOT_LAUNCHER),
        ]
    }
}

/// Fail before anything is committed if the bundle's launcher is not a marked AiDEX
/// launcher, or a launcher destination (or its `.previous` / temporary sibling) is
/// occupied by something that is not an AiDEX launcher.
pub(super) fn preflight(
    paths: &DrivePaths,
    bundle_dir: &Path,
    manifest: &BundleManifest,
) -> Result<(), BundleError> {
    launcher_source(bundle_dir, manifest)?;
    for destination in destinations(paths, manifest.target) {
        for path in [
            destination.clone(),
            with_suffix(&destination, PREVIOUS_SUFFIX),
            with_suffix(&destination, NEW_SUFFIX),
        ] {
            require_replaceable(&path)?;
        }
    }
    Ok(())
}

/// Write the launchers of an installed bundle.
pub(super) fn place_launchers(
    paths: &DrivePaths,
    bundle_dir: &Path,
    manifest: &BundleManifest,
) -> Result<Vec<LauncherPlacement>, BundleError> {
    let binary = Contents::File(launcher_source(bundle_dir, manifest)?);
    let target = manifest.target;
    if target.is_windows() {
        return Ok(vec![place(
            &paths.root().join(WINDOWS_ROOT_LAUNCHER),
            &binary,
        )?]);
    }
    let dir = paths.ensure_dir(&unix_launcher_dir(paths, target))?;
    let script = unix_entry_script(paths.layout().app.as_str())?;
    Ok(vec![
        place(&dir.join(LAUNCHER_BINARY), &binary)?,
        place(
            &paths.root().join(UNIX_ROOT_LAUNCHER),
            &Contents::Bytes(script.into_bytes()),
        )?,
    ])
}

/// The bundle's launcher binary. Only marked launchers are placed, so a later install
/// can recognize (and is allowed to replace) what this one writes.
fn launcher_source(bundle_dir: &Path, manifest: &BundleManifest) -> Result<PathBuf, BundleError> {
    let launcher = manifest
        .file(FileRole::Launcher)
        .ok_or_else(|| BundleError::InvalidSource("the bundle lists no launcher".into()))?;
    let path = join(bundle_dir, &launcher.path);
    let marked = is_aidex_launcher(&path)
        .map_err(|error| BundleError::io(format!("cannot read `{}`", path.display()), error))?;
    if !marked {
        return Err(BundleError::InvalidSource(format!(
            "`{}` does not carry the AiDEX launcher marker",
            launcher.path
        )));
    }
    Ok(path)
}

enum Contents {
    File(PathBuf),
    Bytes(Vec<u8>),
}

impl Contents {
    fn digest(&self) -> Result<String, BundleError> {
        match self {
            Self::File(path) => sha256_file(path)
                .map(|(digest, _)| digest)
                .map_err(|error| {
                    BundleError::io(format!("cannot read `{}`", path.display()), error)
                }),
            Self::Bytes(bytes) => Ok(sha256_hex(bytes)),
        }
    }

    fn write_new(&self, destination: &Path) -> Result<(), BundleError> {
        match self {
            Self::File(path) => copy_synced(path, destination, true),
            Self::Bytes(bytes) => OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .and_then(|mut file| {
                    file.write_all(bytes)?;
                    file.sync_all()?;
                    drop(file);
                    set_executable(destination, true)
                })
                .map_err(|error| {
                    BundleError::io(format!("cannot write `{}`", destination.display()), error)
                }),
        }
    }
}

/// Install `contents` at `destination`: write `<name>.aidex-new`, verify it, move an
/// existing AiDEX launcher to `<name>.previous`, then rename the new file into place.
fn place(destination: &Path, contents: &Contents) -> Result<LauncherPlacement, BundleError> {
    require_replaceable(destination)?;
    let digest = contents.digest()?;
    let occupied = exists(destination)?;
    if occupied && current_digest(destination)? == digest {
        set_executable(destination, true).map_err(|error| {
            BundleError::io(
                format!("cannot set permissions on `{}`", destination.display()),
                error,
            )
        })?;
        return Ok(LauncherPlacement {
            path: destination.to_path_buf(),
            action: PlacementAction::Unchanged,
            previous: None,
        });
    }
    let staged = with_suffix(destination, NEW_SUFFIX);
    require_replaceable(&staged)?;
    if exists(&staged)? {
        // A launcher left behind by an interrupted install (recognized above).
        fs::remove_file(&staged).map_err(|error| {
            BundleError::io(format!("cannot remove `{}`", staged.display()), error)
        })?;
    }
    contents.write_new(&staged)?;
    if current_digest(&staged)? != digest {
        let _ = fs::remove_file(&staged);
        return Err(BundleError::io(
            format!("`{}` did not read back as written", staged.display()),
            std::io::Error::other("launcher copy verification failed"),
        ));
    }
    let previous = if occupied {
        let previous = with_suffix(destination, PREVIOUS_SUFFIX);
        require_replaceable(&previous)?;
        fs::rename(destination, &previous).map_err(|error| {
            BundleError::io(
                format!(
                    "cannot keep the previous launcher as `{}`",
                    previous.display()
                ),
                error,
            )
        })?;
        Some(previous)
    } else {
        None
    };
    if let Err(error) = fs::rename(&staged, destination) {
        if let Some(previous) = &previous {
            let _ = fs::rename(previous, destination);
        }
        return Err(BundleError::io(
            format!(
                "cannot move the new launcher to `{}`",
                destination.display()
            ),
            error,
        ));
    }
    if let Some(parent) = destination.parent() {
        sync_dir(parent);
    }
    Ok(LauncherPlacement {
        path: destination.to_path_buf(),
        action: if previous.is_some() {
            PlacementAction::Replaced
        } else {
            PlacementAction::Created
        },
        previous,
    })
}

/// `path` is absent, or is a regular file carrying the AiDEX launcher marker.
fn require_replaceable(path: &Path) -> Result<(), BundleError> {
    if !exists(path)? {
        return Ok(());
    }
    match is_aidex_launcher(path) {
        Ok(true) => Ok(()),
        Ok(false) => Err(BundleError::Occupied {
            path: path.to_path_buf(),
        }),
        Err(error) => Err(BundleError::io(
            format!("cannot inspect `{}`", path.display()),
            error,
        )),
    }
}

fn exists(path: &Path) -> Result<bool, BundleError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(BundleError::io(
            format!("cannot inspect `{}`", path.display()),
            error,
        )),
    }
}

fn current_digest(path: &Path) -> Result<String, BundleError> {
    sha256_file(path)
        .map(|(digest, _)| digest)
        .map_err(|error| BundleError::io(format!("cannot read `{}`", path.display()), error))
}
