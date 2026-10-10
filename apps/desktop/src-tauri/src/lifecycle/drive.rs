//! Which AiDEX drive this session works on.
//!
//! Resolution order: a drive chosen in this session (kept in memory only; nothing is
//! written to host-side app config), then `AIDEX_DRIVE_ROOT`, then the drive that
//! contains the running executable (the portable layout: the nearest ancestor holding
//! `aidex-drive.json`), else none and the UI asks the user to choose a drive.

use crate::error::BridgeError;
use aidex_drive::{resolve_root, status_with, StatusOptions, MANIFEST_FILE_NAME};
use serde::Serialize;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Ancestors of the executable inspected for a drive manifest.
const MAX_ANCESTORS: usize = 16;

/// Where the session's drive came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveSource {
    /// Chosen in this session with `select_drive`.
    Selected,
    /// The `AIDEX_DRIVE_ROOT` environment variable.
    Environment,
    /// The drive the AiDEX executable runs from.
    Executable,
}

/// A usable drive root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedDrive {
    /// Canonical root (on Windows with the `\\?\` prefix).
    pub root: PathBuf,
    pub source: DriveSource,
}

/// Outcome of drive resolution.
#[derive(Clone, Debug, PartialEq)]
pub enum DriveResolution {
    Resolved(ResolvedDrive),
    /// A configured root (environment variable) that cannot be used.
    Invalid {
        source: DriveSource,
        path: PathBuf,
        error: BridgeError,
    },
    None,
}

/// Resolve the session drive from its inputs. Pure apart from filesystem reads.
pub fn resolve(
    selected: Option<&Path>,
    environment: Option<OsString>,
    executable: Option<&Path>,
) -> DriveResolution {
    if let Some(selected) = selected {
        return DriveResolution::Resolved(ResolvedDrive {
            root: selected.to_path_buf(),
            source: DriveSource::Selected,
        });
    }
    if let Some(value) = environment.filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        return match resolve_root(Some(&path)) {
            Ok(root) => DriveResolution::Resolved(ResolvedDrive {
                root,
                source: DriveSource::Environment,
            }),
            Err(error) => DriveResolution::Invalid {
                source: DriveSource::Environment,
                path,
                error: error.into(),
            },
        };
    }
    match executable.and_then(drive_containing) {
        Some(root) => DriveResolution::Resolved(ResolvedDrive {
            root,
            source: DriveSource::Executable,
        }),
        None => DriveResolution::None,
    }
}

/// The nearest ancestor directory of `executable` that holds a regular
/// `aidex-drive.json`, canonicalized.
pub fn drive_containing(executable: &Path) -> Option<PathBuf> {
    executable
        .ancestors()
        .skip(1)
        .take(MAX_ANCESTORS)
        .find(|directory| {
            fs::symlink_metadata(directory.join(MANIFEST_FILE_NAME))
                .is_ok_and(|metadata| metadata.is_file())
        })
        .and_then(|directory| resolve_root(Some(directory)).ok())
}

/// Validate a user-chosen folder: it must exist and hold a supported AiDEX manifest
/// (schema v1 is accepted so the UI can offer migration). Removable media is not
/// required, so developer folders work too.
pub fn validate_selection(path: &Path) -> Result<PathBuf, BridgeError> {
    let status = status_with(
        path,
        StatusOptions {
            require_removable: false,
        },
    )?;
    Ok(status.root)
}

/// The session's in-memory drive choice.
#[derive(Default)]
pub struct DriveSession {
    selected: Mutex<Option<PathBuf>>,
}

impl DriveSession {
    pub fn selected(&self) -> Option<PathBuf> {
        self.selected
            .lock()
            .ok()
            .and_then(|selected| selected.clone())
    }

    pub fn select(&self, root: PathBuf) {
        if let Ok(mut selected) = self.selected.lock() {
            *selected = Some(root);
        }
    }

    /// Resolve with this process's environment and executable.
    pub fn resolve(&self) -> DriveResolution {
        let executable = std::env::current_exe().ok();
        resolve(
            self.selected().as_deref(),
            std::env::var_os("AIDEX_DRIVE_ROOT"),
            executable.as_deref(),
        )
    }

    /// The resolved root, or the error the UI should show.
    pub fn require(&self) -> Result<ResolvedDrive, BridgeError> {
        match self.resolve() {
            DriveResolution::Resolved(drive) => Ok(drive),
            DriveResolution::Invalid { error, .. } => Err(error),
            DriveResolution::None => Err(BridgeError::no_drive()),
        }
    }
}

