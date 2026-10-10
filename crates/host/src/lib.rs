//! Read-only inspection of the current process host environment.

use std::{env, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostInfo {
    pub os: &'static str,
    pub architecture: &'static str,
    pub executable: Option<PathBuf>,
    pub current_dir: Option<PathBuf>,
}

impl HostInfo {
    /// Capture compile-time platform identifiers and best-effort process paths.
    pub fn current() -> Self {
        Self {
            os: env::consts::OS,
            architecture: env::consts::ARCH,
            executable: env::current_exe().ok(),
            current_dir: env::current_dir().ok(),
        }
    }
}
