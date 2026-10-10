//! Installed runtimes: the per-install manifest, discovery on the drive, and
//! verification before every launch.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use aidex_core::{CancelToken, Timestamp};
use aidex_drive::DrivePaths;
use serde::{Deserialize, Serialize};

use super::{RuntimeCatalog, RuntimeVariant};
use crate::{
    InferenceError, Result,
    fsutil::{self, display},
};

/// Manifest file written into every install directory.
pub const MANIFEST_FILE: &str = "aidex-runtime.json";
pub const MANIFEST_VERSION: u32 = 1;
/// Engine directory under `runtime/<platform>/`.
pub const ENGINE_DIR: &str = "llama";
/// Largest manifest read.
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
/// Files a runtime may contain.
pub(crate) const MAX_RUNTIME_FILES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    /// Path inside the install directory, `/` separated.
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

/// What was installed, from which archive, under which licence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeManifest {
    pub version: u32,
    pub catalog_id: String,
    pub engine: String,
    pub runtime_version: String,
    pub platform: String,
    pub variant: RuntimeVariant,
    pub archive: String,
    pub archive_sha256: String,
    pub url: String,
    pub source: String,
    pub license: String,
    pub server_binary: String,
    pub files: Vec<ManifestFile>,
    /// Hash over `files` (see [`tree_sha256`]).
    pub tree_sha256: String,
    pub installed_at: Timestamp,
}

/// One install directory on the drive.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledRuntime {
    /// Absolute install directory.
    pub dir: PathBuf,
    /// Drive-relative install directory (`runtime/windows-x64/llama/b11205-cpu`).
    pub relative_dir: String,
    pub manifest: RuntimeManifest,
}

/// Summary of the files a runtime verification checked.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeVerification {
    pub files: usize,
    pub bytes: u64,
}

impl InstalledRuntime {
    pub fn server_path(&self) -> PathBuf {
        join_relative(&self.dir, &self.manifest.server_binary)
    }

    /// Verify every recorded file (size and SHA-256), refuse unexpected files
    /// (a planted DLL next to `llama-server.exe` would be loaded), and require
    /// the install to match its pinned catalog entry.
    pub fn verify(
        &self,
        catalog: &RuntimeCatalog,
        cancel: &CancelToken,
    ) -> Result<RuntimeVerification> {
        let name = self.manifest.catalog_id.clone();
        let corrupt = |reason: String| InferenceError::RuntimeCorrupt {
            runtime: name.clone(),
            reason,
        };
        let pinned = catalog
            .find(&self.manifest.catalog_id)
            .ok_or_else(|| corrupt("it is not a runtime of this build's pinned catalog".into()))?;
        if !pinned
            .sha256
            .eq_ignore_ascii_case(&self.manifest.archive_sha256)
            || pinned.platform != self.manifest.platform
            || pinned.variant != self.manifest.variant
            || pinned.server_binary != self.manifest.server_binary
        {
            return Err(corrupt(
                "its manifest does not match the pinned catalog entry".into(),
            ));
        }
        let recomputed = tree_sha256(&self.manifest.files);
        if recomputed != self.manifest.tree_sha256 {
            return Err(corrupt("its manifest file list was modified".into()));
        }
        if let Some(expected) = &pinned.tree_sha256 {
            if !expected.eq_ignore_ascii_case(&recomputed) {
                return Err(corrupt(
                    "its files differ from the pinned release contents".into(),
                ));
            }
        }
        let expected: BTreeMap<&str, &ManifestFile> = self
            .manifest
            .files
            .iter()
            .map(|file| (file.path.as_str(), file))
            .collect();
        let present = list_files(&self.dir).map_err(|error| corrupt(error.to_string()))?;
        for path in &present {
            if path != MANIFEST_FILE && !expected.contains_key(path.as_str()) {
                return Err(corrupt(format!(
                    "unexpected file `{path}` in the runtime directory"
                )));
            }
        }
        let mut bytes = 0u64;
        for file in &self.manifest.files {
            if cancel.is_stopped() {
                return Err(InferenceError::Cancelled);
            }
            let path = join_relative(&self.dir, &file.path);
            let metadata = fsutil::regular_file(&path, "runtime file").map_err(|_| {
                corrupt(format!("`{}` is missing or not a regular file", file.path))
            })?;
            if metadata.len() != file.size_bytes {
                return Err(corrupt(format!("`{}` changed size", file.path)));
            }
            let (digest, _) = fsutil::sha256_file(&path, &mut |_| {}, cancel)?;
            if digest != file.sha256 {
                return Err(corrupt(format!("`{}` failed its SHA-256 check", file.path)));
            }
            bytes = bytes.saturating_add(file.size_bytes);
        }
        if !expected.contains_key(self.manifest.server_binary.as_str()) {
            return Err(corrupt("the server binary is not in the manifest".into()));
        }
        Ok(RuntimeVerification {
            files: self.manifest.files.len(),
            bytes,
        })
    }
}