/// A path for display: Windows verbatim disk prefixes (`\\?\H:\`) are removed.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => rest.to_owned(),
        _ => text.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn developer_drive() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        aidex_drive::init_with(
            root.path(),
            aidex_drive::InitOptions {
                require_removable: false,
            },
        )
        .unwrap();
        root
    }

    #[test]
    fn a_session_selection_wins_over_the_environment_and_the_executable() {
        let drive = developer_drive();
        let other = developer_drive();
        let resolution = resolve(
            Some(drive.path()),
            Some(other.path().as_os_str().to_owned()),
            None,
        );
        assert_eq!(
            resolution,
            DriveResolution::Resolved(ResolvedDrive {
                root: drive.path().to_path_buf(),
                source: DriveSource::Selected,
            })
        );
    }

    #[test]
    fn the_environment_variable_is_canonicalized_or_reported_invalid() {
        let drive = developer_drive();
        let resolution = resolve(None, Some(drive.path().as_os_str().to_owned()), None);
        let DriveResolution::Resolved(resolved) = resolution else {
            panic!("expected a resolved drive, got {resolution:?}");
        };
        assert_eq!(resolved.source, DriveSource::Environment);
        assert_eq!(resolved.root, fs::canonicalize(drive.path()).unwrap());

        let missing = drive.path().join("missing");
        match resolve(None, Some(missing.as_os_str().to_owned()), None) {
            DriveResolution::Invalid {
                source,
                path,
                error,
            } => {
                assert_eq!(source, DriveSource::Environment);
                assert_eq!(path, missing);
                assert_eq!(error.kind, "invalid_root");
            }
            other => panic!("expected invalid, got {other:?}"),
        }
        assert_eq!(
            resolve(None, Some(OsString::new()), None),
            DriveResolution::None
        );
    }

    #[test]
    fn the_portable_layout_finds_the_drive_above_the_executable() {
        let drive = developer_drive();
        let bin = drive.path().join("runtime").join("windows-x64");
        fs::create_dir_all(&bin).unwrap();
        let executable = bin.join("aidex-desktop.exe");
        fs::write(&executable, b"").unwrap();

        let resolution = resolve(None, None, Some(&executable));
        assert_eq!(
            resolution,
            DriveResolution::Resolved(ResolvedDrive {
                root: fs::canonicalize(drive.path()).unwrap(),
                source: DriveSource::Executable,
            })
        );
    }

    #[test]
    fn no_manifest_above_the_executable_resolves_to_none() {
        let plain = tempfile::tempdir().unwrap();
        let executable = plain.path().join("bin").join("aidex-desktop.exe");
        assert_eq!(
            resolve(None, None, Some(&executable)),
            DriveResolution::None
        );
        assert_eq!(resolve(None, None, None), DriveResolution::None);

        // A directory named like the manifest is not a manifest.
        fs::create_dir(plain.path().join(MANIFEST_FILE_NAME)).unwrap();
        assert_eq!(drive_containing(&executable), None);
    }

    #[test]
    fn selections_must_hold_a_supported_manifest_and_are_never_written() {
        let drive = developer_drive();
        assert_eq!(
            validate_selection(drive.path()).unwrap(),
            fs::canonicalize(drive.path()).unwrap()
        );

        let empty = tempfile::tempdir().unwrap();
        let error = validate_selection(empty.path()).unwrap_err();
        assert_eq!(error.kind, "missing_manifest");
        assert_eq!(fs::read_dir(empty.path()).unwrap().count(), 0);

        let session = DriveSession::default();
        assert_eq!(session.selected(), None);
        session.select(drive.path().to_path_buf());
        assert_eq!(session.selected().as_deref(), Some(drive.path()));
        assert_eq!(session.require().unwrap().source, DriveSource::Selected);
    }

    #[test]
    fn display_paths_drop_the_verbatim_disk_prefix_only() {
        assert_eq!(display_path(Path::new(r"\\?\H:\")), r"H:\");
        assert_eq!(
            display_path(Path::new(r"\\?\C:\drives\dev")),
            r"C:\drives\dev"
        );
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share")),
            r"\\?\UNC\server\share"
        );
        assert_eq!(display_path(Path::new("/media/aidex")), "/media/aidex");
    }
}
