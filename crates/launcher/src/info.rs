//! `--launcher-info` / `--launcher-verify`: a JSON report of the verified bundle.

use aidex_bundle::{LAUNCHER_MARKER, SignatureState, Target, VerifyScope, VersionRef};
use serde::Serialize;

use crate::{error::LaunchError, launch::resolve};

#[derive(Serialize)]
struct LauncherReport {
    ok: bool,
    launcher_version: &'static str,
    /// The marker that identifies AiDEX launchers (also keeps it in the binary).
    marker: &'static str,
    drive_root: String,
    target: Target,
    bundle: BundleReport,
    /// Whether this launcher build trusts `AIDEX_DEV_TRUSTED_KEYS` (test builds only).
    dev_trust: bool,
    /// State of an interrupted update, if any.
    pending_update: Option<String>,
}

#[derive(Serialize)]
struct BundleReport {
    version: String,
    dir: String,
    path: String,
    manifest_sha256: String,
    git_commit: Option<String>,
    rust_target: String,
    scope: VerifyScope,
    verified_files: usize,
    signature: SignatureState,
    previous: Option<VersionRef>,
}

/// Verify with `scope` and print one JSON line on stdout.
pub fn print(scope: VerifyScope) -> Result<(), LaunchError> {
    let resolved = resolve(scope)?;
    let bundle = &resolved.active.bundle;
    let report = LauncherReport {
        ok: true,
        launcher_version: env!("CARGO_PKG_VERSION"),
        marker: LAUNCHER_MARKER,
        drive_root: resolved.drive_root().display().to_string(),
        target: resolved.active.target,
        dev_trust: aidex_bundle::TrustStore::dev_trust_enabled(),
        pending_update: resolved.pending_update.clone(),
        bundle: BundleReport {
            version: bundle.manifest.version.clone(),
            dir: resolved.active.pointer.dir.clone(),
            path: resolved.bundle_dir().display().to_string(),
            manifest_sha256: bundle.manifest_sha256.clone(),
            git_commit: bundle.manifest.git_commit.clone(),
            rust_target: bundle.manifest.rust_target.clone(),
            scope: bundle.scope,
            verified_files: bundle.verified_files,
            signature: resolved.active.signature.clone(),
            previous: resolved.active.pointer.previous.clone(),
        },
    };
    let json = serde_json::to_string(&report).map_err(|error| {
        LaunchError::io("cannot encode the report", std::io::Error::other(error))
    })?;
    println!("{json}");
    Ok(())
}
