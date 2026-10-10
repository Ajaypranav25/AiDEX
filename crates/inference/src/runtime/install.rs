//! Installing a pinned runtime archive into `runtime/<platform>/llama/<version>-<variant>/`.
//!
//! The archive must match its catalog entry (size and SHA-256) before it is
//! opened. Entries are extracted into a fresh staging directory with every name
//! validated (no absolute paths, drive prefixes, `..`, reserved Windows names,
//! alternate data streams, links, devices, duplicates, or encryption), bounded
//! counts and sizes, and exclusive file creation. The manifest is written last
//! and the staging directory is renamed into place, so a crash leaves only a
//! `.staging-*` directory that the next install removes.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
};

use aidex_core::{CancelToken, Timestamp};
use aidex_drive::DrivePaths;
use sha2::{Digest, Sha256};

use super::{
    ArchiveFormat, CatalogEntry, ENGINE_DIR, InstalledRuntime, MANIFEST_FILE, MANIFEST_VERSION,
    ManifestFile, RuntimeCatalog, RuntimeManifest,
    installed::{MAX_RUNTIME_FILES, read_manifest, tree_sha256},
};
use crate::{
    InferenceError, Result,
    fsutil::{self, display},
};

/// Largest total extracted size.
const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Longest entry name.
const MAX_NAME_LEN: usize = 512;
/// Directory nesting accepted inside an archive.
const MAX_DEPTH: usize = 16;

const S_IFMT: u32 = 0o170_000;
const S_IFREG: u32 = 0o100_000;
const S_IFDIR: u32 = 0o040_000;

/// Progress of an install.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallPhase {
    Hashing { done: u64, total: u64 },
    Extracting { files: usize },
}

/// Install `archive`, which must be the archive `expected` pins.
pub fn install_runtime(
    paths: &DrivePaths,
    archive: &Path,
    expected: &CatalogEntry,
) -> Result<InstalledRuntime> {
    install_runtime_with(paths, archive, expected, &mut |_| {}, &CancelToken::new())
}

/// Hash `archive` and install it if it is a pinned entry of `catalog`.
pub fn install_from_catalog(
    paths: &DrivePaths,
    archive: &Path,
    catalog: &RuntimeCatalog,
    progress: &mut dyn FnMut(InstallPhase),
    cancel: &CancelToken,
) -> Result<InstalledRuntime> {
    let metadata = fsutil::regular_file(archive, "runtime archive")?;
    let total = metadata.len();
    let (sha256, _) = fsutil::sha256_file(
        archive,
        &mut |done| progress(InstallPhase::Hashing { done, total }),
        cancel,
    )?;
    let entry =
        catalog
            .find_by_sha256(&sha256)
            .ok_or_else(|| InferenceError::RuntimeNotInCatalog {
                sha256: sha256.clone(),
            })?;
    install_hashed(paths, archive, entry, (&sha256, total), progress, cancel)
}

pub fn install_runtime_with(
    paths: &DrivePaths,
    archive: &Path,
    expected: &CatalogEntry,
    progress: &mut dyn FnMut(InstallPhase),
    cancel: &CancelToken,
) -> Result<InstalledRuntime> {
    let metadata = fsutil::regular_file(archive, "runtime archive")?;
    let total = metadata.len();
    let (sha256, size) = fsutil::sha256_file(
        archive,
        &mut |done| progress(InstallPhase::Hashing { done, total }),
        cancel,
    )?;
    install_hashed(paths, archive, expected, (&sha256, size), progress, cancel)
}

