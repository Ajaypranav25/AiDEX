//! Crash-safe file helpers: streaming SHA-256, atomic writes, symlink refusal,
//! and path display without the Windows verbatim prefix.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use aidex_core::CancelToken;
use sha2::{Digest, Sha256};

use crate::{InferenceError, Result};

/// Read buffer for hashing and copying.
const CHUNK: usize = 1024 * 1024;

/// Lowercase hexadecimal.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// Whether `text` is a lowercase or uppercase 64-digit SHA-256 hex string.
pub(crate) fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Stream `path` through SHA-256. `progress` receives the bytes hashed so far.
pub(crate) fn sha256_file(
    path: &Path,
    progress: &mut dyn FnMut(u64),
    cancel: &CancelToken,
) -> Result<(String, u64)> {
    let mut file = File::open(path)
        .map_err(|error| InferenceError::io(format!("cannot open {}", display(path)), &error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    let mut total = 0u64;
    loop {
        if cancel.is_stopped() {
            return Err(InferenceError::Cancelled);
        }
        let read = file.read(&mut buffer).map_err(|error| {
            InferenceError::io(format!("cannot read {}", display(path)), &error)
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total = total.saturating_add(read as u64);
        progress(total);
    }
    Ok((hex(&hasher.finalize()), total))
}

/// Copy `source` into the new file `destination` (which must not exist), hashing
/// what is read, then fsync the copy.
pub(crate) fn copy_hashing(
    source: &Path,
    destination: &Path,
    progress: &mut dyn FnMut(u64),
    cancel: &CancelToken,
) -> Result<(String, u64)> {
    let mut input = File::open(source)
        .map_err(|error| InferenceError::io(format!("cannot open {}", display(source)), &error))?;
    let mut output = create_new(destination)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    let mut total = 0u64;
    loop {
        if cancel.is_stopped() {
            return Err(InferenceError::Cancelled);
        }
        let read = input.read(&mut buffer).map_err(|error| {
            InferenceError::io(format!("cannot read {}", display(source)), &error)
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        output.write_all(&buffer[..read]).map_err(|error| {
            InferenceError::io(format!("cannot write {}", display(destination)), &error)
        })?;
        total = total.saturating_add(read as u64);
        progress(total);
    }
    output.sync_all().map_err(|error| {
        InferenceError::io(format!("cannot sync {}", display(destination)), &error)
    })?;
    Ok((hex(&hasher.finalize()), total))
}

/// Create `path` exclusively (fails if anything exists there).
pub(crate) fn create_new(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| InferenceError::io(format!("cannot create {}", display(path)), &error))
}

/// Write `bytes` to `path` atomically: a sibling temp file is written, synced,
/// and renamed over `path`. Refuses a symlinked destination.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    refuse_symlink(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| InferenceError::corrupt(path, "path has no parent directory"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| InferenceError::corrupt(path, "path has no UTF-8 file name"))?;
    let temp = parent.join(format!(".{name}.{}.tmp", random_suffix()?));
    let result = (|| {
        let mut file = create_new(&temp)?;
        file.write_all(bytes).map_err(|error| {
            InferenceError::io(format!("cannot write {}", display(&temp)), &error)
        })?;
        file.sync_all().map_err(|error| {
            InferenceError::io(format!("cannot sync {}", display(&temp)), &error)
        })?;
        drop(file);
        rename(&temp, path).map_err(|error| {
            InferenceError::io(format!("cannot replace {}", display(path)), &error)
        })?;
        sync_dir(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// `fs::rename`, retried briefly on Windows when another process (typically an
/// antivirus scanner inspecting a freshly written executable) still holds a
/// handle and the move fails with "access denied" or a sharing violation.
pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    const ATTEMPTS: u32 = 40;
    let mut attempt = 0;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if cfg!(windows)
                    && attempt < ATTEMPTS
                    && (error.kind() == io::ErrorKind::PermissionDenied
                        || error.raw_os_error() == Some(32)) =>
            {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(
                    50 * u64::from(attempt.min(5)),
                ));
            }
            Err(error) => return Err(error),
        }
    }
}

/// Best-effort directory fsync so a rename survives power loss (Unix only;
/// Windows flushes directory metadata with the file).
pub(crate) fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Fail if `path` exists as a symbolic link (or junction).
pub(crate) fn refuse_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || is_reparse_point(&metadata) => Err(
            InferenceError::corrupt(path, "is a symbolic link; refusing to follow it"),
        ),
        _ => Ok(()),
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

/// Metadata of a regular, non-symlinked file.
pub(crate) fn regular_file(path: &Path, what: &str) -> Result<fs::Metadata> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || is_reparse_point(&metadata) => Err(
            InferenceError::corrupt(path, format!("{what} is a symbolic link")),
        ),
        Ok(metadata) if metadata.is_file() => Ok(metadata),
        Ok(_) => Err(InferenceError::MissingFile {
            what: what.to_owned(),
            path: path.to_path_buf(),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(InferenceError::MissingFile {
            what: what.to_owned(),
            path: path.to_path_buf(),
        }),
        Err(error) => Err(InferenceError::io(
            format!("cannot inspect {}", display(path)),
            &error,
        )),
    }
}

/// Nanoseconds since the Unix epoch of a modification time (0 when unavailable).
pub(crate) fn modified_nanos(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos())
}

/// 16 random hex characters from the operating-system RNG.
pub(crate) fn random_suffix() -> Result<String> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|error| InferenceError::Io {
        context: "operating-system randomness is unavailable".into(),
        message: error.to_string(),
    })?;
    Ok(hex(&bytes))
}

/// `path` without the Windows `\\?\` prefix when it names an ordinary drive-letter
/// path (what users and llama-server expect); unchanged otherwise.
pub(crate) fn plain_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(text) = path.to_str() {
            if let Some(rest) = text.strip_prefix(r"\\?\") {
                let bytes = rest.as_bytes();
                let drive = bytes.len() >= 3
                    && bytes[0].is_ascii_alphabetic()
                    && bytes[1] == b':'
                    && bytes[2] == b'\\';
                if drive && rest.len() < 260 && !rest.contains('/') {
                    return PathBuf::from(rest);
                }
            }
        }
        path.to_path_buf()
    }
    #[cfg(not(windows))]
    path.to_path_buf()
}

