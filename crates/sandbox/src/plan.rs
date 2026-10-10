//! The negotiated, auditable statement of exactly what a launch will enforce.

use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
};

use aidex_core::{EntityId, Timestamp};
use serde::{Serialize, Serializer, ser::SerializeSeq};

use crate::{
    BackendKind, Enforcement, Mount, NetworkMode, Requirement, ResourceLimits, SandboxError,
    SandboxLevel, env,
};

/// Enforcement of one requirement in a negotiated plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedControl {
    pub requirement: Requirement,
    /// Whether the request (level, limits, network, mounts) required it.
    pub required: bool,
    pub enforcement: Enforcement,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

/// The child environment. Serialization and `Debug` expose variable names only.
#[derive(Clone, Default)]
pub struct PlannedEnv {
    pub(crate) vars: Vec<(OsString, OsString)>,
}

impl PlannedEnv {
    /// Names of every variable the child receives.
    pub fn names(&self) -> Vec<String> {
        self.vars
            .iter()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect()
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<&std::ffi::OsStr> {
        env::lookup(&self.vars, name)
    }
}

impl fmt::Debug for PlannedEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} (values redacted)", self.names())
    }
}

impl Serialize for PlannedEnv {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let names = self.names();
        let mut seq = serializer.serialize_seq(Some(names.len()))?;
        for name in &names {
            seq.serialize_element(name)?;
        }
        seq.end()
    }
}

/// Exactly what a launch under this plan enforces. Produced only by negotiation, which
/// fails instead of producing a plan that would silently weaken the request.
#[derive(Clone, Debug, Serialize)]
pub struct EnforcementPlan {
    pub version: u32,
    pub plan_id: EntityId,
    pub negotiated_at: Timestamp,
    pub backend: BackendKind,
    pub level: SandboxLevel,
    /// True only for level 0 with the explicit `allow_direct` opt-in.
    pub direct_mode: bool,
    /// One entry per [`Requirement`], in [`Requirement::ALL`] order.
    pub controls: Vec<PlannedControl>,
    /// Canonical mounts, including the implicit read-write scratch mount.
    pub mounts: Vec<Mount>,
    pub cwd: PathBuf,
    pub temp_dir: PathBuf,
    pub home_dir: PathBuf,
    pub network: NetworkMode,
    pub limits: ResourceLimits,
    /// Child environment (names only when serialized).
    pub env: PlannedEnv,
    #[serde(skip)]
    pub(crate) helper: PathBuf,
}

impl EnforcementPlan {
    /// The control for `requirement`.
    pub fn control(&self, requirement: Requirement) -> Option<&PlannedControl> {
        self.controls
            .iter()
            .find(|control| control.requirement == requirement)
    }

    /// Whether `requirement` is actively applied (enforced or watchdog).
    pub fn applies(&self, requirement: Requirement) -> bool {
        self.controls
            .iter()
            .any(|control| control.requirement == requirement && control.enforcement.is_active())
    }

    /// Resolve a program name against the plan's PATH and working directory.
    pub fn resolve_program(&self, program: &str) -> Result<PathBuf, SandboxError> {
        env::resolve_program(program, self.env.lookup("PATH"), &self.cwd)
    }

    /// The helper binary this plan was negotiated against.
    pub fn helper(&self) -> &Path {
        &self.helper
    }
}