/// Install an archive whose SHA-256 and size were just measured.
fn install_hashed(
    paths: &DrivePaths,
    archive: &Path,
    expected: &CatalogEntry,
    (sha256, size): (&str, u64),
    progress: &mut dyn FnMut(InstallPhase),
    cancel: &CancelToken,
) -> Result<InstalledRuntime> {
    if size != expected.size_bytes || !sha256.eq_ignore_ascii_case(&expected.sha256) {
        return Err(InferenceError::HashMismatch {
            what: format!("runtime archive {}", expected.archive),
            expected: format!("{} ({} bytes)", expected.sha256, expected.size_bytes),
            actual: format!("{sha256} ({size} bytes)"),
        });
    }
    if expected.format != ArchiveFormat::Zip {
        return Err(InferenceError::UnsupportedArchiveFormat {
            format: expected.format.as_str().into(),
        });
    }

    let engine_dir =
        paths.ensure_dir(&paths.runtime().join(&expected.platform).join(ENGINE_DIR))?;
    let target = engine_dir.join(expected.install_name());
    let relative_dir = fsutil::drive_relative(paths.root(), &target)?;
    if fs::symlink_metadata(&target).is_ok() {
        fsutil::refuse_symlink(&target)?;
        // Re-installing the same archive is idempotent once its files verify.
        let manifest = read_manifest(&target)?;
        if manifest
            .archive_sha256
            .eq_ignore_ascii_case(&expected.sha256)
        {
            let installed = InstalledRuntime {
                dir: target,
                relative_dir,
                manifest,
            };
            let catalog = RuntimeCatalog::from_entries(vec![expected.clone()])?;
            installed.verify(&catalog, cancel)?;
            return Ok(installed);
        }
        return Err(InferenceError::AlreadyExists {
            id: expected.install_name(),
            path: target,
        });
    }
    remove_stale_staging(&engine_dir);

    let staging = engine_dir.join(format!(".staging-{}", fsutil::random_suffix()?));
    fs::create_dir(&staging).map_err(|error| {
        InferenceError::io(format!("cannot create {}", display(&staging)), &error)
    })?;
    let result = (|| {
        let files = extract_zip(archive, &staging, expected, progress, cancel)?;
        if !files.iter().any(|file| file.path == expected.server_binary) {
            return Err(InferenceError::UnsafeArchive {
                entry: expected.server_binary.clone(),
                reason: "the archive does not contain the server binary".into(),
            });
        }
        let tree = tree_sha256(&files);
        if let Some(pinned) = &expected.tree_sha256 {
            if !pinned.eq_ignore_ascii_case(&tree) {
                return Err(InferenceError::HashMismatch {
                    what: format!("contents of {}", expected.archive),
                    expected: pinned.clone(),
                    actual: tree,
                });
            }
        }
        let manifest = RuntimeManifest {
            version: MANIFEST_VERSION,
            catalog_id: expected.id.clone(),
            engine: expected.engine.clone(),
            runtime_version: expected.version.clone(),
            platform: expected.platform.clone(),
            variant: expected.variant,
            archive: expected.archive.clone(),
            archive_sha256: sha256.to_ascii_lowercase(),
            url: expected.url.clone(),
            source: expected.source.clone(),
            license: expected.license.clone(),
            server_binary: expected.server_binary.clone(),
            files,
            tree_sha256: tree,
            installed_at: Timestamp::now(),
        };
        let json = serde_json::to_vec_pretty(&manifest).map_err(|error| InferenceError::Io {
            context: "cannot encode runtime manifest".into(),
            message: error.to_string(),
        })?;
        fsutil::write_atomic(&staging.join(MANIFEST_FILE), &json)?;
        fs::rename(&staging, &target).map_err(|error| {
            InferenceError::io(
                format!("cannot move runtime into {}", display(&target)),
                &error,
            )
        })?;
        fsutil::sync_dir(&engine_dir);
        Ok(manifest)
    })();
    match result {
        Ok(manifest) => Ok(InstalledRuntime {
            dir: target,
            relative_dir,
            manifest,
        }),
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

/// Remove `.staging-*` directories an interrupted install left behind.
fn remove_stale_staging(engine_dir: &Path) {
    if let Ok(read) = fs::read_dir(engine_dir) {
        for item in read.flatten() {
            let name = item.file_name();
            if name.to_string_lossy().starts_with(".staging-")
                && fs::symlink_metadata(item.path()).is_ok_and(|meta| meta.is_dir())
            {
                let _ = fs::remove_dir_all(item.path());
            }
        }
    }
}

/// Validate one archive entry name; returns its `/`-separated components.
pub(crate) fn safe_entry_path(name: &str) -> std::result::Result<Vec<String>, String> {
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return Err("name is empty or too long".into());
    }
    if name.contains('\0') || name.chars().any(char::is_control) {
        return Err("name contains control characters".into());
    }
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err("absolute paths are not allowed".into());
    }
    let trimmed = normalized.trim_end_matches('/');
    let mut parts = Vec::new();
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." {
            return Err("empty or `.` path segments are not allowed".into());
        }
        if part == ".." {
            return Err("`..` would escape the install directory".into());
        }
        if part.contains([':', '<', '>', '"', '|', '?', '*']) {
            return Err("drive prefixes, streams, and wildcard characters are not allowed".into());
        }
        if part.ends_with(['.', ' ']) {
            return Err("names ending in `.` or space alias other files on Windows".into());
        }
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
            || ((stem.starts_with("com") || stem.starts_with("lpt"))
                && stem.len() == 4
                && stem.as_bytes()[3].is_ascii_digit())
        {
            return Err("reserved Windows device names are not allowed".into());
        }
        if part.eq_ignore_ascii_case(MANIFEST_FILE) && parts.is_empty() {
            return Err("the archive must not contain an AiDEX manifest".into());
        }
        parts.push(part.to_owned());
    }
    if parts.len() > MAX_DEPTH {
        return Err("directories are nested too deeply".into());
    }
    Ok(parts)
}

