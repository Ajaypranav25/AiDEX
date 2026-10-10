//! Platform queries: the removable-media gate, free space, and drive-letter roots.

use std::{path::Path, process::Command};

use crate::DriveError;

/// Free bytes on the volume holding `root` (any directory on a lettered volume on Windows).
pub(crate) fn available_bytes(root: &Path) -> Result<u64, DriveError> {
    #[cfg(windows)]
    let output = {
        let letter = volume_letter(root).ok_or_else(|| {
            DriveError::Discovery(format!(
                "cannot determine the drive letter of `{}`",
                root.display()
            ))
        })?;
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-Volume -DriveLetter $env:AIDEX_DRIVE_LETTER -ErrorAction Stop).SizeRemaining",
            ])
            .env("AIDEX_DRIVE_LETTER", letter.to_string())
            .output()
    };
    #[cfg(not(windows))]
    let output = Command::new("df").args(["-Pk"]).arg(root).output();
    let output = output
        .map_err(|error| DriveError::Discovery(format!("cannot query free space: {error}")))?;
    if !output.status.success() {
        return Err(DriveError::Discovery(
            "cannot determine free drive space".into(),
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    #[cfg(windows)]
    let bytes = text.trim().parse::<u64>().ok();
    #[cfg(not(windows))]
    let bytes = text
        .lines()
        .last()
        .and_then(|line| line.split_whitespace().nth(3))
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024));
    bytes.ok_or_else(|| DriveError::Discovery("cannot parse free drive space".into()))
}

/// Refuse roots that are not portable media: the volume must be removable media or a
/// USB-attached fixed disk (portable SSDs report themselves as fixed), and never the
/// system volume. Internal SATA/NVMe disks and unclassified volumes are refused.
pub(crate) fn require_removable(root: &Path) -> Result<(), DriveError> {
    #[cfg(windows)]
    if !is_windows_drive_root(root) {
        return Err(DriveError::InvalidRoot(format!(
            "{} is not a drive-letter root",
            root.display()
        )));
    }
    let volume = crate::volume::raw_volume_for(root).map_err(|error| {
        DriveError::Discovery(format!("cannot identify removable drive: {error}"))
    })?;
    match volume {
        Some(volume) if volume.removable.is_portable() && !volume.system => Ok(()),
        _ => Err(DriveError::NotRemovable(root.display().to_string())),
    }
}

/// Recognize `X:\\` or `X:/` roots, including canonical Windows extended paths.
pub fn is_windows_drive_root(path: &Path) -> bool {
    windows_drive_letter(path).is_some()
}

fn windows_drive_letter(path: &Path) -> Option<char> {
    let value = path.to_string_lossy();
    let value = value.strip_prefix(r"\\?\").unwrap_or(&value);
    let bytes = value.as_bytes();
    (bytes.len() == 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
    .then(|| char::from(bytes[0]))
}

/// The drive letter of any path on a lettered volume (`C:\x`, `\\?\C:\x`).
#[cfg(windows)]
pub(crate) fn volume_letter(path: &Path) -> Option<char> {
    use std::path::{Component, Prefix};
    match path.components().next()? {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => Some(char::from(letter)),
            _ => None,
        },
        _ => None,
    }
}
