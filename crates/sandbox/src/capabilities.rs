//! Host capability report: what each backend can enforce, and how.

use std::{fmt, path::PathBuf};

use serde::Serialize;

use crate::SandboxLevel;

/// Current serialized capability/plan schema version.
pub const CAPABILITIES_VERSION: u32 = 1;

/// A single isolation property that a request may require.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Requirement {
    /// Every descendant can be killed together (cancel, shutdown, emergency stop).
    ProcessTreeTermination,
    /// Live process count bounded by `max_processes`.
    ProcessCountLimit,
    /// Per-process memory bounded by `max_memory_bytes`.
    MemoryLimit,
    /// Per-process CPU time bounded by `max_cpu_time`.
    CpuTimeLimit,
    /// Whole tree killed after `wall_timeout`.
    WallTimeout,
    /// The child starts from a cleared environment plus an allowlist.
    EnvIsolation,
    /// Reads outside mounts and read-only system paths are denied.
    FilesystemReadConfinement,
    /// Writes outside read-write mounts are denied.
    FilesystemWriteConfinement,
    /// All network access is denied.
    NetworkDeny,
    /// Network access is limited to an allowlist of domains.
    NetworkAllowlist,
    /// A disposable isolated environment (container/VM/overlay).
    EphemeralEnvironment,
}

impl Requirement {
    /// Every requirement in report order.
    pub const ALL: [Self; 11] = [
        Self::ProcessTreeTermination,
        Self::ProcessCountLimit,
        Self::MemoryLimit,
        Self::CpuTimeLimit,
        Self::WallTimeout,
        Self::EnvIsolation,
        Self::FilesystemReadConfinement,
        Self::FilesystemWriteConfinement,
        Self::NetworkDeny,
        Self::NetworkAllowlist,
        Self::EphemeralEnvironment,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProcessTreeTermination => "process_tree_termination",
            Self::ProcessCountLimit => "process_count_limit",
            Self::MemoryLimit => "memory_limit",
            Self::CpuTimeLimit => "cpu_time_limit",
            Self::WallTimeout => "wall_timeout",
            Self::EnvIsolation => "env_isolation",
            Self::FilesystemReadConfinement => "filesystem_read_confinement",
            Self::FilesystemWriteConfinement => "filesystem_write_confinement",
            Self::NetworkDeny => "network_deny",
            Self::NetworkAllowlist => "network_allowlist",
            Self::EphemeralEnvironment => "ephemeral_environment",
        }
    }

    /// Resource limits may be satisfied by a terminating watchdog; security
    /// boundaries never are.
    pub fn is_resource_limit(self) -> bool {
        matches!(
            self,
            Self::ProcessCountLimit | Self::MemoryLimit | Self::CpuTimeLimit
        )
    }
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How (or whether) a requirement is enforced.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Enforcement {
    /// The kernel refuses the action, or the AiDEX parent enforces it for the whole tree.
    Enforced { mechanism: String },
    /// AiDEX samples usage every `interval_ms` and kills the whole tree on violation.
    /// Usage may briefly exceed the limit before the kill.
    Watchdog { mechanism: String, interval_ms: u64 },
    /// This backend does not implement the requirement.
    NotEnforced { reason: String },
    /// The backend implements it, but this host lacks the facility.
    Unavailable { reason: String },
}

impl Enforcement {
    pub(crate) fn enforced(mechanism: impl Into<String>) -> Self {
        Self::Enforced {
            mechanism: mechanism.into(),
        }
    }

    pub(crate) fn not_enforced(reason: impl Into<String>) -> Self {
        Self::NotEnforced {
            reason: reason.into(),
        }
    }

    pub(crate) fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    /// Whether this enforcement satisfies `requirement` when it is required.
    pub fn satisfies(&self, requirement: Requirement) -> bool {
        match self {
            Self::Enforced { .. } => true,
            Self::Watchdog { .. } => requirement.is_resource_limit(),
            Self::NotEnforced { .. } | Self::Unavailable { .. } => false,
        }
    }

    /// Whether anything is applied at all.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Enforced { .. } | Self::Watchdog { .. })
    }
}

/// The enforcement a backend offers for one requirement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Capability {
    pub requirement: Requirement,
    pub enforcement: Enforcement,
    /// Known gaps of the mechanism, stated plainly.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

impl Capability {
    pub(crate) fn new(requirement: Requirement, enforcement: Enforcement) -> Self {
        Self {
            requirement,
            enforcement,
            limitations: Vec::new(),
        }
    }

    pub(crate) fn limited(mut self, limitation: impl Into<String>) -> Self {
        self.limitations.push(limitation.into());
        self
    }
}

/// The OS isolation backend in use.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// Windows Job Objects plus a cleared environment.
    WindowsJobObject,
    /// Linux process groups, rlimits, Landlock, and user/network namespaces.
    LinuxLandlock,
    /// macOS process groups, rlimits, and `sandbox-exec` (Seatbelt) profiles.
    MacosSeatbelt,
    /// No backend exists for this operating system.
    Unsupported,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WindowsJobObject => "windows_job_object",
            Self::LinuxLandlock => "linux_landlock",
            Self::MacosSeatbelt => "macos_seatbelt",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Whether the `aidex-sandbox-exec` helper was found.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HelperStatus {
    Available { path: PathBuf },
    Missing { path: PathBuf, reason: String },
}

/// Versioned, serializable capability report for this host (doctor/`aidex-host` hook).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BackendCapabilities {
    pub version: u32,
    pub backend: BackendKind,
    pub os: &'static str,
    pub helper: HelperStatus,
    /// One entry per [`Requirement`], in [`Requirement::ALL`] order.
    pub capabilities: Vec<Capability>,
    /// Highest level whose baseline requirements are enforceable; `None` when the
    /// backend cannot launch anything (for example a missing helper).
    pub max_level: Option<SandboxLevel>,
    /// Facts gathered by the probe (kernel feature versions and similar).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<String>,
}

impl BackendCapabilities {
    pub(crate) fn build(
        backend: BackendKind,
        helper: HelperStatus,
        capabilities: Vec<Capability>,
        facts: Vec<String>,
    ) -> Self {
        let mut report = Self {
            version: CAPABILITIES_VERSION,
            backend,
            os: std::env::consts::OS,
            helper,
            capabilities,
            max_level: None,
            facts,
        };
        report.capabilities.sort_by_key(|capability| {
            Requirement::ALL
                .iter()
                .position(|candidate| *candidate == capability.requirement)
        });
        report.max_level = SandboxLevel::ALL
            .iter()
            .rev()
            .copied()
            .find(|level| report.supports_baseline(*level));
        report
    }

    /// The capability entry for `requirement`.
    pub fn get(&self, requirement: Requirement) -> Option<&Capability> {
        self.capabilities
            .iter()
            .find(|capability| capability.requirement == requirement)
    }

    /// Enforcement for `requirement`, `Unavailable` if the backend did not report it.
    pub fn enforcement(&self, requirement: Requirement) -> Enforcement {
        self.get(requirement).map_or_else(
            || Enforcement::unavailable("not reported by this backend"),
            |capability| capability.enforcement.clone(),
        )
    }

    /// Whether every baseline requirement of `level` is enforceable (requests may add
    /// more requirements through limits, network mode, or mounts).
    pub fn supports_baseline(&self, level: SandboxLevel) -> bool {
        matches!(self.helper, HelperStatus::Available { .. })
            && crate::negotiate::baseline_requirements(level)
                .iter()
                .all(|requirement| self.enforcement(*requirement).satisfies(*requirement))
    }
}
