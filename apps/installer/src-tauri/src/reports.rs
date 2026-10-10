//! Drive inspection and initialization for the setup wizard, without Tauri types so
//! they are unit-tested directly. TypeScript mirrors: `packages/client/src/dto/installer.ts`.
//!
//! Inspection only reads. Initialization calls `aidex_drive::init`, which creates the
//! manifest-declared directories and the manifest at the drive root and never formats,
//! repartitions, or deletes anything.

use crate::error::SetupError;
use aidex_core::ServiceErrorCode;
use aidex_drive::{
    init_with, resolve_root, status_with, volume_for, DrivePaths, DriveStatus, InitOptions,
    RemovableStatus, StatusOptions, VolumeInfo, CURRENT_SCHEMA_VERSION,
};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// `inspect_drive`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DriveInspection {
    pub root: String,
    pub volume: VolumeInfo,
    pub eligible: bool,
    pub ineligible_reason: Option<String>,
    pub health: Option<DriveHealth>,
    pub health_error: Option<SetupError>,
}

/// `init_drive`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InitReport {
    pub root: String,
    pub previous_schema_version: Option<u32>,
    pub health: DriveHealth,
    pub directories: Vec<String>,
}

/// `aidex_drive::DriveStatus` in UI terms (same shape as the desktop's).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DriveHealth {
    pub writable: bool,
    pub free_bytes: Option<u64>,
    pub schema_version: Option<u32>,
    pub current_schema_version: u32,
    pub migration_required: bool,
    pub migration_advice: Option<String>,
}

impl From<&DriveStatus> for DriveHealth {
    fn from(status: &DriveStatus) -> Self {
        Self {
            writable: status.writable,
            free_bytes: status.free_bytes,
            schema_version: status.schema_version,
            current_schema_version: CURRENT_SCHEMA_VERSION,
            migration_required: status.migration_required(),
            migration_advice: status.migration_advice(),
        }
    }
}

/// Inspect `root` without changing anything.
pub fn inspect(root: &Path) -> Result<DriveInspection, SetupError> {
    let root = resolve_root(Some(root))?;
    let volume = volume_for(&root)?;
    let ineligible_reason = ineligible_reason(&root, &volume);
    let (health, health_error) = if volume.has_aidex_manifest {
        match status_with(
            &root,
            StatusOptions {
                require_removable: false,
            },
        ) {
            Ok(status) => (Some(DriveHealth::from(&status)), None),
            Err(error) => (None, Some(SetupError::from(error))),
        }
    } else {
        (None, None)
    };
    Ok(DriveInspection {
        root: display_path(&root),
        eligible: ineligible_reason.is_none(),
        ineligible_reason,
        volume,
        health,
        health_error,
    })
}

/// Why AiDEX will not initialize `root` on `volume`, or `None` when it will.
pub fn ineligible_reason(root: &Path, volume: &VolumeInfo) -> Option<String> {
    if volume.system {
        return Some("This is the system drive that holds the operating system.".into());
    }
    match volume.removable {
        RemovableStatus::Fixed => {
            return Some(
                "This is an internal disk. AiDEX installs only on removable or USB drives.".into(),
            );
        }
        RemovableStatus::Unknown => {
            return Some(
                "The drive type could not be determined, so AiDEX will not use it.".into(),
            );
        }
        RemovableStatus::Removable | RemovableStatus::UsbFixed => {}
    }
    if !same_path(root, &volume.root) {
        return Some(format!(
            "AiDEX installs at the root of a drive; choose {} instead.",
            display_path(&volume.root)
        ));
    }
    None
}

/// Initialize `root` (removable media required) and report the result.
pub fn initialize(root: &Path) -> Result<InitReport, SetupError> {
    initialize_with(root, InitOptions::default())
}

