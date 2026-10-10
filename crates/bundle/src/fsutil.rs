//! Crash-safe file helpers: synced copies, atomic replacement, and directory fsync.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::BundleError;

/// A unique sibling path for temporary data (`.<name>.<tag>-<pid>-<nanos>`).
pub(crate) fn temp_sibling(path: &Path, tag: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_file_name(format!(".{name}.{tag}-{}-{nanos}", process::id()))
}

/// Replace `path` with `bytes`: write a temporary sibling, fsync it, rename it over
/// `path`, then fsync the directory (best effort). Readers see old or new content.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), BundleError> {
    write_atomic_with(path, bytes, &mut |_| Ok(()))
}

/// [`write_atomic`] that calls `before_rename` with the synced temporary file just
/// before it is renamed over `path`. An error from the hook leaves the temporary file
/// in place (as a crash at that point would) and is returned; fault-injection tests
/// use it to simulate torn writes.
pub fn write_atomic_with(
    path: &Path,
    bytes: &[u8],
    before_rename: &mut dyn FnMut(&Path) -> Result<(), BundleError>,
) -> Result<(), BundleError> {
    let temporary = temp_sibling(path, "tmp");
    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(BundleError::io(
            format!("cannot write `{}`", path.display()),
            error,
        ));
    }
    before_rename(&temporary)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(BundleError::io(
            format!("cannot write `{}`", path.display()),
            error,
        ));
    }
    if let Some(parent) = path.parent() {
        sync_dir(parent);
    }
    Ok(())
}

/// Temporary siblings of `file_name` in `dir` left behind by interrupted
/// [`write_atomic`] calls (`.<file_name>.<tag>-<pid>-<nanos>`).
pub fn stale_temporaries(dir: &Path, file_name: &str) -> io::Result<Vec<PathBuf>> {
    let prefix = format!(".{file_name}.");
    let mut found = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(found),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let is_temporary = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(&prefix));
        if is_temporary && entry.file_type()?.is_file() {
            found.push(entry.path());
        }
    }
    found.sort();
    Ok(found)
}

/// Copy `source` to a new file `destination` (which must not exist), fsync it, and
/// apply the executable bit on Unix.
pub(crate) fn copy_synced(
    source: &Path,
    destination: &Path,
    executable: bool,
) -> Result<(), BundleError> {
    let result = (|| {
        let mut input = File::open(source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        drop(output);
        set_executable(destination, executable)
    })();
    result.map_err(|error| {
        BundleError::io(
            format!(
                "cannot copy `{}` to `{}`",
                source.display(),
                destination.display()
            ),
            error,
        )
    })
}

/// Set mode 0755 (executable) or 0644 on Unix; no-op on Windows.
pub(crate) fn set_executable(path: &Path, executable: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, executable);
        Ok(())
    }
}

/// Flush directory metadata after a rename (Unix; best effort, no-op on Windows).
pub fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        let _ = File::open(dir).and_then(|handle| handle.sync_all());
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}
