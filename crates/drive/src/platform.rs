//! Platform queries: removable-media detection, free space, and drive-letter roots.

#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::{path::Path, process::Command};

#[cfg(target_os = "linux")]
use std::fs;

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

pub(crate) fn require_removable(root: &Path) -> Result<(), DriveError> {
    #[cfg(target_os = "linux")]
    {
        let canonical =
            fs::canonicalize(root).map_err(|error| DriveError::Discovery(error.to_string()))?;
        let mounts = fs::read_to_string("/proc/mounts").map_err(|error| {
            DriveError::Discovery(format!("cannot identify removable filesystem: {error}"))
        })?;
        let mount = mounts
            .lines()
            .filter_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                let point = fields.get(1)?.replace("\\040", " ");
                let point_path = PathBuf::from(point);
                canonical
                    .starts_with(&point_path)
                    .then(|| (fields[0], point_path))
            })
            .max_by_key(|(_, point)| point.components().count());
        let (device, _) =
            mount.ok_or_else(|| DriveError::NotRemovable(root.display().to_string()))?;
        let block = device
            .strip_prefix("/dev/")
            .ok_or_else(|| DriveError::NotRemovable(root.display().to_string()))?;
        let block = block.trim_end_matches(|character: char| character.is_ascii_digit());
        if fs::read_to_string(PathBuf::from("/sys/block").join(block).join("removable"))
            .ok()
            .is_some_and(|value| value.trim() == "1")
        {
            return Ok(());
        }
        Err(DriveError::NotRemovable(root.display().to_string()))
    }
    #[cfg(windows)]
    {
        if !is_windows_drive_root(root) {
            return Err(DriveError::InvalidRoot(format!(
                "{} is not a drive-letter root",
                root.display()
            )));
        }
        let letter = windows_drive_letter(root).unwrap_or_default().to_string();
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance Win32_LogicalDisk -Filter (\"DeviceID='\" + $env:AIDEX_DRIVE_LETTER + \":'\")).DriveType",
            ])
            .env("AIDEX_DRIVE_LETTER", letter)
            .output()
            .map_err(|error| {
                DriveError::Discovery(format!("cannot identify removable drive: {error}"))
            })?;
        if output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "2" {
            return Ok(());
        }
        Err(DriveError::NotRemovable(root.display().to_string()))
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("diskutil")
            .args(["info", &root.to_string_lossy()])
            .output()
            .map_err(|error| {
                DriveError::Discovery(format!("cannot identify removable drive: {error}"))
            })?;
        let info = String::from_utf8_lossy(&output.stdout);
        if output.status.success()
            && info
                .lines()
                .any(|line| line.contains("Removable Media:") && line.contains("Removable"))
        {
            return Ok(());
        }
        Err(DriveError::NotRemovable(root.display().to_string()))
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = root;
        Err(DriveError::Discovery(
            "removable-drive detection is unsupported on this platform".into(),
        ))
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
