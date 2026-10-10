//! Deterministic bundle archives (`.zip` for Windows, `.tar.gz` elsewhere) and bounded,
//! fail-closed extraction.
//!
//! Writing: entries are the bundle's top-level directory followed by `bundle.json` and
//! every listed file, sorted by path; every entry gets the manifest's `created_at` as
//! its timestamp, mode 0755 (executables) or 0644, and owner 0:0; the gzip header
//! carries no name or time. Extraction: only regular files and directories under one
//! top-level directory are accepted; links, devices, absolute paths, traversal,
//! duplicate (case-insensitive) names, and oversized content are refused.

mod tar_format;
mod zip_format;

use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::{
    BundleError, MANIFEST_FILE_NAME, Target, VerifiedBundle, VerifyScope,
    digest::sha256_file,
    fsutil::temp_sibling,
    path::{fold, join_relative, validate_bundle_path},
};

/// Archive container format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

impl ArchiveFormat {
    /// `.zip` for Windows targets, `.tar.gz` for Unix targets.
    pub fn for_target(target: Target) -> Self {
        if target.is_windows() {
            Self::Zip
        } else {
            Self::TarGz
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => ".zip",
            Self::TarGz => ".tar.gz",
        }
    }

    /// Detect the format from a file name (`.zip`, `.tar.gz`, or `.tgz`).
    pub fn detect(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        if name.ends_with(".zip") {
            Some(Self::Zip)
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(Self::TarGz)
        } else {
            None
        }
    }
}

/// `aidex-<version>-<target>`: the archive stem and its top-level directory.
pub fn bundle_root_name(version: &str, target: Target) -> String {
    format!("aidex-{version}-{target}")
}

/// `aidex-<version>-<target>.zip` or `.tar.gz`.
pub fn archive_file_name(version: &str, target: Target) -> String {
    format!(
        "{}{}",
        bundle_root_name(version, target),
        ArchiveFormat::for_target(target).extension()
    )
}

/// Written archive facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveSummary {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub entries: usize,
}

/// One archive member, in write order.
pub(crate) struct Member {
    /// Path inside the archive, `<root>/<relative>`.
    pub name: String,
    pub source: PathBuf,
    pub mode: u32,
}

/// Write a fully verified bundle as a deterministic archive at `out` (via a temporary
/// sibling and rename). The bundle must have been verified with [`VerifyScope::All`].
pub fn write_archive(
    bundle: &VerifiedBundle,
    out: &Path,
    format: ArchiveFormat,
) -> Result<ArchiveSummary, BundleError> {
    if bundle.scope != VerifyScope::All {
        return Err(BundleError::InvalidSource(
            "archives are written only from bundles verified with VerifyScope::All".into(),
        ));
    }
    let manifest = &bundle.manifest;
    let root = bundle_root_name(&manifest.version, manifest.target);
    let mut members = vec![Member {
        name: format!("{root}/{MANIFEST_FILE_NAME}"),
        source: bundle.dir.join(MANIFEST_FILE_NAME),
        mode: 0o644,
    }];
    for file in &manifest.files {
        members.push(Member {
            name: format!("{root}/{}", file.path),
            source: join_relative(&bundle.dir, &file.path),
            mode: if file.executable { 0o755 } else { 0o644 },
        });
    }
    members.sort_by(|a, b| a.name.cmp(&b.name));
    let mtime = u64::try_from(manifest.created_at.unix_millis().div_euclid(1000)).unwrap_or(0);

    let temporary = temp_sibling(out, "partial");
    let written = match format {
        ArchiveFormat::Zip => zip_format::write(&temporary, &root, &members, mtime),
        ArchiveFormat::TarGz => tar_format::write(&temporary, &root, &members, mtime),
    }
    .and_then(|()| {
        fs::rename(&temporary, out).map_err(|error| {
            BundleError::io(format!("cannot move archive to `{}`", out.display()), error)
        })
    });
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let (sha256, size) = sha256_file(out)
        .map_err(|error| BundleError::io(format!("cannot read `{}`", out.display()), error))?;
    Ok(ArchiveSummary {
        path: out.to_path_buf(),
        sha256,
        size,
        entries: members.len() + 1,
    })
}