/// [`initialize`] with explicit options (tests pass `require_removable: false`).
pub fn initialize_with(root: &Path, options: InitOptions) -> Result<InitReport, SetupError> {
    let root = resolve_root(Some(root))?;
    if options.require_removable {
        let volume = volume_for(&root)?;
        if let Some(reason) = ineligible_reason(&root, &volume) {
            return Err(SetupError::new(
                ServiceErrorCode::InvalidArgument,
                "not_eligible",
                reason,
            ));
        }
    }
    // Missing, malformed, or unsupported manifests have no previous schema; init reports
    // the malformed and unsupported ones without rewriting them.
    let previous_schema_version = status_with(
        &root,
        StatusOptions {
            require_removable: false,
        },
    )
    .ok()
    .and_then(|status| status.schema_version);
    let status = init_with(&root, options)?;
    let directories = DrivePaths::open(&root)?
        .layout()
        .entries()
        .map(|(_, relative)| relative.as_str().to_owned())
        .collect();
    Ok(InitReport {
        root: display_path(&status.root),
        previous_schema_version,
        health: DriveHealth::from(&status),
        directories,
    })
}

/// A path for display: Windows verbatim disk prefixes (`\\?\H:\`) are removed.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => rest.to_owned(),
        _ => text.into_owned(),
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| -> PathBuf {
        let text = display_path(path);
        PathBuf::from(if cfg!(windows) {
            text.to_ascii_lowercase()
        } else {
            text
        })
    };
    normalize(left) == normalize(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn volume(root: &str, removable: RemovableStatus, system: bool) -> VolumeInfo {
        VolumeInfo {
            root: root.into(),
            label: None,
            filesystem: Some("exFAT".into()),
            total_bytes: Some(64 << 30),
            free_bytes: Some(60 << 30),
            removable,
            system,
            has_aidex_manifest: false,
            schema_version: None,
        }
    }

    #[test]
    fn only_portable_non_system_volume_roots_are_eligible() {
        let root = if cfg!(windows) { r"H:\" } else { "/media/usb" };
        let usb = volume(root, RemovableStatus::UsbFixed, false);
        assert_eq!(ineligible_reason(Path::new(root), &usb), None);
        let stick = volume(root, RemovableStatus::Removable, false);
        assert_eq!(ineligible_reason(Path::new(root), &stick), None);

        let system = volume(root, RemovableStatus::UsbFixed, true);
        assert!(ineligible_reason(Path::new(root), &system)
            .unwrap()
            .contains("system drive"));
        let internal = volume(root, RemovableStatus::Fixed, false);
        assert!(ineligible_reason(Path::new(root), &internal)
            .unwrap()
            .contains("internal disk"));
        let unknown = volume(root, RemovableStatus::Unknown, false);
        assert!(ineligible_reason(Path::new(root), &unknown).is_some());

        let nested = Path::new(root).join("AiDEX");
        assert!(ineligible_reason(&nested, &stick)
            .unwrap()
            .contains("root of a drive"));
    }

    #[test]
    fn inspecting_a_temporary_folder_is_read_only_and_not_eligible() {
        let folder = tempfile::tempdir().unwrap();
        let inspection = inspect(folder.path()).unwrap();
        assert!(!inspection.eligible);
        assert!(inspection.ineligible_reason.is_some());
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
    }

    #[test]
    fn initialization_refuses_ineligible_targets_without_writing() {
        let folder = tempfile::tempdir().unwrap();
        let error = initialize(folder.path()).unwrap_err();
        assert_eq!(error.code, "invalid_argument");
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
    }

    #[test]
    fn initialization_reports_the_layout_and_previous_schema() {
        let folder = tempfile::tempdir().unwrap();
        let options = InitOptions {
            require_removable: false,
        };
        let report = initialize_with(folder.path(), options).unwrap();
        assert_eq!(report.previous_schema_version, None);
        assert_eq!(report.health.schema_version, Some(CURRENT_SCHEMA_VERSION));
        assert!(report.health.writable);
        assert!(!report.health.migration_required);
        assert!(report.directories.contains(&"runtime".to_owned()));
        assert!(report.directories.contains(&"private/vault".to_owned()));
        for directory in &report.directories {
            assert!(folder.path().join(directory).is_dir(), "{directory}");
        }

        let again = initialize_with(folder.path(), options).unwrap();
        assert_eq!(again.previous_schema_version, Some(CURRENT_SCHEMA_VERSION));
        assert_eq!(again.directories, report.directories);
    }

    #[test]
    fn display_paths_drop_the_verbatim_disk_prefix_only() {
        assert_eq!(display_path(Path::new(r"\\?\H:\")), r"H:\");
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share")),
            r"\\?\UNC\server\share"
        );
    }
}
