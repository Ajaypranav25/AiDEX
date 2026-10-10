//! macOS helper: rlimits applied to itself, then `exec` (through `sandbox-exec` when
//! the plan carries a Seatbelt profile).

use std::{os::unix::process::CommandExt, process::Command};

use nix::{
    sys::resource::{Resource, getrlimit, setrlimit},
    unistd::getppid,
};

use super::{
    HelperFailure,
    protocol::{HelperPlan, RlimitPlan},
    signal_ready,
};

pub(crate) fn exec(plan: &HelperPlan) -> Result<i32, HelperFailure> {
    if plan.user_namespace || plan.network_namespace || plan.landlock.is_some() {
        return Err(HelperFailure::new(
            "plan",
            "namespaces and Landlock are Linux-only",
        ));
    }
    if plan.rlimits.max_tasks.is_some() || plan.rlimits.address_space_bytes.is_some() {
        return Err(HelperFailure::new(
            "plan",
            "macOS does not enforce RLIMIT_NPROC per sandbox or RLIMIT_AS",
        ));
    }
    if u32::try_from(getppid().as_raw()).ok() != Some(plan.parent_pid) {
        return Err(HelperFailure::new(
            "parent",
            "the AiDEX parent exited before the sandbox started",
        ));
    }
    apply_rlimits(&plan.rlimits)?;
    let mut command = match &plan.seatbelt {
        Some(seatbelt) => {
            let mut command = Command::new(&seatbelt.sandbox_exec);
            command.arg("-p").arg(&seatbelt.profile);
            for (name, value) in &seatbelt.params {
                command.arg("-D").arg(format!("{name}={value}"));
            }
            command.arg(&plan.program).args(&plan.args);
            command
        }
        None => {
            let mut command = Command::new(&plan.program);
            command.args(&plan.args);
            command
        }
    };
    signal_ready()?;
    let error = command.exec();
    Err(HelperFailure::new("exec", error))
}

fn set_limit(resource: Resource, value: u64, stage: &'static str) -> Result<(), HelperFailure> {
    let (_, hard) = getrlimit(resource).map_err(|error| HelperFailure::new(stage, error))?;
    let value = value.min(hard);
    setrlimit(resource, value, value).map_err(|error| HelperFailure::new(stage, error))
}

fn apply_rlimits(limits: &RlimitPlan) -> Result<(), HelperFailure> {
    set_limit(Resource::RLIMIT_CORE, 0, "rlimit_core")?;
    if let Some(seconds) = limits.cpu_seconds {
        set_limit(Resource::RLIMIT_CPU, seconds, "rlimit_cpu")?;
    }
    Ok(())
}
