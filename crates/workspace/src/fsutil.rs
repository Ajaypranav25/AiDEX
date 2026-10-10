//! Small filesystem helpers: hashing, crash-safe writes, and link checks.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use sha2::{Digest, Sha256};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Lowercase hexadecimal SHA-256 of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Size and SHA-256 of a regular file's contents.
pub(crate) fn hash_file(path: &Path) -> io::Result<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((size, hex(&hasher.finalize())))
}

/// Copy a regular file to a new destination (which must not exist), returning the
/// size and SHA-256 of exactly the bytes written.
pub(crate) fn copy_hashed(source: &Path, destination: &Path) -> io::Result<(u64, String)> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        output.write_all(&buffer[..read])?;
        size += read as u64;
    }
    copy_unix_mode(source, destination)?;
    Ok((size, hex(&hasher.finalize())))
}

/// Propagate the Unix permission bits (e.g. the executable bit). Windows attributes
/// such as read-only are deliberately not copied.
pub(crate) fn copy_unix_mode(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(source)?.permissions().mode() & 0o777;
        fs::set_permissions(destination, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (source, destination);
    }
    Ok(())
}

/// A uniquely named temporary file next to `destination`, created exclusively.
pub(crate) fn sibling_temp(destination: &Path) -> io::Result<(PathBuf, File)> {
    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "path has no parent"))?;
    loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".aidex-tmp-{}-{sequence}", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Write `bytes` to `destination` atomically: exclusive temp file, fsync, rename.
pub(crate) fn write_atomic(destination: &Path, bytes: &[u8]) -> io::Result<()> {
    let (temporary, mut file) = sibling_temp(destination)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, destination)?;
        sync_parent(destination);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Best-effort directory fsync so a completed rename survives power loss (Unix).
pub(crate) fn sync_parent(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Whether `path` exists as a symbolic link or junction (not following it).
pub(crate) fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// Require `path` to exist as a real directory (not a symbolic link or junction).
pub(crate) fn require_real_dir(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("`{}` is not a real directory", path.display()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_writes_replace_content_and_leave_no_temporaries() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("data.json");
        write_atomic(&target, b"one").unwrap();
        write_atomic(&target, b"two").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"two");
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["data.json"]);
    }

    #[test]
    fn hashes_match_copied_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a");
        fs::write(&source, b"hello").unwrap();
        let copied = copy_hashed(&source, &dir.path().join("b")).unwrap();
        assert_eq!(copied, (5, sha256_hex(b"hello")));
        assert_eq!(hash_file(&dir.path().join("b")).unwrap(), copied);
        assert!(copy_hashed(&source, &dir.path().join("b")).is_err());
    }
}
