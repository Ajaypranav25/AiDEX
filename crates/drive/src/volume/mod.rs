//! Mounted-volume discovery: capacity, filesystem, how a volume is attached, and
//! whether its root already holds an AiDEX manifest.
//!
//! Discovery is read-only. On Windows it runs one PowerShell query (the drive letter,
//! when filtered, is passed through an environment variable, never interpolated into
//! the script); on Linux it reads `/proc/mounts`, sysfs, and `df`; on macOS it reads
//! `df` and `diskutil info`. Parsers are platform-independent so they are tested on
//! every host.

// Parsers for other platforms are compiled everywhere so their tests run on every host.
#[cfg_attr(not(test), allow(dead_code))]
mod unix;
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
mod windows;

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{DriveError, MANIFEST_FILE_NAME, ops::read_stored_manifest};

/// How a volume is attached, as far as the platform reports it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovableStatus {
    /// The platform reports removable media (USB flash drives, SD cards).
    Removable,
    /// A fixed disk attached over USB, such as a portable SSD (Samsung T7 and
    /// similar enclosures report themselves as fixed disks).
    UsbFixed,
    /// An internal or otherwise non-USB fixed disk (SATA, NVMe, virtual, RAID).
    Fixed,
    /// The platform did not report enough to classify the volume.
    Unknown,
}

impl RemovableStatus {
    /// Whether the volume is portable media AiDEX may initialize: removable media or
    /// a USB-attached fixed disk. Internal and unclassified disks are refused.
    pub fn is_portable(self) -> bool {
        matches!(self, Self::Removable | Self::UsbFixed)
    }

    /// Stable lowercase identifier (`removable`, `usb_fixed`, `fixed`, `unknown`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Removable => "removable",
            Self::UsbFixed => "usb_fixed",
            Self::Fixed => "fixed",
            Self::Unknown => "unknown",
        }
    }
}

/// One mounted volume as reported by the platform.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VolumeInfo {
    /// Volume root (`H:\` on Windows, the mount point elsewhere).
    pub root: PathBuf,
    /// Volume label, when one is set.
    pub label: Option<String>,
    /// Filesystem name as reported by the platform (`NTFS`, `exFAT`, `ext4`, ...).
    pub filesystem: Option<String>,
    /// Capacity in bytes; `None` when unreported (for example a card reader without media).
    pub total_bytes: Option<u64>,
    /// Available bytes; `None` when unreported.
    pub free_bytes: Option<u64>,
    /// How the volume is attached.
    pub removable: RemovableStatus,
    /// The volume holds the running operating system (`%SystemDrive%` on Windows,
    /// `/` elsewhere). It is never eligible for initialization.
    pub system: bool,
    /// A regular `aidex-drive.json` file exists at the volume root.
    pub has_aidex_manifest: bool,
    /// Schema version of that manifest when it parses as a supported schema; `None`
    /// when there is no manifest or it is malformed or unsupported.
    pub schema_version: Option<u32>,
}

impl VolumeInfo {
    /// Whether [`crate::init`] accepts this volume: portable media that is not the
    /// system volume.
    pub fn is_eligible(&self) -> bool {
        self.removable.is_portable() && !self.system
    }
}

/// Facts a platform query reports before the manifest probe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RawVolume {
    pub(crate) root: PathBuf,
    pub(crate) label: Option<String>,
    pub(crate) filesystem: Option<String>,
    pub(crate) total_bytes: Option<u64>,
    pub(crate) free_bytes: Option<u64>,
    pub(crate) removable: RemovableStatus,
    pub(crate) system: bool,
}

impl RawVolume {
    fn into_info(self) -> VolumeInfo {
        // A volume without reported capacity (no media) is not touched at all.
        let (has_aidex_manifest, schema_version) = if self.total_bytes.is_some() {
            probe_manifest(&self.root)
        } else {
            (false, None)
        };
        VolumeInfo {
            root: self.root,
            label: self.label,
            filesystem: self.filesystem,
            total_bytes: self.total_bytes,
            free_bytes: self.free_bytes,
            removable: self.removable,
            system: self.system,
            has_aidex_manifest,
            schema_version,
        }
    }
}

/// List mounted volumes with their capacity, attachment, and AiDEX manifest state.
///
/// Windows lists lettered fixed and removable volumes (optical and network drives are
/// skipped). Linux lists mounted block devices (`/dev/*`, loop devices skipped).
/// macOS lists `/` and volumes under `/Volumes`. Never modifies anything.
pub fn list_volumes() -> Result<Vec<VolumeInfo>, DriveError> {
    Ok(platform_volumes(None)?
        .into_iter()
        .map(RawVolume::into_info)
        .collect())
}

/// The mounted volume that holds `path` (which must exist).
pub fn volume_for(path: impl AsRef<Path>) -> Result<VolumeInfo, DriveError> {
    let path = path.as_ref();
    raw_volume_for(path)?
        .map(RawVolume::into_info)
        .ok_or_else(|| {
            DriveError::Discovery(format!(
                "cannot identify the volume of `{}`",
                path.display()
            ))
        })
}

/// Platform facts for the volume holding `path`, without the manifest probe.
/// `Ok(None)` when no listed volume holds it; `Err` when the query itself failed.
pub(crate) fn raw_volume_for(path: &Path) -> Result<Option<RawVolume>, DriveError> {
    let Ok(canonical) = fs::canonicalize(path) else {
        return Ok(None);
    };
    #[cfg(windows)]
    {
        let Some(letter) = crate::platform::volume_letter(&canonical) else {
            return Ok(None);
        };
        Ok(platform_volumes(Some(letter))?.into_iter().next())
    }
    #[cfg(not(windows))]
    {
        Ok(platform_volumes(None)?
            .into_iter()
            .filter(|volume| canonical.starts_with(&volume.root))
            .max_by_key(|volume| volume.root.components().count()))
    }
}

fn platform_volumes(letter: Option<char>) -> Result<Vec<RawVolume>, DriveError> {
    #[cfg(windows)]
    {
        windows::query(letter)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = letter;
        unix::linux_volumes()
    }
    #[cfg(target_os = "macos")]
    {
        let _ = letter;
        unix::macos_volumes()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = letter;
        Err(DriveError::Discovery(
            "volume discovery is unsupported on this platform".into(),
        ))
    }
}

/// `(has_aidex_manifest, schema_version)` for a volume root; reads only the manifest.
fn probe_manifest(root: &Path) -> (bool, Option<u32>) {
    let path = root.join(MANIFEST_FILE_NAME);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => match read_stored_manifest(&path) {
            Ok((manifest, _)) => (true, Some(manifest.schema_version())),
            Err(_) => (true, None),
        },
        // A directory or link named like the manifest is not an AiDEX manifest.
        Ok(_) => (true, None),
        Err(_) => (false, None),
    }
}

#[cfg(test)]
mod tests;