fn extract_zip(
    archive: &Path,
    staging: &Path,
    expected: &CatalogEntry,
    progress: &mut dyn FnMut(InstallPhase),
    cancel: &CancelToken,
) -> Result<Vec<ManifestFile>> {
    let file = File::open(archive)
        .map_err(|error| InferenceError::io(format!("cannot open {}", display(archive)), &error))?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file)).map_err(|error| {
        InferenceError::UnsafeArchive {
            entry: expected.archive.clone(),
            reason: format!("not a readable zip archive: {error}"),
        }
    })?;
    if zip.len() > MAX_RUNTIME_FILES {
        return Err(InferenceError::UnsafeArchive {
            entry: expected.archive.clone(),
            reason: format!(
                "{} entries exceed the limit of {MAX_RUNTIME_FILES}",
                zip.len()
            ),
        });
    }
    let mut seen = BTreeSet::new();
    let mut files = Vec::new();
    let mut written_total = 0u64;
    for index in 0..zip.len() {
        if cancel.is_stopped() {
            return Err(InferenceError::Cancelled);
        }
        let mut entry = zip
            .by_index(index)
            .map_err(|error| InferenceError::UnsafeArchive {
                entry: format!("#{index}"),
                reason: format!("unreadable entry: {error}"),
            })?;
        let name = entry.name().to_owned();
        let unsafe_entry = |reason: String| InferenceError::UnsafeArchive {
            entry: name.chars().take(200).collect(),
            reason,
        };
        let parts = safe_entry_path(&name).map_err(unsafe_entry)?;
        if entry.encrypted() {
            return Err(unsafe_entry("encrypted entries are not supported".into()));
        }
        let mode = entry.unix_mode();
        let file_type = mode.map(|mode| mode & S_IFMT).filter(|kind| *kind != 0);
        let is_dir = entry.is_dir();
        match file_type {
            None => {}
            Some(S_IFDIR) if is_dir => {}
            Some(S_IFREG) if !is_dir => {}
            Some(_) => {
                return Err(unsafe_entry(
                    "symbolic links and special files are not allowed".into(),
                ));
            }
        }
        let relative = parts.join("/");
        if !seen.insert(relative.to_ascii_lowercase()) {
            return Err(unsafe_entry(
                "the name appears twice (case-insensitively)".into(),
            ));
        }
        let path = parts
            .iter()
            .fold(staging.to_path_buf(), |path, part| path.join(part));
        if is_dir {
            create_dirs(staging, &parts)?;
            continue;
        }
        if parts.len() > 1 {
            create_dirs(staging, &parts[..parts.len() - 1])?;
        }
        let remaining = MAX_TOTAL_BYTES.saturating_sub(written_total);
        let (sha256, size) = write_entry(&mut entry, &path, remaining)?;
        written_total = written_total.saturating_add(size);
        if size > remaining {
            return Err(unsafe_entry(format!(
                "extracted size exceeds the limit of {MAX_TOTAL_BYTES} bytes"
            )));
        }
        set_permissions(&path, mode, relative == expected.server_binary)?;
        files.push(ManifestFile {
            path: relative,
            size_bytes: size,
            sha256,
        });
        progress(InstallPhase::Extracting { files: files.len() });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn create_dirs(staging: &Path, parts: &[String]) -> Result<()> {
    let mut current = staging.to_path_buf();
    for part in parts {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(InferenceError::UnsafeArchive {
                    entry: parts.join("/"),
                    reason: "a file and a directory share a name".into(),
                });
            }
            Err(_) => fs::create_dir(&current).map_err(|error| {
                InferenceError::io(format!("cannot create {}", display(&current)), &error)
            })?,
        }
    }
    Ok(())
}

