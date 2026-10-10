//! Small filesystem helpers: hashing, bounded reads, and crash-safe writes.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use sha2::{Digest, Sha256};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Lowercase hexadecimal SHA-256 of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Whether `path` exists as a symbolic link (or junction).
pub(crate) fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// Read a regular, non-link file of at most `max` bytes; `Ok(None)` when missing.
pub(crate) fn read_bounded(path: &Path, max: u64) -> io::Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{}` is not a regular file", path.display()),
        ));
    }
    if metadata.len() > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("`{}` is larger than {max} bytes", path.display()),
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("`{}` is larger than {max} bytes", path.display()),
        ));
    }
    Ok(Some(bytes))
}

/// Replace `path` with `bytes`: exclusive temporary file in the same directory,
/// fsync, rename, then fsync the directory (best effort on Windows).
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if is_link(path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to replace symlinked `{}`", path.display()),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_string_lossy();
    let temporary = parent.join(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_writes_replace_and_bounded_reads_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.toml");
        assert_eq!(read_bounded(&path, 10).unwrap(), None);
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(read_bounded(&path, 10).unwrap().unwrap(), b"two");
        assert!(read_bounded(&path, 2).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
