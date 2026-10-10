//! Integrity verification of a bundle directory against its `bundle.json`.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    BundleError, BundleManifest, DocumentError, IntegrityProblem, MANIFEST_FILE_NAME,
    MAX_MANIFEST_BYTES,
    digest::{sha256_file, sha256_hex},
    layout::BIN_DIR,
    manifest::BundleFile,
    path::fold,
};

/// Which files [`verify_bundle`] hashes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyScope {
    /// Critical roles (launcher, cli, daemon, helper, runtime), and `bin/` must hold
    /// nothing unlisted (an unlisted library there could be loaded by the binaries).
    Critical,
    /// Every listed file, and the whole tree must hold nothing unlisted.
    All,
}

/// A bundle whose manifest parsed and whose files matched it.
#[derive(Clone, Debug)]
pub struct VerifiedBundle {
    /// The bundle directory as given.
    pub dir: PathBuf,
    pub manifest: BundleManifest,
    /// SHA-256 of the `bundle.json` bytes that were verified.
    pub manifest_sha256: String,
    pub scope: VerifyScope,
    /// Number of files hashed.
    pub verified_files: usize,
}

/// Read `bundle.json` from a bundle directory without verifying files. Returns the
/// raw bytes, which callers hash or pin before trusting the parsed manifest.
pub fn read_manifest_bytes(bundle_dir: &Path) -> Result<Vec<u8>, BundleError> {
    require_real_dir(bundle_dir)?;
    let path = bundle_dir.join(MANIFEST_FILE_NAME);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(BundleError::integrity(
                bundle_dir,
                MANIFEST_FILE_NAME,
                IntegrityProblem::Missing,
            ));
        }
        Err(error) => {
            return Err(BundleError::io(
                format!("cannot inspect `{}`", path.display()),
                error,
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(BundleError::integrity(
            bundle_dir,
            MANIFEST_FILE_NAME,
            IntegrityProblem::Link,
        ));
    }
    if !metadata.is_file() {
        return Err(BundleError::integrity(
            bundle_dir,
            MANIFEST_FILE_NAME,
            IntegrityProblem::NotRegularFile,
        ));
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(BundleError::Manifest {
            path,
            error: DocumentError::Malformed(format!("larger than {MAX_MANIFEST_BYTES} bytes")),
        });
    }
    let mut bytes = Vec::new();
    File::open(&path)
        .and_then(|file| file.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| BundleError::io(format!("cannot read `{}`", path.display()), error))?;
    Ok(bytes)
}

/// Parse manifest bytes read from `bundle_dir`.
pub(crate) fn parse_manifest(
    bundle_dir: &Path,
    bytes: &[u8],
) -> Result<BundleManifest, BundleError> {
    BundleManifest::from_json(bytes).map_err(|error| BundleError::Manifest {
        path: bundle_dir.join(MANIFEST_FILE_NAME),
        error,
    })
}

/// Read and validate a bundle's manifest; returns it with the digest of its bytes.
pub fn read_manifest(bundle_dir: &Path) -> Result<(BundleManifest, String), BundleError> {
    let bytes = read_manifest_bytes(bundle_dir)?;
    let manifest = parse_manifest(bundle_dir, &bytes)?;
    Ok((manifest, sha256_hex(&bytes)))
}

/// Verify a bundle directory: parse `bundle.json`, check that each file in `scope` is
/// a regular file (no links anywhere on its path) with the recorded size and SHA-256,
/// and that no unlisted file exists where `scope` forbids one.
pub fn verify_bundle(bundle_dir: &Path, scope: VerifyScope) -> Result<VerifiedBundle, BundleError> {
    let (manifest, manifest_sha256) = read_manifest(bundle_dir)?;
    let verified_files = verify_files(bundle_dir, &manifest, scope)?;
    Ok(VerifiedBundle {
        dir: bundle_dir.to_path_buf(),
        manifest,
        manifest_sha256,
        scope,
        verified_files,
    })
}

/// Verify the files of an already-parsed manifest; returns how many were hashed.
pub(crate) fn verify_files(
    bundle_dir: &Path,
    manifest: &BundleManifest,
    scope: VerifyScope,
) -> Result<usize, BundleError> {
    let listed: HashSet<String> = manifest.files.iter().map(|file| fold(&file.path)).collect();
    match scope {
        VerifyScope::Critical => reject_unlisted(bundle_dir, BIN_DIR, &listed, false)?,
        VerifyScope::All => reject_unlisted(bundle_dir, "", &listed, true)?,
    }
    let mut count = 0;
    for file in &manifest.files {
        if scope == VerifyScope::All || file.role.is_critical() {
            verify_file(bundle_dir, file)?;
            count += 1;
        }
    }
    Ok(count)
}

fn verify_file(bundle_dir: &Path, file: &BundleFile) -> Result<(), BundleError> {
    let fail = |problem| BundleError::integrity(bundle_dir, file.path.as_str(), problem);
    let segments: Vec<&str> = file.path.split('/').collect();
    let mut current = bundle_dir.to_path_buf();
    for (index, segment) in segments.iter().enumerate() {
        current.push(segment);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(fail(IntegrityProblem::Missing));
            }
            Err(error) => {
                return Err(BundleError::io(
                    format!("cannot inspect `{}`", current.display()),
                    error,
                ));
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(fail(IntegrityProblem::Link));
        }
        let last = index + 1 == segments.len();
        if (last && !metadata.is_file()) || (!last && !metadata.is_dir()) {
            return Err(fail(IntegrityProblem::NotRegularFile));
        }
        if last && metadata.len() != file.size {
            return Err(fail(IntegrityProblem::SizeMismatch {
                expected: file.size,
                actual: metadata.len(),
            }));
        }
    }
    let (digest, size) = sha256_file(&current)
        .map_err(|error| BundleError::io(format!("cannot read `{}`", current.display()), error))?;
    if size != file.size {
        return Err(fail(IntegrityProblem::SizeMismatch {
            expected: file.size,
            actual: size,
        }));
    }
    if digest != file.sha256 {
        return Err(fail(IntegrityProblem::DigestMismatch {
            expected: file.sha256.clone(),
            actual: digest,
        }));
    }
    Ok(())
}