/// Limits applied while extracting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtractLimits {
    pub max_entries: usize,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        Self {
            max_entries: 16_384,
            max_total_bytes: 4 * 1024 * 1024 * 1024,
            max_file_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

/// Extract `archive` into the empty directory `destination` and return the bundle
/// directory (the archive's single top-level directory, which must hold `bundle.json`).
/// The extracted files are not verified here; install verifies them against the manifest.
pub fn extract_archive(
    archive: &Path,
    destination: &Path,
    limits: &ExtractLimits,
) -> Result<PathBuf, BundleError> {
    let format = ArchiveFormat::detect(archive).ok_or_else(|| {
        BundleError::Archive(format!(
            "`{}` is not a .zip or .tar.gz file",
            archive.display()
        ))
    })?;
    let mut sink = Extractor {
        destination: destination.to_path_buf(),
        limits: *limits,
        entries: 0,
        total_bytes: 0,
        root: None,
        seen: HashSet::new(),
    };
    match format {
        ArchiveFormat::Zip => zip_format::extract(archive, &mut sink)?,
        ArchiveFormat::TarGz => tar_format::extract(archive, &mut sink)?,
    }
    let root = sink
        .root
        .ok_or_else(|| BundleError::Archive("the archive is empty".into()))?;
    let bundle_dir = destination.join(&root);
    if !bundle_dir.join(MANIFEST_FILE_NAME).is_file() {
        return Err(BundleError::Archive(format!(
            "`{root}/{MANIFEST_FILE_NAME}` is missing"
        )));
    }
    Ok(bundle_dir)
}

/// Validating writer shared by both formats.
pub(crate) struct Extractor {
    destination: PathBuf,
    limits: ExtractLimits,
    entries: usize,
    total_bytes: u64,
    root: Option<String>,
    seen: HashSet<String>,
}

impl Extractor {
    fn admit(&mut self, raw: &str) -> Result<PathBuf, BundleError> {
        self.entries += 1;
        if self.entries > self.limits.max_entries {
            return Err(BundleError::Archive(format!(
                "more than {} entries",
                self.limits.max_entries
            )));
        }
        validate_bundle_path(raw).map_err(BundleError::Archive)?;
        let (root, _) = raw.split_once('/').unwrap_or((raw, ""));
        match &self.root {
            None => self.root = Some(root.to_owned()),
            Some(existing) if existing == root => {}
            Some(existing) => {
                return Err(BundleError::Archive(format!(
                    "entries must share one top-level directory (`{existing}` and `{root}`)"
                )));
            }
        }
        if !self.seen.insert(fold(raw)) {
            return Err(BundleError::Archive(format!(
                "`{raw}` appears more than once (names are compared case-insensitively)"
            )));
        }
        Ok(join_relative(&self.destination, raw))
    }

    pub(crate) fn directory(&mut self, raw: &str) -> Result<(), BundleError> {
        let raw = raw.strip_suffix('/').unwrap_or(raw);
        let path = self.admit(raw)?;
        fs::create_dir_all(&path)
            .map_err(|error| BundleError::io(format!("cannot create `{}`", path.display()), error))
    }

    pub(crate) fn file(
        &mut self,
        raw: &str,
        declared_size: u64,
        reader: &mut dyn Read,
    ) -> Result<(), BundleError> {
        let path = self.admit(raw)?;
        if !raw.contains('/') {
            return Err(BundleError::Archive(format!(
                "`{raw}` is not inside the bundle's top-level directory"
            )));
        }
        if declared_size > self.limits.max_file_bytes
            || self.total_bytes + declared_size > self.limits.max_total_bytes
        {
            return Err(BundleError::Archive(format!(
                "`{raw}` exceeds the extraction size limits"
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                BundleError::io(format!("cannot create `{}`", parent.display()), error)
            })?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                BundleError::io(format!("cannot create `{}`", path.display()), error)
            })?;
        let written = std::io::copy(&mut reader.take(declared_size + 1), &mut output)
            .and_then(|written| output.sync_all().map(|()| written))
            .map_err(|error| BundleError::Archive(format!("cannot extract `{raw}`: {error}")))?;
        output.flush().ok();
        if written != declared_size {
            return Err(BundleError::Archive(format!(
                "`{raw}` holds {written} bytes but declares {declared_size}"
            )));
        }
        self.total_bytes += written;
        Ok(())
    }
}
