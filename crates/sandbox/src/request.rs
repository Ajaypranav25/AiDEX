//! What a caller asks the sandbox to enforce, and what it asks it to run.

use std::{fmt, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{SandboxLevel, env::DEFAULT_ENV_ALLOWLIST};

/// Bytes retained per output stream unless the request sets a different bound.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// How a mounted host directory may be used by the sandboxed process tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountAccess {
    /// Read, list, and execute.
    ReadOnly,
    /// Read, execute, create, modify, and delete.
    ReadWrite,
}

/// A host directory exposed to the sandbox.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Mount {
    /// Absolute host directory. It is canonicalized during negotiation.
    pub host_path: PathBuf,
    pub access: MountAccess,
}

impl Mount {
    pub fn read_only(host_path: impl Into<PathBuf>) -> Self {
        Self {
            host_path: host_path.into(),
            access: MountAccess::ReadOnly,
        }
    }

    pub fn read_write(host_path: impl Into<PathBuf>) -> Self {
        Self {
            host_path: host_path.into(),
            access: MountAccess::ReadWrite,
        }
    }
}

/// Network policy for the process tree (spec §12.5).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "domains", rename_all = "snake_case")]
pub enum NetworkMode {
    /// No network access at all. Requires OS-level network denial.
    Offline,
    /// Only the listed domains. Requires an enforcing egress filter.
    Allowlist(Vec<String>),
    /// Network governed by broker policy; no OS-level restriction is applied.
    Normal,
    /// No network checks of any kind.
    Unrestricted,
}

/// Resource bounds. `None` means "not requested" (and therefore not enforced).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum number of simultaneously live processes in the tree, including the
    /// launched program (Linux counts threads; see the crate README).
    pub max_processes: Option<u32>,
    /// Maximum memory per process, in bytes (Windows: private commit; Linux: address space).
    pub max_memory_bytes: Option<u64>,
    /// Maximum CPU time per process.
    #[serde(with = "duration_ms_opt")]
    pub max_cpu_time: Option<Duration>,
    /// Maximum wall-clock lifetime of the whole tree.
    #[serde(with = "duration_ms_opt")]
    pub wall_timeout: Option<Duration>,
    /// Maximum bytes buffered per output stream; excess output is drained and dropped.
    pub max_output_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_processes: None,
            max_memory_bytes: None,
            max_cpu_time: None,
            wall_timeout: None,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

/// A complete isolation request. Nothing here is serialized, because `env` may carry
/// secret values resolved by the broker; `Debug` redacts those values.
#[derive(Clone)]
pub struct SandboxRequest {
    pub level: SandboxLevel,
    /// Directories the process tree may use. At level 2+ nothing else is readable or
    /// writable apart from the platform's read-only system paths.
    pub mounts: Vec<Mount>,
    /// Working directory; must be inside a mount.
    pub cwd: PathBuf,
    /// Existing directory owned by this sandbox run. `tmp/` and `home/` are created
    /// inside it and exported as TEMP/TMP/TMPDIR and HOME/USERPROFILE. It is mounted
    /// read-write implicitly.
    pub scratch_dir: PathBuf,
    /// Parent environment variables that may be copied into the child.
    pub env_allowlist: Vec<String>,
    /// Explicit additions (for example a secret the broker resolved for one tool).
    pub env: Vec<(String, String)>,
    pub network: NetworkMode,
    pub limits: ResourceLimits,
    /// Explicit, documented user choice permitting level 0 (direct) execution.
    pub allow_direct: bool,
}

impl SandboxRequest {
    /// A request with the default environment allowlist, offline network, default
    /// limits, and no mounts.
    pub fn new(
        level: SandboxLevel,
        cwd: impl Into<PathBuf>,
        scratch_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            level,
            mounts: Vec::new(),
            cwd: cwd.into(),
            scratch_dir: scratch_dir.into(),
            env_allowlist: DEFAULT_ENV_ALLOWLIST
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            env: Vec::new(),
            network: NetworkMode::Offline,
            limits: ResourceLimits::default(),
            allow_direct: false,
        }
    }

    pub fn with_mount(mut self, mount: Mount) -> Self {
        self.mounts.push(mount);
        self
    }

    pub fn with_network(mut self, network: NetworkMode) -> Self {
        self.network = network;
        self
    }

    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((name.into(), value.into()));
        self
    }

    pub fn allow_direct(mut self) -> Self {
        self.allow_direct = true;
        self
    }
}

impl fmt::Debug for SandboxRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env_names: Vec<&str> = self.env.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("SandboxRequest")
            .field("level", &self.level)
            .field("mounts", &self.mounts)
            .field("cwd", &self.cwd)
            .field("scratch_dir", &self.scratch_dir)
            .field("env_allowlist", &self.env_allowlist)
            .field("env", &format_args!("{env_names:?} (values redacted)"))
            .field("network", &self.network)
            .field("limits", &self.limits)
            .field("allow_direct", &self.allow_direct)
            .finish()
    }
}

/// The program to launch under an [`crate::EnforcementPlan`].
#[derive(Clone)]
pub struct CommandSpec {
    /// Absolute path; use [`crate::EnforcementPlan::resolve_program`] for bare names.
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Bytes delivered on stdin, followed by end-of-file. `None` gives immediate EOF.
    pub stdin: Option<Vec<u8>>,
}

impl CommandSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            stdin: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(bytes.into());
        self
    }
}

impl fmt::Debug for CommandSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandSpec")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("stdin_bytes", &self.stdin.as_ref().map(Vec::len))
            .finish()
    }
}

/// Serialize a duration as integer milliseconds.
pub(crate) fn serialize_duration_ms<S: serde::Serializer>(
    value: &Duration,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_u64(u64::try_from(value.as_millis()).unwrap_or(u64::MAX))
}

/// Serialize optional durations as integer milliseconds for readable audit records.
pub(crate) mod duration_ms_opt {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        value: &Option<Duration>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(duration) => {
                serializer.serialize_some(&u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Duration>, D::Error> {
        Ok(Option::<u64>::deserialize(deserializer)?.map(Duration::from_millis))
    }
}
