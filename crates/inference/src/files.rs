use crate::InferenceError;
use aidex_core::{CancelToken, EntityId};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub(crate) fn check_cancel(cancel: &CancelToken) -> Result<(), InferenceError> {
    if cancel.is_stopped() {
        Err(InferenceError::Cancelled)
    } else {
        Ok(())
    }
}

pub(crate) fn is_link(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

/// Refuse links/reparse points in every existing component, including source parents.
pub(crate) fn safe_path(path: &Path) -> Result<(), InferenceError> {
    if !path.is_absolute() {
        return Err(InferenceError::Invalid(
            "source must be an absolute path".into(),
        ));
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir) {
            return Err(InferenceError::Invalid("path traversal refused".into()));
        }
        current.push(part);
        // A Windows drive/UNC prefix is incomplete until its RootDir follows.
        // Canonical portable paths use \\?\H:\; querying \\?\H: alone fails.
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let meta = fs::symlink_metadata(&current)?;
        if is_link(&meta) {
            return Err(InferenceError::Integrity(
                "symbolic links and reparse points are refused".into(),
            ));
        }
    }
    Ok(())
}
pub(crate) fn regular(path: &Path) -> Result<fs::Metadata, InferenceError> {
    safe_path(path)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() {
        return Err(InferenceError::Invalid("expected a regular file".into()));
    }
    Ok(meta)
}
pub(crate) fn segment(s: &str) -> Result<(), InferenceError> {
    if s.is_empty()
        || s.len() > 128
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || s.starts_with('.')
        || s.ends_with('.')
        || matches!(
            s.split('.')
                .next()
                .unwrap_or("")
                .to_ascii_uppercase()
                .as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    {
        return Err(InferenceError::Invalid("unsafe filename".into()));
    }
    Ok(())
}
pub(crate) fn digest(path: &Path, cancel: &CancelToken) -> Result<(String, u64), InferenceError> {
    regular(path)?;
    let mut f = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buf = vec![0; 1024 * 1024];
    loop {
        check_cancel(cancel)?;
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        bytes += n as u64;
    }
    Ok((format!("{:x}", hash.finalize()), bytes))
}
pub(crate) fn copy(
    source: &Path,
    target: &Path,
    cancel: &CancelToken,
) -> Result<(String, u64), InferenceError> {
    let metadata = regular(source)?;
    let mut src = File::open(source)?;
    let mut dst = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buf = vec![0; 1024 * 1024];
    loop {
        check_cancel(cancel)?;
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n])?;
        hash.update(&buf[..n]);
        size += n as u64;
    }
    check_cancel(cancel)?;
    dst.sync_all()?;
    let after = regular(source)?;
    if size != metadata.len()
        || metadata.len() != after.len()
        || metadata.modified().ok() != after.modified().ok()
    {
        return Err(InferenceError::Integrity(
            "source changed during copy".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            target,
            fs::Permissions::from_mode(metadata.permissions().mode()),
        )?;
    }
    Ok((format!("{:x}", hash.finalize()), size))
}
pub(crate) fn free_space(path: &Path, needed: u64) -> Result<(), InferenceError> {
    let free = fs2::available_space(path)?;
    if free < needed.saturating_add(16 * 1024 * 1024) {
        return Err(InferenceError::Unavailable(
            "insufficient free space on selected drive".into(),
        ));
    }
    Ok(())
}
pub(crate) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, InferenceError> {
    if regular(path)?.len() > 4 * 1024 * 1024 {
        return Err(InferenceError::Integrity("metadata too large".into()));
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub(crate) fn write_json<T: serde::Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), InferenceError> {
    if path.exists() {
        regular(path)?;
    }
    let temp = path.with_extension(format!("{}.tmp", unique()?));
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        f.write_all(&serde_json::to_vec_pretty(value)?)?;
        f.sync_all()?;
        rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
/// `fs::rename`, retried briefly on Windows while another process (typically an
/// antivirus scanner inspecting a freshly copied executable or model) still holds
/// a handle and the move fails with access denied or a sharing violation.
pub(crate) fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    const ATTEMPTS: u64 = 40;
    let mut attempt = 0;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if cfg!(windows)
                    && attempt < ATTEMPTS
                    && (error.kind() == std::io::ErrorKind::PermissionDenied
                        || error.raw_os_error() == Some(32)) =>
            {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(50 * attempt.min(5)));
            }
            Err(error) => return Err(error),
        }
    }
}
pub(crate) fn unique() -> Result<String, InferenceError> {
    EntityId::generate("stage")
        .map(|id| id.to_string())
        .map_err(|e| InferenceError::Io(e.to_string()))
}

/// Staging directories are generated under managed parents; remove only their flat,
/// regular files. Unexpected nested directories/links fail closed.
pub(crate) fn remove_flat(dir: &Path) -> Result<(), InferenceError> {
    safe_path(dir)?;
    let files = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;
    for f in &files {
        regular(f)?;
    }
    for f in files {
        fs::remove_file(f)?;
    }
    fs::remove_dir(dir)?;
    Ok(())
}
pub(crate) struct Stage(pub PathBuf);
impl Stage {
    pub fn new(parent: &Path) -> Result<Self, InferenceError> {
        let path = parent.join(format!(".{}", unique()?));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = remove_flat(&self.0);
        }
    }
}

#[cfg(all(test, windows))]
mod windows_path_tests {
    use super::*;
    #[test]
    fn canonical_windows_paths_are_checked_after_the_drive_root() {
        let temp = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(temp.path()).unwrap();
        assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
        safe_path(&canonical).expect("verbatim drive prefix alone is not a filesystem entry");
        let file = canonical.join("fixture.txt");
        std::fs::write(&file, b"fixture").unwrap();
        assert_eq!(regular(&file).unwrap().len(), 7);
    }
}
