//! Process-group containment shared by the Linux and macOS backends.

use std::{
    os::unix::process::CommandExt,
    process::{Child, Command},
};

use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    unistd::Pid,
};

use crate::{EnforcementPlan, Requirement, SandboxError};

/// The helper (and, after `exec`, the target) leads a new process group.
pub(crate) fn configure(command: &mut Command) {
    command.process_group(0);
}

pub(crate) fn contain(child: &Child) -> Result<TreeHandle, SandboxError> {
    let pgid = i32::try_from(child.id())
        .map_err(|_| SandboxError::Spawn("process id out of range".into()))?;
    Ok(TreeHandle { pgid })
}

/// The process group holding the tree.
pub(crate) struct TreeHandle {
    pgid: i32,
}

impl TreeHandle {
    /// SIGKILL the whole process group.
    pub(crate) fn kill(&self) -> Result<(), SandboxError> {
        match killpg(Pid::from_raw(self.pgid), Signal::SIGKILL) {
            Ok(()) | Err(Errno::ESRCH) => Ok(()),
            Err(error) => Err(SandboxError::Io(format!(
                "killing process group {}: {error}",
                self.pgid
            ))),
        }
    }

    pub(crate) fn members(&self) -> Result<Vec<u32>, SandboxError> {
        super::platform::group_members(self.pgid)
    }
}

/// Unix limits are kernel-enforced rlimits; no sampling watchdog is needed.
pub(crate) struct Watchdog;

impl Watchdog {
    pub(crate) fn for_plan(_plan: &EnforcementPlan) -> Option<Self> {
        None
    }

    pub(crate) fn check(&mut self, _tree: &TreeHandle) -> Option<Requirement> {
        None
    }
}
