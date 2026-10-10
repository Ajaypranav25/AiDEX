//! Read verified distribution metadata for setup before writing to a drive.
use crate::{BundleError, FileRole, Target, VerifyScope, verify_bundle};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub struct BundleCatalog {
    pub source: PathBuf,
    pub version: String,
    pub target: Target,
    pub installed_bytes: u64,
    pub required_free_bytes: u64,
    pub file_count: usize,
    pub manifest_sha256: String,
}

/// A desktop installation requires an executable desktop, not a CLI-only bundle.
pub fn desktop_catalog(source: &Path) -> Result<BundleCatalog, BundleError> {
    let verified = verify_bundle(source, VerifyScope::All)?;
    let manifest = &verified.manifest;
    let desktop_name = manifest.target.executable_name("aidex-desktop");
    if !manifest.files.iter().any(|entry| {
        entry.role == FileRole::Desktop
            && entry.executable
            && Path::new(&entry.path)
                .file_name()
                .is_some_and(|name| name == desktop_name.as_str())
    }) {
        return Err(BundleError::InvalidSource(
            "the distribution has no executable AiDEX desktop".into(),
        ));
    }
    let installed_bytes = manifest
        .files
        .iter()
        .try_fold(0u64, |total, entry| total.checked_add(entry.size))
        .ok_or_else(|| BundleError::InvalidSource("bundle size overflow".into()))?;
    let required_free_bytes = installed_bytes
        .checked_mul(2)
        .and_then(|size| size.checked_add(16 * 1024 * 1024))
        .ok_or_else(|| BundleError::InvalidSource("required space overflow".into()))?;
    Ok(BundleCatalog {
        source: source.to_path_buf(),
        version: manifest.version.clone(),
        target: manifest.target,
        installed_bytes,
        required_free_bytes,
        file_count: manifest.files.len(),
        manifest_sha256: verified.manifest_sha256,
    })
}
