//! The versioned host capability manifest.
//!
//! Default discovery never invokes programs, opens network connections, or
//! writes files. Tool availability is inferred only from executable files
//! visible in `PATH`; this does not prove that a tool can run or that a
//! capability is usable.

use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::{GpuProbeOptions, GpuStatus, HostResources, probe_gpus};

/// Current serialized host profile schema. Fields added since (physical cores,
/// available RAM) are additive.
pub const PROFILE_VERSION: u32 = 1;
/// Maximum number of PATH entries examined for each discovery.
const MAX_PATH_ENTRIES: usize = 256;

/// A capability whose state can be safely inferred from local metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", content = "path", rename_all = "snake_case")]
pub enum Probe<T> {
    /// Positive evidence was found.
    Available(T),
    /// A bounded search completed without finding the capability.
    Unavailable,
    /// No reliable non-invasive probe is available, or discovery failed.
    Unknown,
}

/// Facts about a directory observed without attempting to create or modify it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DirectoryFacts {
    /// Whether the path exists and is a directory.
    pub available: Probe<()>,
    /// Write permission is never tested by creating a file.
    pub writable: Probe<()>,
}

/// Versioned, serializable snapshot of host facts and safely probed tools.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HostProfile {
    /// Manifest schema version.
    pub version: u32,
    /// Compile-time operating-system identifier.
    pub os: &'static str,
    /// Compile-time architecture identifier.
    pub architecture: &'static str,
    /// Number of logical CPUs, when reported by the standard library.
    pub logical_cpu_count: Option<usize>,
    /// Number of physical CPU cores, when the operating system reports it.
    pub physical_cpu_count: Option<usize>,
    /// Total system RAM in bytes, when the operating system reports it.
    pub ram_bytes: Option<u64>,
    /// RAM available for new allocations at capture time, in bytes.
    pub available_ram_bytes: Option<u64>,
    /// Best-effort path of the running executable.
    pub executable: Option<PathBuf>,
    /// Best-effort process current directory.
    pub current_dir: Option<PathBuf>,
    /// Temporary directory from the process environment, if available.
    pub temporary_directory: Option<PathBuf>,
    /// Read-only filesystem observations for the temporary directory.
    pub temporary_filesystem: DirectoryFacts,
    /// Read-only filesystem observations for the current directory.
    pub current_directory_filesystem: DirectoryFacts,
    /// Shell executables found in PATH, in stable platform-defined order.
    pub shells: Vec<NamedTool>,
    /// Git executable found in PATH.
    pub git: Probe<PathBuf>,
    /// Rust compiler and package manager found in PATH.
    pub rust_toolchain: RustToolchain,
    /// Local inference runtimes found in PATH.
    pub inference_runtimes: Vec<NamedTool>,
    /// Network state is not probed.
    pub network: Probe<()>,
    /// GPU presence: `unknown` unless [`HostProbeOptions::gpu`] ran the opt-in
    /// probe ([`probe_gpus`] returns the device details).
    pub gpu: Probe<()>,
    /// Sandbox state is not probed.
    pub sandbox: Probe<()>,
}

/// A named executable located through PATH.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NamedTool {
    /// Stable capability name.
    pub name: &'static str,
    /// Tool discovery result.
    pub result: Probe<PathBuf>,
}

/// Rust toolchain components discoverable through PATH.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RustToolchain {
    /// Rust compiler executable.
    pub rustc: Probe<PathBuf>,
    /// Cargo executable.
    pub cargo: Probe<PathBuf>,
}

/// Backwards-compatible basic host information and richer capability manifest.
pub type HostInfo = HostProfile;

/// Optional, more invasive probes. Everything is off by default.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HostProbeOptions {
    /// Run the GPU probe (may execute a vendor tool such as `nvidia-smi`).
    pub gpu: Option<GpuProbeOptions>,
}

impl HostProfile {
    /// Capture local host facts using bounded, read-only probes.
    pub fn current() -> Self {
        Self::current_with(&HostProbeOptions::default())
    }

    /// Capture host facts, additionally running the opt-in probes `options` enables.
    pub fn current_with(options: &HostProbeOptions) -> Self {
        let current_dir = env::current_dir().ok();
        let temporary_directory = Some(env::temp_dir());
        let paths = path_entries();
        let mut profile =
            Self::from_path_entries(current_dir, temporary_directory, paths.as_deref());
        let resources = HostResources::current();
        profile.physical_cpu_count = resources.physical_cores;
        profile.ram_bytes = resources.total_ram_bytes;
        profile.available_ram_bytes = resources.available_ram_bytes;
        if let Some(gpu) = &options.gpu {
            profile.gpu = match probe_gpus(gpu) {
                GpuStatus::Detected { .. } => Probe::Available(()),
                GpuStatus::NotDetected { .. } => Probe::Unavailable,
                GpuStatus::NotProbed | GpuStatus::Unknown { .. } => Probe::Unknown,
            };
        }
        profile
    }

