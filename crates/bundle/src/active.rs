//! Resolve and verify the active bundle of a target on a drive (what the launcher runs).

use std::{fs, io::ErrorKind, path::PathBuf};

use aidex_drive::{CURRENT_SCHEMA_VERSION, DrivePaths};

use crate::{
    BundleError, CurrentPointer, IntegrityProblem, SignaturePolicy, SignatureState, Target,
    TargetPointer, VerifiedBundle, VerifyScope,
    digest::sha256_hex,
    layout::{pointer_path, versions_dir},
    signature::verify_signature,
    verify::{parse_manifest, read_manifest_bytes, verify_files},
};

/// The verified active bundle for one target.
#[derive(Clone, Debug)]
pub struct ActiveBundle {
    pub target: Target,
    /// The pointer entry that selected the bundle.
    pub pointer: TargetPointer,
    pub bundle: VerifiedBundle,
    /// How the bundle's signature was accepted.
    pub signature: SignatureState,
}

impl ActiveBundle {
    /// Absolute path of a bundle file.
    pub fn file_path(&self, relative: &str) -> PathBuf {
        crate::path::join_relative(&self.bundle.dir, relative)
    }
}

/// Resolve `app/current.json` for `target` on an opened drive and verify the bundle it
/// selects before anything from it runs.
///
/// Fails closed when the pointer is missing or malformed, escapes `app/versions/`,
/// names a link or missing directory, when `bundle.json` differs from the pinned
/// digest, is for another target or version, needs a newer drive schema or host
/// extraction, when its signature is missing (unless `policy` allows unsigned
/// bundles), malformed, made by an unknown, revoked, or out-of-window key, or does not
/// verify, or when any file in `scope` does not match the manifest.
pub fn resolve_active(
    paths: &DrivePaths,
    target: Target,
    scope: VerifyScope,
    policy: &SignaturePolicy,
) -> Result<ActiveBundle, BundleError> {
    let pointer_file = pointer_path(paths);
    let pointer = CurrentPointer::load(&pointer_file)?.ok_or(BundleError::NotInstalled {
        pointer: pointer_file.clone(),
    })?;
    let entry = pointer
        .get(target)
        .cloned()
        .ok_or_else(|| BundleError::TargetNotInstalled {
            target,
            installed: pointer.installed_targets(),
        })?;
    let name = entry.dir_name().map_err(|error| BundleError::Pointer {
        path: pointer_file.clone(),
        error,
    })?;
    let versions = versions_dir(paths);
    require_plain_dir(&paths.app())?;
    require_plain_dir(&versions)?;
    let bundle_dir = versions.join(name);

    let bytes = read_manifest_bytes(&bundle_dir)?;
    let manifest_sha256 = sha256_hex(&bytes);
    if manifest_sha256 != entry.manifest_sha256 {
        return Err(BundleError::integrity(
            &bundle_dir,
            crate::MANIFEST_FILE_NAME,
            IntegrityProblem::ManifestPin {
                expected: entry.manifest_sha256.clone(),
                actual: manifest_sha256,
            },
        ));
    }
    let manifest = parse_manifest(&bundle_dir, &bytes)?;
    if manifest.target != target {
        return Err(BundleError::WrongTarget {
            expected: target,
            found: manifest.target,
        });
    }
    if manifest.version != entry.version {
        return Err(BundleError::VersionMismatch {
            pointer: entry.version.clone(),
            manifest: manifest.version.clone(),
        });
    }
    if manifest.min_drive_schema > CURRENT_SCHEMA_VERSION {
        return Err(BundleError::DriveSchemaTooOld {
            required: manifest.min_drive_schema,
            found: CURRENT_SCHEMA_VERSION,
        });
    }
    if manifest.state.host_extraction {
        return Err(BundleError::HostExtractionUnsupported);
    }
    let signature = verify_signature(&bundle_dir, &bytes, &manifest.created_at, policy)?;
    let verified_files = verify_files(&bundle_dir, &manifest, scope)?;
    Ok(ActiveBundle {
        target,
        pointer: entry,
        bundle: VerifiedBundle {
            dir: bundle_dir,
            manifest,
            manifest_sha256,
            scope,
            verified_files,
        },
        signature,
    })
}

/// `app/versions/` must be a real directory (a link there could redirect every bundle).
fn require_plain_dir(dir: &std::path::Path) -> Result<(), BundleError> {
    match fs::symlink_metadata(dir) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(BundleError::integrity(dir, ".", IntegrityProblem::Link))
        }
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
