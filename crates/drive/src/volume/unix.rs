//! Linux and macOS volume discovery (best effort).
//!
//! Linux: mounted block devices from `/proc/mounts`; sysfs `removable` and the device
//! path (a `/usb` segment marks a USB-attached disk); labels from `/dev/disk/by-label`;
//! capacity from `df -Pk` over the listed mount points only (never remote mounts).
//! macOS: `df -Pk` for `/` and `/Volumes/*`, classified by `diskutil info`.
//! Anything that cannot be determined is reported as unknown, never guessed.

use std::path::PathBuf;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::{path::Path, process::Command};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use super::RawVolume;
use super::RemovableStatus;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::DriveError;

/// One `/proc/mounts` entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MountEntry {
    pub(super) device: String,
    pub(super) mount_point: PathBuf,
    pub(super) filesystem: String,
}

/// One `df -Pk` row, with sizes converted to bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DfRow {
    pub(super) device: String,
    pub(super) total_bytes: u64,
    pub(super) free_bytes: u64,
    pub(super) mount_point: PathBuf,
}

/// Fields of `diskutil info` this module reads.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct DiskutilInfo {
    pub(super) volume_name: Option<String>,
    pub(super) filesystem: Option<String>,
    pub(super) removable_media: Option<String>,
    pub(super) protocol: Option<String>,
}

/// Parse `/proc/mounts`, decoding the kernel's octal escapes (`\040` for a space).
pub(super) fn parse_proc_mounts(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let device = unescape_octal(fields.next()?);
            let mount_point = PathBuf::from(unescape_octal(fields.next()?));
            let filesystem = fields.next()?.to_owned();
            Some(MountEntry {
                device,
                mount_point,
                filesystem,
            })
        })
        .collect()
}

/// Whether a mount entry is a real block device worth listing (loop devices,
/// pseudo filesystems, and network mounts are not).
pub(super) fn is_listed_block_device(device: &str) -> bool {
    device.starts_with("/dev/")
        && !device.starts_with("/dev/loop")
        && !device.starts_with("/dev/ram")
}

/// Parse POSIX `df -Pk` output. The mount point is the remainder of each line, so
/// mount points containing spaces survive (runs of spaces collapse to one).
pub(super) fn parse_df(text: &str) -> Vec<DfRow> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 6 {
                return None;
            }
            let total = fields[1].parse::<u64>().ok()?;
            let free = fields[3].parse::<u64>().ok()?;
            Some(DfRow {
                device: fields[0].to_owned(),
                total_bytes: total.saturating_mul(1024),
                free_bytes: free.saturating_mul(1024),
                mount_point: PathBuf::from(fields[5..].join(" ")),
            })
        })
        .collect()
}

/// Classify a Linux disk from its sysfs device path and `removable` flag.
pub(super) fn classify_sysfs(device_path: &str, removable_flag: Option<bool>) -> RemovableStatus {
    match removable_flag {
        Some(true) => RemovableStatus::Removable,
        _ if device_path.contains("/usb") => RemovableStatus::UsbFixed,
        Some(false) => RemovableStatus::Fixed,
        None => RemovableStatus::Unknown,
    }
}

/// Parse the `Key: value` lines of `diskutil info <mount>`.
pub(super) fn parse_diskutil(text: &str) -> DiskutilInfo {
    let mut info = DiskutilInfo::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let slot = match key.trim() {
            "Volume Name" => &mut info.volume_name,
            "File System Personality" => &mut info.filesystem,
            "Removable Media" => &mut info.removable_media,
            "Protocol" => &mut info.protocol,
            _ => continue,
        };
        *slot = Some(value.to_owned());
    }
    info
}

/// Classify a macOS volume from `diskutil info`: removable media, then USB protocol.
pub(super) fn classify_diskutil(info: &DiskutilInfo) -> RemovableStatus {
    let media = info.removable_media.as_deref().map(str::to_ascii_lowercase);
    let usb = info
        .protocol
        .as_deref()
        .is_some_and(|protocol| protocol.eq_ignore_ascii_case("usb"));
    match media.as_deref() {
        Some("removable") => RemovableStatus::Removable,
        _ if usb => RemovableStatus::UsbFixed,
        Some("fixed") => RemovableStatus::Fixed,
        _ => RemovableStatus::Unknown,
    }
}

