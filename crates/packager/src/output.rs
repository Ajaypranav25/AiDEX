//! Human and JSON reports for each command.

use std::path::{Path, PathBuf};

use aidex_bundle::InstallReport;
use serde::Serialize;

use crate::{build::BuildReport, error::PackagerError, smoke::SmokeReport};

/// `path` without the Windows verbatim prefix (`\\?\C:\x` becomes `C:\x`) for display.
fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

fn json(value: &impl Serialize) -> Result<(), PackagerError> {
    let text = serde_json::to_string(value)
        .map_err(|error| PackagerError::new(format!("cannot encode the report: {error}")))?;
    println!("{text}");
    Ok(())
}

fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

pub fn build(report: &BuildReport, as_json: bool) -> Result<(), PackagerError> {
    if as_json {
        return json(report);
    }
    println!(
        "Bundle: {} ({}, {})",
        report.bundle_dir.display(),
        report.target,
        report.rust_target
    );
    println!(
        "Version: {} (commit {}{})",
        report.version,
        report.git_commit.as_deref().unwrap_or("unknown"),
        if report.source_dirty == Some(true) {
            ", uncommitted changes"
        } else {
            ""
        }
    );
    println!(
        "Files: {} ({}), licenses: {}",
        report.files,
        mib(report.bundle_bytes),
        report.licenses
    );
    if !report.missing_license_texts.is_empty() {
        println!(
            "Components without a bundled license text: {}",
            report.missing_license_texts.join(", ")
        );
    }
    println!(
        "Archive: {} ({}, sha256 {})",
        report.archive.display(),
        mib(report.archive_bytes),
        report.archive_sha256
    );
    println!("Checksums: {}", report.sha256sums.display());
    println!(
        "Time: cargo build {} ms, assemble {} ms, archive {} ms, total {} ms",
        report.timings.cargo_build_ms,
        report.timings.assemble_ms,
        report.timings.archive_ms,
        report.timings.total_ms
    );
    Ok(())
}

pub fn install(report: &InstallReport, as_json: bool) -> Result<(), PackagerError> {
    if as_json {
        return json(report);
    }
    println!(
        "Installed AiDEX {} for {} at {} ({})",
        report.version,
        report.target,
        report.dir,
        if report.reused_existing {
            "verified existing copy"
        } else {
            "new"
        }
    );
    println!("Drive: {}", plain(&report.drive_root).display());
    if let Some(previous) = &report.previous {
        println!("Previous: {} ({})", previous.version, previous.dir);
    }
    for launcher in &report.launchers {
        println!(
            "Launcher: {} ({:?})",
            plain(&launcher.path).display(),
            launcher.action
        );
        if let Some(previous) = &launcher.previous {
            println!("  previous launcher kept as {}", plain(previous).display());
        }
    }
    Ok(())
}

pub fn smoke(report: &SmokeReport, as_json: bool) -> Result<(), PackagerError> {
    if as_json {
        return json(report);
    }
    println!("Smoke test passed: {}", report.archive.display());
    println!(
        "Drive: {} (install {} ms)",
        report.drive.display(),
        report.install_ms
    );
    for ran in &report.commands {
        println!(
            "  {} -> {:?} ({} ms)",
            ran.args.join(" "),
            ran.exit_code,
            ran.duration_ms
        );
    }
    println!(
        "Doctor: ok={:?} drive_ok={:?} oh_my_pi={:?}",
        report.doctor.ok, report.doctor.drive_ok, report.doctor.oh_my_pi_available
    );
    println!(
        "Host residue: none in {} watched directories",
        report.residue_watched.len()
    );
    Ok(())
}