/// Copy at most `limit` bytes of `reader` into the new file `path`.
fn write_entry(reader: &mut impl Read, path: &Path, limit: u64) -> Result<(String, u64)> {
    let mut output = fsutil::create_new(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    let mut limited = reader.take(limit.saturating_add(1));
    loop {
        let read = limited
            .read(&mut buffer)
            .map_err(|error| InferenceError::UnsafeArchive {
                entry: display(path),
                reason: format!("cannot decompress: {error}"),
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        output.write_all(&buffer[..read]).map_err(|error| {
            InferenceError::io(format!("cannot write {}", display(path)), &error)
        })?;
        total += read as u64;
    }
    output
        .sync_all()
        .map_err(|error| InferenceError::io(format!("cannot sync {}", display(path)), &error))?;
    Ok((fsutil::hex(&hasher.finalize()), total))
}

#[cfg(unix)]
fn set_permissions(path: &Path, mode: Option<u32>, server: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let executable = server || mode.is_some_and(|mode| mode & 0o111 != 0);
    let bits = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(bits)).map_err(|error| {
        InferenceError::io(format!("cannot set mode of {}", display(path)), &error)
    })
}

#[cfg(not(unix))]
fn set_permissions(_: &Path, _: Option<u32>, _: bool) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_that_escape_or_alias_are_rejected() {
        for bad in [
            "../evil.dll",
            "a/../../evil",
            "/etc/passwd",
            "\\windows\\evil.dll",
            "C:/evil.dll",
            "C:evil",
            "dir\\..\\..\\evil",
            "file.dll:stream",
            "CON",
            "nul.txt",
            "com1.dll",
            "evil. ",
            "evil.",
            "a//b",
            "./a",
            "",
            "aidex-runtime.json",
        ] {
            assert!(safe_entry_path(bad).is_err(), "{bad:?} was accepted");
        }
        assert_eq!(
            safe_entry_path("bin\\llama-server.exe").unwrap(),
            ["bin", "llama-server.exe"]
        );
        assert_eq!(safe_entry_path("lib/").unwrap(), ["lib"]);
        assert_eq!(safe_entry_path("console.dll").unwrap(), ["console.dll"]);
        assert_eq!(safe_entry_path("sub/aidex-runtime.json").unwrap().len(), 2);
    }
}
