//! Names of the bundle layout inside a bundle directory and inside the drive's `app/`
//! directory. See `portable/packaging/README.md` for the full layout.

use std::path::{Path, PathBuf};

use aidex_drive::DrivePaths;

use crate::Target;

/// `app/versions/<version>-<target>/`: one immutable directory per installed bundle.
pub const VERSIONS_DIR: &str = "versions";
/// `app/staging/<id>/`: scratch space for an install in progress (same volume, so the
/// finished bundle is moved into `versions/` with a rename).
pub const STAGING_DIR: &str = "staging";
/// `app/launcher/<target>/`: launcher binaries for Unix targets (the root `aidex`
/// script selects one by `uname`).
pub const LAUNCHER_DIR: &str = "launcher";
/// Executables inside a bundle directory.
pub const BIN_DIR: &str = "bin";
/// Third-party and AiDEX license texts inside a bundle directory.
pub const LICENSES_DIR: &str = "licenses";

/// Executable stem of the `aidex` CLI.
pub const CLI_BINARY: &str = "aidex";
/// Executable stem of the daemon.
pub const DAEMON_BINARY: &str = "aidexd";
/// Executable stem of the sandbox helper (must sit next to `aidexd`).
pub const HELPER_BINARY: &str = "aidex-sandbox-exec";
/// Executable stem of the launcher binary inside a bundle and under `app/launcher/`.
/// (It cannot be `AiDEX` there: `AiDEX.exe` and `aidex.exe` are the same file on
/// case-insensitive filesystems.) Installs copy it to the drive root as `AiDEX.exe`.
pub const LAUNCHER_BINARY: &str = "aidex-launcher";

/// Launcher at the drive root for Windows targets.
pub const WINDOWS_ROOT_LAUNCHER: &str = "AiDEX.exe";
/// POSIX `sh` entry script at the drive root for Linux and macOS targets.
pub const UNIX_ROOT_LAUNCHER: &str = "aidex";
/// Suffix of the launcher kept when an install replaces a root launcher.
pub const PREVIOUS_SUFFIX: &str = ".previous";

/// `bin/<stem><suffix>` for `target`.
pub fn bin_path(stem: &str, target: Target) -> String {
    format!("{BIN_DIR}/{}", target.executable_name(stem))
}

/// Where the launcher for `target` lives on a drive: `AiDEX.exe` at the root on
/// Windows, `app/launcher/<target>/aidex-launcher` elsewhere.
pub fn launcher_path(paths: &DrivePaths, target: Target) -> PathBuf {
    if target.is_windows() {
        paths.root().join(WINDOWS_ROOT_LAUNCHER)
    } else {
        unix_launcher_dir(paths, target).join(LAUNCHER_BINARY)
    }
}

/// `app/launcher/<target>/`.
pub fn unix_launcher_dir(paths: &DrivePaths, target: Target) -> PathBuf {
    paths.app().join(LAUNCHER_DIR).join(target.as_str())
}

/// The drive's bundle pointer, `app/current.json`.
pub fn pointer_path(paths: &DrivePaths) -> PathBuf {
    paths.app().join(crate::POINTER_FILE_NAME)
}

/// `app/versions/`.
pub fn versions_dir(paths: &DrivePaths) -> PathBuf {
    paths.app().join(VERSIONS_DIR)
}

/// `path` with `suffix` appended to its file name (`AiDEX.exe` → `AiDEX.exe.previous`).
pub(crate) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}
