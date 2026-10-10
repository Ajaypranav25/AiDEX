//! Sandbox security levels (spec §12.1).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Ordered isolation levels. A higher level never implies fewer requirements.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxLevel {
    /// Level 0: runs as the current user with no confinement. Only with `allow_direct`.
    Direct = 0,
    /// Level 1: sensitive actions go through the broker; the process gets a cleared
    /// environment, a terminable process tree, and the requested resource limits.
    Brokered = 1,
    /// Level 2: native OS confinement of filesystem reads and writes to the mounts.
    Restricted = 2,
    /// Level 3: a disposable isolated environment (container/VM/overlay). No backend
    /// implements it yet, so requests always fail closed.
    Ephemeral = 3,
}

impl SandboxLevel {
    /// Every level, lowest first.
    pub const ALL: [Self; 4] = [
        Self::Direct,
        Self::Brokered,
        Self::Restricted,
        Self::Ephemeral,
    ];

    /// Numeric rank (0-3) as used by the specification.
    pub fn rank(self) -> u8 {
        self as u8
    }

    /// Stable lowercase identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Brokered => "brokered",
            Self::Restricted => "restricted",
            Self::Ephemeral => "ephemeral",
        }
    }
}

impl fmt::Display for SandboxLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.rank(), self.as_str())
    }
}