/// `sha256sum`-style lines (`<sha256>  <path>\n`) of `files` sorted by path, hashed.
pub fn tree_sha256(files: &[ManifestFile]) -> String {
    let mut sorted: Vec<&ManifestFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut text = String::new();
    for file in sorted {
        text.push_str(&file.sha256);
        text.push_str("  ");
        text.push_str(&file.path);
        text.push('\n');
    }
    fsutil::sha256_bytes(text.as_bytes())
}

pub(crate) fn join_relative(dir: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// Every regular file under `dir`, relative and `/` separated. Links and other
/// special files are reported as errors.
fn list_files(dir: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let mut pending = vec![(dir.to_path_buf(), String::new())];
    while let Some((current, prefix)) = pending.pop() {
        let read = fs::read_dir(&current).map_err(|error| {
            InferenceError::io(format!("cannot list {}", display(&current)), &error)
        })?;
        for item in read {
            let item = item.map_err(|error| {
                InferenceError::io(format!("cannot list {}", display(&current)), &error)
            })?;
            let name = item
                .file_name()
                .into_string()
                .map_err(|_| InferenceError::corrupt(item.path(), "file name is not UTF-8"))?;
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let kind = fs::symlink_metadata(item.path()).map_err(|error| {
                InferenceError::io(format!("cannot inspect {relative}"), &error)
            })?;
            fsutil::refuse_symlink(&item.path())?;
            if kind.is_dir() {
                pending.push((item.path(), relative));
            } else if kind.is_file() {
                files.push(relative);
            } else {
                return Err(InferenceError::corrupt(
                    item.path(),
                    "is not a regular file",
                ));
            }
            if files.len() > MAX_RUNTIME_FILES {
                return Err(InferenceError::corrupt(dir, "contains too many files"));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// A runtime directory whose manifest could not be read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProblem {
    pub dir: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledListing {
    pub runtimes: Vec<InstalledRuntime>,
    pub problems: Vec<RuntimeProblem>,
}

/// Every install under `runtime/<platform>/llama/` for every platform on the
/// drive (a portable drive may carry runtimes for several operating systems).
pub fn list_installed(paths: &DrivePaths) -> Result<InstalledListing> {
    let mut listing = InstalledListing::default();
    let root = paths.runtime();
    let platforms = match fs::read_dir(&root) {
        Ok(read) => read,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(listing),
        Err(error) => {
            return Err(InferenceError::io(
                format!("cannot list {}", display(&root)),
                &error,
            ));
        }
    };
    for platform in platforms.flatten() {
        let engine = platform.path().join(ENGINE_DIR);
        let Ok(installs) = fs::read_dir(&engine) else {
            continue;
        };
        for install in installs.flatten() {
            let dir = install.path();
            let name = install.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !dir.is_dir() {
                continue;
            }
            let relative =
                fsutil::drive_relative(paths.root(), &dir).unwrap_or_else(|_| display(&dir));
            match read_manifest(&dir) {
                Ok(manifest) => listing.runtimes.push(InstalledRuntime {
                    dir,
                    relative_dir: relative,
                    manifest,
                }),
                Err(error) => listing.problems.push(RuntimeProblem {
                    dir: relative,
                    message: error.to_string(),
                }),
            }
        }
    }
    listing
        .runtimes
        .sort_by(|a, b| a.relative_dir.cmp(&b.relative_dir));
    Ok(listing)
}

pub(crate) fn read_manifest(dir: &Path) -> Result<RuntimeManifest> {
    let path = dir.join(MANIFEST_FILE);
    let metadata = fsutil::regular_file(&path, "runtime manifest")?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(InferenceError::corrupt(&path, "manifest is too large"));
    }
    let bytes = fs::read(&path)
        .map_err(|error| InferenceError::io(format!("cannot read {}", display(&path)), &error))?;
    let manifest: RuntimeManifest = serde_json::from_slice(&bytes)
        .map_err(|error| InferenceError::corrupt(&path, format!("invalid manifest: {error}")))?;
    if manifest.version != MANIFEST_VERSION {
        return Err(InferenceError::corrupt(
            &path,
            format!("manifest version {} is not supported", manifest.version),
        ));
    }
    for file in &manifest.files {
        if file.path.is_empty()
            || file.path.split('/').any(|part| {
                part.is_empty() || part == "." || part == ".." || part.contains(['\\', ':'])
            })
        {
            return Err(InferenceError::corrupt(
                &path,
                "manifest lists an unsafe path",
            ));
        }
    }
    Ok(manifest)
}