    fn from_path_entries(
        current_dir: Option<PathBuf>,
        temporary_directory: Option<PathBuf>,
        paths: Option<&[PathBuf]>,
    ) -> Self {
        let locate = |names: &[&str]| locate_tool(paths, names);
        #[cfg(windows)]
        let shell_names: &[(&str, &[&str])] = &[
            ("powershell", &["powershell.exe", "powershell"]),
            ("cmd", &["cmd.exe", "cmd"]),
        ];
        #[cfg(not(windows))]
        let shell_names: &[(&str, &[&str])] = &[("sh", &["sh"]), ("bash", &["bash"])];
        let shells = shell_names
            .iter()
            .map(|(name, candidates)| NamedTool {
                name,
                result: locate(candidates),
            })
            .collect();
        #[cfg(windows)]
        let runtimes: &[(&str, &[&str])] = &[
            ("ollama", &["ollama.exe", "ollama"]),
            ("llama-server", &["llama-server.exe", "llama-server"]),
            ("llama-cli", &["llama-cli.exe", "llama-cli"]),
        ];
        #[cfg(not(windows))]
        let runtimes: &[(&str, &[&str])] = &[
            ("ollama", &["ollama"]),
            ("llama-server", &["llama-server"]),
            ("llama-cli", &["llama-cli"]),
        ];
        Self {
            version: PROFILE_VERSION,
            os: env::consts::OS,
            architecture: env::consts::ARCH,
            logical_cpu_count: std::thread::available_parallelism().ok().map(usize::from),
            physical_cpu_count: None,
            ram_bytes: None,
            available_ram_bytes: None,
            executable: env::current_exe().ok(),
            current_dir: current_dir.clone(),
            current_directory_filesystem: DirectoryFacts {
                available: directory_state(current_dir.as_deref()),
                writable: Probe::Unknown,
            },
            temporary_filesystem: DirectoryFacts {
                available: directory_state(temporary_directory.as_deref()),
                writable: Probe::Unknown,
            },
            temporary_directory,
            shells,
            git: locate(&["git.exe", "git"]),
            rust_toolchain: RustToolchain {
                rustc: locate(&["rustc.exe", "rustc"]),
                cargo: locate(&["cargo.exe", "cargo"]),
            },
            inference_runtimes: runtimes
                .iter()
                .map(|(name, candidates)| NamedTool {
                    name,
                    result: locate(candidates),
                })
                .collect(),
            network: Probe::Unknown,
            gpu: Probe::Unknown,
            sandbox: Probe::Unknown,
        }
    }
}

/// The first `MAX_PATH_ENTRIES + 1` entries of `PATH` (one extra to detect truncation).
pub(crate) fn path_entries() -> Option<Vec<PathBuf>> {
    env::var_os("PATH").map(|value| {
        env::split_paths(&value)
            .take(MAX_PATH_ENTRIES + 1)
            .collect::<Vec<_>>()
    })
}

fn directory_state(path: Option<&Path>) -> Probe<()> {
    match path.and_then(|path| fs::metadata(path).ok()) {
        Some(metadata) if metadata.is_dir() => Probe::Available(()),
        Some(_) => Probe::Unavailable,
        None => Probe::Unknown,
    }
}

pub(crate) fn locate_tool(paths: Option<&[PathBuf]>, candidates: &[&str]) -> Probe<PathBuf> {
    locate_tool_with(paths, candidates, |path| match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) => Err(error.kind()),
    })
}

fn locate_tool_with(
    paths: Option<&[PathBuf]>,
    candidates: &[&str],
    mut inspect: impl FnMut(&Path) -> Result<bool, std::io::ErrorKind>,
) -> Probe<PathBuf> {
    let Some(paths) = paths else {
        return Probe::Unknown;
    };
    for directory in paths.iter().take(MAX_PATH_ENTRIES) {
        for candidate in candidates {
            let path = directory.join(candidate);
            match inspect(&path) {
                Ok(true) => return Probe::Available(path),
                Ok(false) => {}
                Err(std::io::ErrorKind::NotFound) => {}
                Err(_) => return Probe::Unknown,
            }
        }
    }
    if paths.len() > MAX_PATH_ENTRIES {
        Probe::Unknown
    } else {
        Probe::Unavailable
    }
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
