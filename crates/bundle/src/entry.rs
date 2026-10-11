//! Drive-root launch entries: the launcher marker, the Unix `aidex` entry script, and
//! recognition of files that AiDEX may replace.
//!
//! The installer only replaces a root entry that is absent or recognized as an AiDEX
//! launcher, so user files that happen to be named `AiDEX.exe` or `aidex` are never
//! overwritten.

use std::{
    fs::{self, File},
    io::{self, ErrorKind, Read},
    path::Path,
};

use crate::{BundleError, layout::LAUNCHER_BINARY, layout::LAUNCHER_DIR};

/// Byte string embedded in every AiDEX launcher binary and entry script. A drive-root
/// file is treated as an AiDEX launcher only when it contains this marker.
pub const LAUNCHER_MARKER: &str = "aidex-portable-launcher/v1:5b0f4c1e-8d27-4a63-b9e2-7c1d0a6f3e58";

/// Upper bound on how much of a file is scanned for [`LAUNCHER_MARKER`].
const MAX_SCAN_BYTES: u64 = 256 * 1024 * 1024;

/// Whether `path` is a regular file containing [`LAUNCHER_MARKER`]. Links and
/// non-files are never launchers; a missing path returns `Ok(false)`.
pub fn is_aidex_launcher(path: &Path) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Ok(false);
    }
    contains_marker(File::open(path)?.take(MAX_SCAN_BYTES))
}

fn contains_marker(mut reader: impl Read) -> io::Result<bool> {
    let marker = LAUNCHER_MARKER.as_bytes();
    let mut window: Vec<u8> = Vec::with_capacity(64 * 1024 + marker.len());
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) => return Ok(false),
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        window.extend_from_slice(&chunk[..read]);
        if window
            .windows(marker.len())
            .any(|candidate| candidate == marker)
        {
            return Ok(true);
        }
        let keep = marker.len().saturating_sub(1).min(window.len());
        window.drain(..window.len() - keep);
    }
}

/// The POSIX `sh` script written to `<drive>/aidex` for Linux and macOS hosts. It maps
/// `uname` to a target and `exec`s `<app>/launcher/<target>/aidex-launcher`, which verifies the
/// bundle. `app_relative` is the drive manifest's `app` path.
pub fn unix_entry_script(app_relative: &str) -> Result<String, BundleError> {
    let safe = !app_relative.is_empty()
        && app_relative
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
    if !safe {
        return Err(BundleError::InvalidSource(format!(
            "the drive's app directory `{app_relative}` cannot be referenced from the launcher script; use [A-Za-z0-9._/-]"
        )));
    }
    Ok(format!(
        r#"#!/bin/sh
# {LAUNCHER_MARKER}
# AiDEX portable entry for Linux and macOS. Written by the AiDEX bundle installer and
# replaced on every install; edits are lost. It selects this host's launcher, which
# verifies the installed bundle before running the aidex CLI from this drive.
set -eu
case "$(uname -s) $(uname -m)" in
  "Linux x86_64" | "Linux amd64") target=linux-x64 ;;
  "Linux aarch64" | "Linux arm64") target=linux-arm64 ;;
  "Darwin arm64") target=macos-arm64 ;;
  "Darwin x86_64") target=macos-x64 ;;
  *) echo "aidex: unsupported host: $(uname -s) $(uname -m)" >&2; exit 126 ;;
esac
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
launcher="$root/{app_relative}/{LAUNCHER_DIR}/$target/{LAUNCHER_BINARY}"
if [ ! -f "$launcher" ]; then
  echo "aidex: no AiDEX bundle for $target is installed on this drive" >&2
  exit 127
fi
if [ ! -x "$launcher" ]; then
  echo "aidex: $launcher is not executable; the drive may be mounted without exec permission (remount it with exec)" >&2
  exit 126
fi
exec "$launcher" "$@"
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_is_found_across_chunk_boundaries() {
        let mut data = vec![b'x'; 64 * 1024 - 10];
        data.extend_from_slice(LAUNCHER_MARKER.as_bytes());
        data.extend_from_slice(&[0; 100]);
        assert!(contains_marker(&data[..]).unwrap());
        assert!(!contains_marker(&data[..64 * 1024]).unwrap());
        assert!(!contains_marker(&b"MZ plain binary"[..]).unwrap());
    }

    #[test]
    fn entry_script_is_marked_and_rejects_unsafe_app_paths() {
        let script = unix_entry_script("app").unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(contains_marker(script.as_bytes()).unwrap());
        assert!(script.contains("$root/app/launcher/$target/aidex-launcher"));
        for bad in ["", "app\"; rm -rf /", "app$(x)", "a b", "app'"] {
            assert!(unix_entry_script(bad).is_err(), "{bad:?}");
        }
    }
}