/// Decode `\NNN` octal escapes used by `/proc/mounts` and `/dev/disk/by-label`
/// (`\x20` hex escapes are decoded too).
pub(super) fn unescape_octal(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            let octal = bytes
                .get(index + 1..index + 4)
                .filter(|digits| digits.iter().all(|digit| (b'0'..=b'7').contains(digit)))
                .and_then(|digits| u8::from_str_radix(std::str::from_utf8(digits).ok()?, 8).ok());
            if let Some(byte) = octal {
                decoded.push(byte);
                index += 4;
                continue;
            }
            let hex = bytes
                .get(index + 1..index + 4)
                .filter(|digits| digits[0] == b'x')
                .and_then(|digits| {
                    u8::from_str_radix(std::str::from_utf8(&digits[1..]).ok()?, 16).ok()
                });
            if let Some(byte) = hex {
                decoded.push(byte);
                index += 4;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(target_os = "linux")]
fn df_rows(mount_points: &[&Path]) -> Vec<DfRow> {
    if mount_points.is_empty() {
        return Vec::new();
    }
    Command::new("df")
        .arg("-Pk")
        .arg("--")
        .args(mount_points)
        .output()
        .ok()
        .map(|output| parse_df(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
pub(super) fn linux_volumes() -> Result<Vec<RawVolume>, DriveError> {
    use std::{collections::HashSet, fs};

    let mounts = fs::read_to_string("/proc/mounts")
        .map_err(|error| DriveError::Discovery(format!("cannot read /proc/mounts: {error}")))?;
    let mut seen = HashSet::new();
    // The last entry for a mount point is the visible one.
    let mut entries: Vec<MountEntry> = parse_proc_mounts(&mounts)
        .into_iter()
        .rev()
        .filter(|entry| is_listed_block_device(&entry.device))
        .filter(|entry| seen.insert(entry.mount_point.clone()))
        .collect();
    entries.reverse();
    let points: Vec<&Path> = entries
        .iter()
        .map(|entry| entry.mount_point.as_path())
        .collect();
    let sizes = df_rows(&points);
    let labels = linux_labels();
    Ok(entries
        .iter()
        .map(|entry| {
            let device = fs::canonicalize(&entry.device).ok();
            let size = sizes
                .iter()
                .find(|row| row.mount_point == entry.mount_point);
            RawVolume {
                root: entry.mount_point.clone(),
                label: device
                    .as_ref()
                    .and_then(|device| labels.iter().find(|(path, _)| path == device))
                    .map(|(_, label)| label.clone()),
                filesystem: Some(entry.filesystem.clone()),
                total_bytes: size.map(|row| row.total_bytes).filter(|bytes| *bytes > 0),
                free_bytes: size.map(|row| row.free_bytes),
                removable: device
                    .as_deref()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    .map_or(RemovableStatus::Unknown, linux_disk_status),
                system: entry.mount_point == Path::new("/"),
            }
        })
        .collect())
}

#[cfg(target_os = "linux")]
fn linux_disk_status(block_name: &str) -> RemovableStatus {
    use std::fs;

    let Ok(device) = fs::canonicalize(Path::new("/sys/class/block").join(block_name)) else {
        return RemovableStatus::Unknown;
    };
    // A partition's sysfs directory sits inside its disk's directory.
    let disk = if device.join("partition").is_file() {
        device.parent().map(Path::to_path_buf).unwrap_or(device)
    } else {
        device
    };
    let removable = fs::read_to_string(disk.join("removable"))
        .ok()
        .map(|value| value.trim() == "1");
    classify_sysfs(&disk.to_string_lossy(), removable)
}

#[cfg(target_os = "linux")]
fn linux_labels() -> Vec<(PathBuf, String)> {
    use std::fs;

    let Ok(entries) = fs::read_dir("/dev/disk/by-label") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let target = fs::canonicalize(entry.path()).ok()?;
            let label = unescape_octal(entry.file_name().to_str()?);
            Some((target, label))
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub(super) fn macos_volumes() -> Result<Vec<RawVolume>, DriveError> {
    let output = Command::new("df")
        .args(["-Pkl"])
        .output()
        .map_err(|error| DriveError::Discovery(format!("cannot list volumes: {error}")))?;
    let rows: Vec<DfRow> = parse_df(&String::from_utf8_lossy(&output.stdout))
        .into_iter()
        .filter(|row| row.device.starts_with("/dev/disk"))
        .filter(|row| row.mount_point == Path::new("/") || row.mount_point.starts_with("/Volumes"))
        .collect();
    Ok(rows
        .into_iter()
        .map(|row| {
            let info = Command::new("diskutil")
                .arg("info")
                .arg(&row.mount_point)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| parse_diskutil(&String::from_utf8_lossy(&output.stdout)))
                .unwrap_or_default();
            RawVolume {
                system: row.mount_point == Path::new("/"),
                root: row.mount_point,
                label: info.volume_name.clone(),
                filesystem: info.filesystem.clone(),
                total_bytes: Some(row.total_bytes).filter(|bytes| *bytes > 0),
                free_bytes: Some(row.free_bytes),
                removable: classify_diskutil(&info),
            }
        })
        .collect())
}
