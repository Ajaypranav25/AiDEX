//! Update sources: offline and manual packages first.
//!
//! An update package is a signed bundle archive (`aidex-<version>-<target>.zip` or
//! `.tar.gz`), optionally with the release's `SHA256SUMS` next to it. When that file
//! is present it must list the archive with the matching digest; it is a transfer
//! check only — trust comes from the bundle's signature, never from the checksum file
//! or the transport. No network feed is implemented.

use std::{
    fs,
    path::{Path, PathBuf},
};

use aidex_bundle::{
    InstallSource, MANIFEST_FILE_NAME, SHA256SUMS_FILE_NAME, Sha256Sums, Target,
    archive::{ArchiveFormat, archive_file_name},
    sha256_file, validate_version,
};
use serde::{Deserialize, Serialize};

use crate::{UpdateError, UpdateLimits, version::parse_semver};

/// Where an update comes from.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "path", rename_all = "snake_case")]
pub enum UpdateSource {
    /// A package archive (`.zip`, `.tar.gz`, `.tgz`).
    LocalFile(PathBuf),
    /// An unpacked bundle directory (containing `bundle.json`), or a release directory
    /// holding packages for one or more targets: the newest package for the update
    /// target is used.
    Directory(PathBuf),
}

impl UpdateSource {
    /// A file is a [`LocalFile`](Self::LocalFile), a directory a
    /// [`Directory`](Self::Directory).
    pub fn from_path(path: &Path) -> Result<Self, UpdateError> {
        let metadata = fs::metadata(path).map_err(|_| UpdateError::SourceNotFound {
            path: path.to_path_buf(),
        })?;
        if metadata.is_dir() {
            Ok(Self::Directory(path.to_path_buf()))
        } else {
            Ok(Self::LocalFile(path.to_path_buf()))
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::LocalFile(path) | Self::Directory(path) => path,
        }
    }
}

/// How the package's `SHA256SUMS` entry was checked.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumStatus {
    /// `SHA256SUMS` next to the archive lists it with the matching digest.
    Verified,
    /// There is no `SHA256SUMS` next to the archive (the signature still decides).
    Absent,
    /// An unpacked bundle directory has no archive to check.
    NotApplicable,
}

/// A source narrowed down to one package.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedSource {
    pub install: InstallSource,
    pub package: PathBuf,
    pub checksum: ChecksumStatus,
}

pub(crate) fn resolve(
    source: &UpdateSource,
    target: Target,
    limits: &UpdateLimits,
) -> Result<ResolvedSource, UpdateError> {
    match source {
        UpdateSource::LocalFile(path) => archive(path, limits),
        UpdateSource::Directory(dir) => {
            if !dir.is_dir() {
                return Err(UpdateError::SourceNotFound { path: dir.clone() });
            }
            if dir.join(MANIFEST_FILE_NAME).is_file() {
                return Ok(ResolvedSource {
                    install: InstallSource::Directory(dir.clone()),
                    package: dir.clone(),
                    checksum: ChecksumStatus::NotApplicable,
                });
            }
            let newest = newest_package(dir, target)?.ok_or_else(|| UpdateError::NoPackage {
                dir: dir.clone(),
                target,
            })?;
            archive(&newest, limits)
        }
    }
}

fn archive(path: &Path, limits: &UpdateLimits) -> Result<ResolvedSource, UpdateError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| UpdateError::SourceNotFound {
        path: path.to_path_buf(),
    })?;
    if !metadata.is_file() {
        return Err(UpdateError::InvalidSource(format!(
            "`{}` is not a regular file",
            path.display()
        )));
    }
    if ArchiveFormat::detect(path).is_none() {
        return Err(UpdateError::InvalidSource(format!(
            "`{}` is not a .zip or .tar.gz update package",
            path.display()
        )));
    }
    if metadata.len() > limits.max_archive_bytes {
        return Err(UpdateError::ArchiveTooLarge {
            detail: format!(
                "`{}` is {} bytes; the limit is {} bytes",
                path.display(),
                metadata.len(),
                limits.max_archive_bytes
            ),
        });
    }
    let checksum = check_sums(path)?;
    Ok(ResolvedSource {
        install: InstallSource::Archive(path.to_path_buf()),
        package: path.to_path_buf(),
        checksum,
    })
}

fn check_sums(archive: &Path) -> Result<ChecksumStatus, UpdateError> {
    let sums_path = archive
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(SHA256SUMS_FILE_NAME);
    if !sums_path.is_file() {
        return Ok(ChecksumStatus::Absent);
    }
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| UpdateError::InvalidSource("the package name is not UTF-8".into()))?
        .to_owned();
    let sums = Sha256Sums::load(&sums_path)?;
    let expected = sums
        .get(&name)
        .ok_or_else(|| UpdateError::ChecksumMissing { file: name.clone() })?
        .to_owned();
    let (actual, _) = sha256_file(archive)
        .map_err(|error| UpdateError::io(format!("cannot read `{}`", archive.display()), error))?;
    if actual != expected {
        return Err(UpdateError::ChecksumMismatch {
            file: name,
            expected,
            actual,
        });
    }
    Ok(ChecksumStatus::Verified)
}

/// The highest-versioned `aidex-<version>-<target>.<ext>` regular file in `dir`.
fn newest_package(dir: &Path, target: Target) -> Result<Option<PathBuf>, UpdateError> {
    let entries = fs::read_dir(dir)
        .map_err(|error| UpdateError::io(format!("cannot list `{}`", dir.display()), error))?;
    let suffix = format!("-{target}");
    let mut best: Option<(semver::Version, PathBuf)> = None;
    for entry in entries {
        let entry = entry
            .map_err(|error| UpdateError::io(format!("cannot list `{}`", dir.display()), error))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let Some(stem) = name
            .strip_suffix(ArchiveFormat::for_target(target).extension())
            .and_then(|stem| stem.strip_prefix("aidex-"))
            .and_then(|stem| stem.strip_suffix(&suffix))
        else {
            continue;
        };
        if validate_version(stem).is_err() || archive_file_name(stem, target) != name {
            continue;
        }
        let Ok(version) = parse_semver(stem) else {
            continue;
        };
        let newer = best
            .as_ref()
            .is_none_or(|(current, _)| version.cmp_precedence(current).is_gt());
        if newer {
            best = Some((version, entry.path()));
        }
    }
    Ok(best.map(|(_, path)| path))
}
