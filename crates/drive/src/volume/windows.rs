//! Windows volume query: one PowerShell run over `Get-Volume`, joined with the bus
//! type of each volume's disk (`Get-Partition | Get-Disk`).
//!
//! `DriveType` 2 (removable) is removable media. Portable SSDs report `DriveType` 3
//! (fixed); a fixed volume whose disk sits on the USB bus is classified
//! [`RemovableStatus::UsbFixed`]. Internal SATA/NVMe/RAID/virtual disks stay `Fixed`.

use std::path::PathBuf;

use serde::Deserialize;

use super::{RawVolume, RemovableStatus};
use crate::DriveError;

/// Lists lettered volumes, optionally only `$env:AIDEX_DRIVE_LETTER`. The letter is
/// validated by the caller and read from the environment, never spliced into the script.
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) const SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
$filter = $env:AIDEX_DRIVE_LETTER
$rows = foreach ($volume in @(Get-Volume | Where-Object { $_.DriveLetter -and (-not $filter -or [string]$_.DriveLetter -eq $filter) })) {
  $bus = ''
  try { $bus = [string](Get-Partition -DriveLetter $volume.DriveLetter -ErrorAction Stop | Get-Disk -ErrorAction Stop).BusType } catch { $bus = '' }
  [pscustomobject]@{
    letter = [string]$volume.DriveLetter
    label = [string]$volume.FileSystemLabel
    filesystem = [string]$volume.FileSystem
    drive_type = [string]$volume.DriveType
    bus_type = $bus
    size = [uint64]$volume.Size
    free = [uint64]$volume.SizeRemaining
  }
}
ConvertTo-Json -InputObject @($rows) -Compress -Depth 2"#;

/// One row of [`SCRIPT`] output.
#[derive(Debug, Deserialize)]
pub(super) struct Row {
    letter: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    filesystem: Option<String>,
    #[serde(default)]
    drive_type: Option<String>,
    #[serde(default)]
    bus_type: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    free: Option<u64>,
}

#[cfg(windows)]
pub(super) fn query(letter: Option<char>) -> Result<Vec<RawVolume>, DriveError> {
    use std::process::Command;

    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    match letter {
        Some(letter) if letter.is_ascii_alphabetic() => {
            command.env(
                "AIDEX_DRIVE_LETTER",
                letter.to_ascii_uppercase().to_string(),
            );
        }
        Some(_) => {
            return Err(DriveError::InvalidRoot(
                "a Windows volume must have a drive letter".into(),
            ));
        }
        None => {
            command.env_remove("AIDEX_DRIVE_LETTER");
        }
    }
    let output = command
        .output()
        .map_err(|error| DriveError::Discovery(format!("cannot list volumes: {error}")))?;
    if !output.status.success() {
        return Err(DriveError::Discovery(format!(
            "cannot list volumes: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let system = std::env::var("SystemDrive").ok();
    parse(&String::from_utf8_lossy(&output.stdout), system.as_deref())
}

/// Decode [`SCRIPT`] output. `system_drive` is `%SystemDrive%` (`C:`).
pub(super) fn parse(json: &str, system_drive: Option<&str>) -> Result<Vec<RawVolume>, DriveError> {
    let json = json.trim().trim_start_matches('\u{feff}');
    let rows: Vec<Row> = if json.is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(json).map_err(|error| {
            DriveError::Discovery(format!("cannot parse the volume list: {error}"))
        })?
    };
    let system_letter = system_drive
        .and_then(|drive| drive.chars().next())
        .filter(char::is_ascii_alphabetic)
        .map(|letter| letter.to_ascii_uppercase());
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let letter = single_letter(&row.letter)?;
            let drive_type = row.drive_type.as_deref().unwrap_or_default();
            if is_skipped_drive_type(drive_type) {
                return None;
            }
            Some(RawVolume {
                root: PathBuf::from(format!("{letter}:\\")),
                label: non_empty(row.label),
                filesystem: non_empty(row.filesystem),
                total_bytes: reported_bytes(row.size),
                free_bytes: row.size.filter(|size| *size > 0).and(row.free),
                removable: classify(drive_type, row.bus_type.as_deref().unwrap_or_default()),
                system: system_letter == Some(letter),
            })
        })
        .collect())
}

/// Classify a volume from `Get-Volume` `DriveType` and `Get-Disk` `BusType`, accepting
/// both the display names and the raw CIM numbers PowerShell may print.
pub(super) fn classify(drive_type: &str, bus_type: &str) -> RemovableStatus {
    let drive = drive_type.trim().to_ascii_lowercase();
    let bus = bus_type.trim().to_ascii_lowercase();
    // MSFT_Disk BusType: 7 = USB, 12 = SD, 13 = MMC.
    let usb = matches!(bus.as_str(), "usb" | "7");
    let card = matches!(bus.as_str(), "sd" | "mmc" | "12" | "13");
    match drive.as_str() {
        // MSFT_Volume DriveType: 2 = removable, 3 = fixed.
        "removable" | "2" => RemovableStatus::Removable,
        "fixed" | "3" if usb => RemovableStatus::UsbFixed,
        "fixed" | "3" if card => RemovableStatus::Removable,
        "fixed" | "3" => RemovableStatus::Fixed,
        _ => RemovableStatus::Unknown,
    }
}

/// Optical (5) and network (4) drives can never hold an AiDEX drive.
fn is_skipped_drive_type(drive_type: &str) -> bool {
    matches!(
        drive_type.trim().to_ascii_lowercase().as_str(),
        "cd-rom" | "5" | "network" | "remote" | "4"
    )
}

fn single_letter(value: &str) -> Option<char> {
    let mut characters = value.trim().chars();
    let letter = characters.next()?;
    (letter.is_ascii_alphabetic() && characters.next().is_none())
        .then(|| letter.to_ascii_uppercase())
}

/// Zero means "not reported" (no media in the drive).
fn reported_bytes(value: Option<u64>) -> Option<u64> {
    value.filter(|bytes| *bytes > 0)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}
