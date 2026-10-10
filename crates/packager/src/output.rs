//! Human and JSON reports for each command.

use std::path::{Path, PathBuf};

use aidex_bundle::{InstallReport, SignatureState};
use serde::Serialize;

use crate::{
    build::BuildReport,
    error::PackagerError,
    signing::{KeygenReport, SignReport, VerifyReport},
    smoke::SmokeReport,
};

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
    println!(
        "SBOM: {} ({} components, CycloneDX 1.5)",
        report.sbom.display(),
        report.sbom_components
    );
    if report.trusted_keys.is_empty() {
        println!("Trust: release keys only (sign with a release key, or rebuild with --trust-key)");
    } else {
        println!("Trust: release keys + {}", report.trusted_keys.join(", "));
    }
    println!("Checksums: {}", report.sha256sums.display());
    println!("Next: sign the archive with `aidex-packager sign --key <file.key> <archive>`");
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
    println!("Signature: {}", signature_text(report.signature.as_ref()));
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

fn signature_text(signature: Option<&SignatureState>) -> String {
    match signature {
        Some(SignatureState::Verified {
            key_id, key_name, ..
        }) => format!("verified (key {key_name}, {key_id})"),
        Some(SignatureState::UnsignedAllowed) => "UNSIGNED (allowed by --allow-unsigned)".into(),
        None => "not checked".into(),
    }
}

pub fn keygen(report: &KeygenReport, as_json: bool) -> Result<(), PackagerError> {
    if as_json {
        return json(report);
    }
    println!("Key: {} (id {})", report.name, report.key_id);
    println!(
        "Secret key: {} (keep it secret; never commit or upload it)",
        report.secret_key_file.display()
    );
    println!("Public key: {}", report.public_key_file.display());
    println!("This is a development/CI key, not a release key.");
    Ok(())
}

pub fn sign(report: &SignReport, as_json: bool) -> Result<(), PackagerError> {
    if as_json {
        return json(report);
    }
    println!(
        "Signed AiDEX {} for {} with key {} ({})",
        report.version, report.target, report.key_name, report.key_id
    );
    println!("Bundle: {}", report.bundle.display());
    println!("bundle.json sha256: {}", report.manifest_sha256);
    if let Some(digest) = &report.archive_sha256 {
        println!(
            "Archive sha256: {digest}{}",
            if report.sha256sums_updated {
                " (SHA256SUMS updated)"
            } else {
                ""
            }
        );
    }
    Ok(())
}

pub fn verify(report: &VerifyReport, as_json: bool) -> Result<(), PackagerError> {
    if as_json {
        return json(report);
    }
    println!(
        "Verified AiDEX {} for {}: {} files, checksum {}",
        report.version, report.target, report.verified_files, report.checksum
    );
    println!("Signature: {}", signature_text(Some(&report.signature)));
    match (&report.sbom.summary, &report.sbom.error) {
        (Some(summary), _) => println!(
            "SBOM: {} components ({})",
            summary.components, summary.subject
        ),
        (None, Some(error)) => println!("SBOM: invalid ({error})"),
        (None, None) => println!("SBOM: none"),
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
        "Doctor: ok={:?} drive_ok={:?} oh_my_pi={:?} integrity={:?} signer={:?} sbom={:?}",
        report.doctor.ok,
        report.doctor.drive_ok,
        report.doctor.oh_my_pi_available,
        report.doctor.integrity_verified,
        report.doctor.signer_key_id,
        report.doctor.sbom_valid
    );
    println!(
        "Host residue: none in {} watched directories",
        report.residue_watched.len()
    );
    Ok(())
}
