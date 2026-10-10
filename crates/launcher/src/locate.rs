//! Find the drive from the launcher's own location (no autorun, no install, no
//! environment lookup: `AIDEX_DRIVE_ROOT` from the caller is ignored).

use std::{
    fs,
    path::{Path, PathBuf},
};

use aidex_bundle::{Target, layout::unix_launcher_dir};
use aidex_drive::{DrivePaths, MANIFEST_FILE_NAME};

use crate::error::LaunchError;

/// How many directories above the launcher are searched for `aidex-drive.json`
/// (`<root>/app/launcher/<target>/aidex-launcher` is three levels down).
const MAX_DEPTH: usize = 6;

/// The canonical path of the running launcher.
pub fn current_exe() -> Result<PathBuf, LaunchError> {
    let exe = std::env::current_exe()
        .map_err(|error| LaunchError::io("cannot locate the launcher executable", error))?;
    fs::canonicalize(&exe)
        .map_err(|error| LaunchError::io(format_args!("cannot resolve `{}`", exe.display()), error))
}

/// Open the drive that contains `exe`. The nearest ancestor holding
/// `aidex-drive.json` is the drive; its manifest is parsed and validated, and the
/// launcher must sit at the drive root or in `app/launcher/<target>/`.
pub fn drive_for(exe: &Path, target: Target) -> Result<DrivePaths, LaunchError> {
    let parent = exe.parent().ok_or_else(|| LaunchError::no_drive(exe))?;
    let root = parent
        .ancestors()
        .take(MAX_DEPTH)
        .find(|candidate| fs::symlink_metadata(candidate.join(MANIFEST_FILE_NAME)).is_ok())
        .ok_or_else(|| LaunchError::no_drive(exe))?;
    let paths = DrivePaths::open(root)?;
    let allowed = parent == paths.root() || parent == unix_launcher_dir(&paths, target);
    if !allowed {
        return Err(LaunchError::misplaced(exe, paths.root()));
    }
    Ok(paths)
}

/// Strip the Windows verbatim prefix from local drive paths (`\\?\H:\x` → `H:\x`) so
/// child processes and users see ordinary paths. Other paths are returned unchanged.
pub fn display_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(rest) = path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
            let bytes = rest.as_bytes();
            let is_drive_path = bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && bytes[2] == b'\\';
            if is_drive_path && rest.len() < 260 {
                return PathBuf::from(rest);
            }
        }
        path.to_path_buf()
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}
