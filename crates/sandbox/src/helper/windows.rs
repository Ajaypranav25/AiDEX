//! Windows helper: already inside the Job Object; start the target and relay its exit.

use std::process::{Command, Stdio};

use super::{HelperFailure, protocol::HelperPlan, signal_ready};

pub(crate) fn exec(plan: &HelperPlan) -> Result<i32, HelperFailure> {
    if plan.user_namespace
        || plan.network_namespace
        || plan.landlock.is_some()
        || plan.seatbelt.is_some()
    {
        return Err(HelperFailure::new(
            "plan",
            "plan requests a restriction that the Windows backend cannot apply",
        ));
    }
    if plan.rlimits.max_tasks.is_some()
        || plan.rlimits.address_space_bytes.is_some()
        || plan.rlimits.cpu_seconds.is_some()
    {
        return Err(HelperFailure::new(
            "plan",
            "Windows limits are enforced by the parent, not by helper rlimits",
        ));
    }
    // `parent_pid` is not checked here: Windows parent ids are not authoritative. The
    // trust anchor is job membership, which the parent established before sending the
    // plan; if the parent dies, KILL_ON_JOB_CLOSE terminates this helper and the target.
    signal_ready()?;
    let mut child = Command::new(&plan.program)
        .args(&plan.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| HelperFailure::new("spawn", error))?;
    let status = child
        .wait()
        .map_err(|error| HelperFailure::new("wait", error))?;
    Ok(status.code().unwrap_or(1))
}