/// Display form of a path (see [`plain_path`]).
pub(crate) fn display(path: &Path) -> String {
    plain_path(path).display().to_string()
}

/// `path` relative to the drive root, written with `/` (for registry records).
pub(crate) fn drive_relative(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| InferenceError::corrupt(path, "is not inside the drive"))?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(
                part.to_str()
                    .ok_or_else(|| InferenceError::corrupt(path, "path is not UTF-8"))?
                    .to_owned(),
            ),
            _ => return Err(InferenceError::corrupt(path, "path is not drive-relative")),
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_known_vectors() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        fs::write(&path, b"abc").unwrap();
        let mut seen = 0;
        let (digest, size) =
            sha256_file(&path, &mut |done| seen = done, &CancelToken::new()).unwrap();
        assert_eq!(
            (digest.as_str(), size, seen),
            (sha256_bytes(b"abc").as_str(), 3, 3)
        );
        assert!(is_sha256_hex(&digest));
        assert!(!is_sha256_hex("abc"));
    }

    #[test]
    fn atomic_writes_replace_files_and_copies_refuse_existing_targets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entry.toml");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no temp files remain"
        );
        let copy = dir.path().join("copy");
        copy_hashing(&path, &copy, &mut |_| {}, &CancelToken::new()).unwrap();
        assert!(copy_hashing(&path, &copy, &mut |_| {}, &CancelToken::new()).is_err());
        let cancelled = CancelToken::new();
        cancelled.cancel();
        assert_eq!(
            sha256_file(&path, &mut |_| {}, &cancelled).unwrap_err(),
            InferenceError::Cancelled
        );
    }

    #[test]
    fn drive_relative_paths_use_forward_slashes() {
        let root = Path::new("root");
        assert_eq!(
            drive_relative(root, &root.join("models").join("m").join("model.gguf")).unwrap(),
            "models/m/model.gguf"
        );
        assert!(drive_relative(root, Path::new("elsewhere/x")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_prefixes_are_dropped_for_drive_paths() {
        assert_eq!(
            plain_path(Path::new(r"\\?\E:\drive\models")),
            PathBuf::from(r"E:\drive\models")
        );
        assert_eq!(
            plain_path(Path::new(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\?\UNC\server\share")
        );
    }
}
