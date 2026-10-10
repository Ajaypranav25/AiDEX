//! Platform dispatch: probing, helper plans, launching, containment, and watchdogs.

pub(crate) mod seatbelt;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod unsupported;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
use unsupported as platform;
#[cfg(windows)]
use windows as platform;

use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

pub(crate) use platform::{TreeHandle, Watchdog};

use crate::{
    BackendCapabilities, CommandSpec, Enforcement, EnforcementPlan, HelperStatus, Mount,
    Requirement, SandboxError,
    helper::protocol::{HelperPlan, PROTOCOL_VERSION, RlimitPlan},
    paths,
};

/// Sampling interval of watchdog-enforced limits.
pub(crate) const WATCHDOG_INTERVAL: Duration = Duration::from_millis(25);

/// Probe this host's backend.
pub(crate) fn probe(helper: HelperStatus) -> BackendCapabilities {
    platform::probe(helper)
}

/// Enforcement offered for `requirement` given the request's validated mounts.
pub(crate) fn request_enforcement(
    capabilities: &BackendCapabilities,
    requirement: Requirement,
    mounts: &[Mount],
) -> Enforcement {
    platform::request_enforcement(capabilities, requirement, mounts)
}

/// The plan sent to the helper for `command`.
pub(crate) fn helper_plan(
    plan: &EnforcementPlan,
    command: &CommandSpec,
) -> Result<HelperPlan, SandboxError> {
    let base = HelperPlan {
        version: PROTOCOL_VERSION,
        program: paths::utf8(&command.program, "program")?,
        args: command.args.clone(),
        parent_pid: std::process::id(),
        rlimits: RlimitPlan::default(),
        user_namespace: false,
        network_namespace: false,
        landlock: None,
        seatbelt: None,
    };
    platform::helper_plan(plan, base)
}

/// Platform checks on the program before launch (for example Landlock exec rights).
pub(crate) fn check_program(plan: &EnforcementPlan, program: &Path) -> Result<(), SandboxError> {
    platform::check_program(plan, program)
}

/// A started helper and the container that holds its tree.
pub(crate) struct Launched {
    pub child: Child,
    pub tree: TreeHandle,
}

/// Start the helper with the plan's environment and working directory, then place it in
/// the platform container before it receives its plan.
pub(crate) fn launch(plan: &EnforcementPlan) -> Result<Launched, SandboxError> {
    let mut command = Command::new(&plan.helper);
    command
        .arg("--exec")
        .env_clear()
        .envs(plan.env.vars.iter().map(|(name, value)| (name, value)))
        .current_dir(&plan.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    platform::configure(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| SandboxError::Spawn(error.to_string()))?;
    match platform::contain(&child) {
        Ok(tree) => Ok(Launched { child, tree }),
        Err(error) => {
            // The helper blocks on its plan, so it has not started anything yet.
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

/// Seconds for a CPU-time rlimit, rounded up, at least 1.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn cpu_seconds(duration: Duration) -> u64 {
    (duration.as_secs() + u64::from(duration.subsec_nanos() > 0)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_seconds_round_up() {
        assert_eq!(cpu_seconds(Duration::from_millis(1)), 1);
        assert_eq!(cpu_seconds(Duration::from_secs(2)), 2);
        assert_eq!(cpu_seconds(Duration::from_millis(2001)), 3);
    }
}
