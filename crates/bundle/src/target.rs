//! Bundle targets: the normalized `<os>-<arch>` names also used for `runtime/<os>-<arch>`.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// A portable bundle platform. Serialized as its normalized name (`windows-x64`, ...).
///
/// Listing a target here does not claim that it is supported; see
/// `portable/packaging/README.md` for which targets are built and smoke-tested.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    WindowsX64,
    LinuxX64,
    LinuxArm64,
    MacosArm64,
    MacosX64,
}

impl Target {
    /// Every target in stable order.
    pub const ALL: [Target; 5] = [
        Target::WindowsX64,
        Target::LinuxX64,
        Target::LinuxArm64,
        Target::MacosArm64,
        Target::MacosX64,
    ];

    /// The normalized name, identical to `aidex_drive::platform_runtime_name()` values.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WindowsX64 => "windows-x64",
            Self::LinuxX64 => "linux-x64",
            Self::LinuxArm64 => "linux-arm64",
            Self::MacosArm64 => "macos-arm64",
            Self::MacosX64 => "macos-x64",
        }
    }

    /// The target this binary was compiled for, or `None` on unnamed platforms.
    pub fn host() -> Option<Self> {
        aidex_drive::platform_runtime_name().and_then(|name| name.parse().ok())
    }

    pub fn is_windows(self) -> bool {
        matches!(self, Self::WindowsX64)
    }

    /// `.exe` on Windows targets, empty elsewhere.
    pub fn executable_suffix(self) -> &'static str {
        if self.is_windows() { ".exe" } else { "" }
    }

    /// `stem` plus the target's executable suffix.
    pub fn executable_name(self, stem: &str) -> String {
        format!("{stem}{}", self.executable_suffix())
    }

    /// The Rust target triple the packager builds by default. Linux uses musl so the
    /// binaries are statically linked and do not depend on the host's glibc version.
    pub fn default_rust_triple(self) -> &'static str {
        match self {
            Self::WindowsX64 => "x86_64-pc-windows-msvc",
            Self::LinuxX64 => "x86_64-unknown-linux-musl",
            Self::LinuxArm64 => "aarch64-unknown-linux-musl",
            Self::MacosArm64 => "aarch64-apple-darwin",
            Self::MacosX64 => "x86_64-apple-darwin",
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error for an unrecognized target name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownTarget(pub String);

impl fmt::Display for UnknownTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let known: Vec<_> = Target::ALL.iter().map(|target| target.as_str()).collect();
        write!(
            f,
            "unknown bundle target `{}` (expected one of {})",
            self.0,
            known.join(", ")
        )
    }
}

impl std::error::Error for UnknownTarget {}

impl FromStr for Target {
    type Err = UnknownTarget;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|target| target.as_str() == value)
            .ok_or_else(|| UnknownTarget(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_through_serde_and_from_str() {
        for target in Target::ALL {
            let json = serde_json::to_string(&target).unwrap();
            assert_eq!(json, format!("\"{}\"", target.as_str()));
            assert_eq!(serde_json::from_str::<Target>(&json).unwrap(), target);
            assert_eq!(target.as_str().parse::<Target>().unwrap(), target);
        }
        assert!("windows-arm64".parse::<Target>().is_err());
    }

    #[test]
    fn host_matches_the_drive_runtime_name() {
        assert_eq!(
            Target::host().map(Target::as_str),
            aidex_drive::platform_runtime_name()
        );
    }

    #[test]
    fn executable_names_follow_the_target() {
        assert_eq!(Target::WindowsX64.executable_name("aidex"), "aidex.exe");
        assert_eq!(Target::LinuxX64.executable_name("aidex"), "aidex");
    }
}