/// Walk `relative` under `bundle_dir` and fail on any link, or on any regular file
/// that is not listed (`bundle.json` and `bundle.sig` at the top are allowed; the
/// signature is checked separately). With `recurse` false,
/// subdirectories are themselves treated as unlisted entries.
fn reject_unlisted(
    bundle_dir: &Path,
    relative: &str,
    listed: &HashSet<String>,
    recurse: bool,
) -> Result<(), BundleError> {
    let directory = if relative.is_empty() {
        bundle_dir.to_path_buf()
    } else {
        bundle_dir.join(relative)
    };
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound && !relative.is_empty() => {
            // A missing directory is reported by the per-file checks.
            return Ok(());
        }
        Err(error) => {
            return Err(BundleError::io(
                format!("cannot list `{}`", directory.display()),
                error,
            ));
        }
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            BundleError::io(format!("cannot list `{}`", directory.display()), error)
        })?;
        names.push(entry.file_name());
    }
    names.sort();
    for name in names {
        let name_text = name.to_string_lossy().into_owned();
        let child = if relative.is_empty() {
            name_text.clone()
        } else {
            format!("{relative}/{name_text}")
        };
        let path = directory.join(&name);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            BundleError::io(format!("cannot inspect `{}`", path.display()), error)
        })?;
        if metadata.file_type().is_symlink() {
            return Err(BundleError::integrity(
                bundle_dir,
                child,
                IntegrityProblem::Link,
            ));
        }
        if metadata.is_dir() {
            if recurse {
                reject_unlisted(bundle_dir, &child, listed, true)?;
                continue;
            }
            return Err(BundleError::integrity(
                bundle_dir,
                child,
                IntegrityProblem::Unlisted,
            ));
        }
        let is_reserved = relative.is_empty()
            && matches!(
                fold(&child).as_str(),
                MANIFEST_FILE_NAME | crate::SIGNATURE_FILE_NAME
            );
        if !is_reserved && (name.to_str().is_none() || !listed.contains(&fold(&child))) {
            return Err(BundleError::integrity(
                bundle_dir,
                child,
                IntegrityProblem::Unlisted,
            ));
        }
    }
    Ok(())
}

fn require_real_dir(dir: &Path) -> Result<(), BundleError> {
    match fs::symlink_metadata(dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(BundleError::integrity(dir, ".", IntegrityProblem::Link))
        }
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(BundleError::integrity(
            dir,
            ".",
            IntegrityProblem::NotRegularFile,
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Err(BundleError::integrity(dir, ".", IntegrityProblem::Missing))
        }
        Err(error) => Err(BundleError::io(
            format!("cannot inspect `{}`", dir.display()),
            error,
        )),
    }
}
