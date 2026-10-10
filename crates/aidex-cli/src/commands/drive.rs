//! `aidex init` and `aidex status`: drive provisioning and health.

use crate::{
    exit::{CliError, Exit},
    output::Output,
};
use aidex_drive::{DriveStatus, init as init_drive, resolve_root, status as drive_status};
use serde::Serialize;
use std::path::Path;

/// `--json` shape of a drive report.
#[derive(Debug, Serialize)]
pub struct DriveReport {
    pub root: String,
    pub schema_version: Option<u32>,
    pub writable: bool,
    pub free_bytes: Option<u64>,
    pub migration_required: bool,
}

impl From<&DriveStatus> for DriveReport {
    fn from(status: &DriveStatus) -> Self {
        Self {
            root: status.root.display().to_string(),
            schema_version: status.schema_version,
            writable: status.writable,
            free_bytes: status.free_bytes,
            migration_required: status.migration_required(),
        }
    }
}

pub fn print_status(status: &DriveStatus) {
    println!("Drive root: {}", status.root.display());
    println!(
        "Schema version: {}",
        status
            .schema_version
            .map_or_else(|| "unavailable".into(), |version| version.to_string())
    );
    println!("Writable: {}", if status.writable { "yes" } else { "no" });
    println!(
        "Free space: {}",
        status
            .free_bytes
            .map_or_else(|| "unavailable".into(), |bytes| format!("{bytes} bytes"))
    );
    if let Some(advice) = status.migration_advice() {
        println!("Migration: {advice}");
    }
}

/// Human placeholder lines for a drive that could not be inspected.
pub fn print_unavailable(root: &Path) {
    println!("Drive root: {}", root.display());
    println!("Schema version: unavailable");
    println!("Writable: unknown");
    println!("Free space: unavailable");
}

pub fn init(drive: &Path, output: Output) -> Result<Exit, CliError> {
    let status = init_drive(drive)?;
    output.emit(&DriveReport::from(&status), || print_status(&status))?;
    Ok(Exit::Success)
}

pub fn status(drive: Option<&Path>, output: Output) -> Result<Exit, CliError> {
    let root = resolve_root(drive)?;
    let status = drive_status(&root).inspect_err(|_| {
        if !output.is_json() {
            print_unavailable(&root);
        }
    })?;
    output.emit(&DriveReport::from(&status), || print_status(&status))?;
    Ok(if status.writable {
        Exit::Success
    } else {
        Exit::Drive
    })
}
