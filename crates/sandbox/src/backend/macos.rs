//! macOS backend: process groups, RLIMIT_CPU, and `sandbox-exec` (Seatbelt) profiles for
//! write confinement and network denial.

use std::path::Path;

use super::seatbelt::{self, SANDBOX_EXEC};
pub(crate) use super::unix::{TreeHandle, Watchdog, configure, contain};
use crate::{
    BackendCapabilities, BackendKind, Capability, Enforcement, EnforcementPlan, HelperStatus,
    Mount, MountAccess, Requirement, SandboxError,
    helper::protocol::{HelperPlan, SeatbeltPlan},
    paths,
};

fn sandbox_exec_available() -> bool {
    Path::new(SANDBOX_EXEC).is_file()
}

pub(crate) fn probe(helper: HelperStatus) -> BackendCapabilities {
    let seatbelt = sandbox_exec_available();
    let seatbelt_or = |mechanism: &str| {
        if seatbelt {
            Enforcement::enforced(mechanism)
        } else {
            Enforcement::unavailable(format!("{SANDBOX_EXEC} is missing"))
        }
    };
    let capabilities = vec![
        Capability::new(
            Requirement::ProcessTreeTermination,
            Enforcement::enforced("SIGKILL to the process group led by the target"),
        )
        .limited("a descendant that calls setsid() or setpgid() leaves the group and is not terminated")
        .limited("there is no parent-death signal: if the daemon crashes, the tree keeps running"),
        Capability::new(
            Requirement::ProcessCountLimit,
            Enforcement::not_enforced("RLIMIT_NPROC on macOS counts every process of the user"),
        ),
        Capability::new(
            Requirement::MemoryLimit,
            Enforcement::not_enforced("macOS does not enforce RLIMIT_AS or RLIMIT_RSS"),
        ),
        Capability::new(
            Requirement::CpuTimeLimit,
            Enforcement::enforced("RLIMIT_CPU per process"),
        ),
        Capability::new(
            Requirement::WallTimeout,
            Enforcement::enforced("parent deadline; process group SIGKILL"),
        ),
        Capability::new(
            Requirement::EnvIsolation,
            Enforcement::enforced("cleared environment plus allowlist"),
        ),
        Capability::new(
            Requirement::FilesystemReadConfinement,
            Enforcement::not_enforced(
                "the Seatbelt profile does not restrict reads; satisfiable only when the request mounts / read-only",
            ),
        ),
        Capability::new(
            Requirement::FilesystemWriteConfinement,
            seatbelt_or("sandbox-exec profile denies file-write* outside read-write mounts"),
        )
        .limited("tools that ignore TMPDIR and write to the per-user Darwin temp directory fail"),
        Capability::new(
            Requirement::NetworkDeny,
            seatbelt_or("sandbox-exec profile (deny network*)"),
        )
        .limited("mach services stay reachable; a system service could make requests on the process's behalf"),
        Capability::new(
            Requirement::NetworkAllowlist,
            Enforcement::not_enforced("no enforcing egress proxy is implemented"),
        ),
        Capability::new(
            Requirement::EphemeralEnvironment,
            Enforcement::not_enforced("no container, VM, or overlay backend is implemented"),
        ),
    ];
    let facts = vec![format!("sandbox_exec={seatbelt}")];
    BackendCapabilities::build(BackendKind::MacosSeatbelt, helper, capabilities, facts)
}

pub(crate) fn request_enforcement(
    capabilities: &BackendCapabilities,
    requirement: Requirement,
    mounts: &[Mount],
) -> Enforcement {
    let root_read_only = mounts
        .iter()
        .any(|mount| mount.access == MountAccess::ReadOnly && mount.host_path == Path::new("/"));
    if requirement == Requirement::FilesystemReadConfinement
        && root_read_only
        && sandbox_exec_available()
    {
        return Enforcement::enforced("/ is mounted read-only: every read is permitted");
    }
    capabilities.enforcement(requirement)
}

pub(crate) fn helper_plan(
    plan: &EnforcementPlan,
    mut base: HelperPlan,
) -> Result<HelperPlan, SandboxError> {
    if plan.applies(Requirement::CpuTimeLimit) {
        base.rlimits.cpu_seconds = plan.limits.max_cpu_time.map(super::cpu_seconds);
    }
    let confine_writes = plan.applies(Requirement::FilesystemWriteConfinement);
    let deny_network = plan.applies(Requirement::NetworkDeny);
    if confine_writes || deny_network {
        let writable = plan
            .mounts
            .iter()
            .filter(|mount| mount.access == MountAccess::ReadWrite)
            .map(|mount| paths::utf8(&mount.host_path, "mount"))
            .collect::<Result<Vec<_>, _>>()?;
        let profile = seatbelt::profile(&writable, confine_writes, deny_network);
        base.seatbelt = Some(SeatbeltPlan {
            sandbox_exec: SANDBOX_EXEC.into(),
            profile: profile.text,
            params: profile.params,
        });
    }
    Ok(base)
}

pub(crate) fn check_program(_plan: &EnforcementPlan, _program: &Path) -> Result<(), SandboxError> {
    Ok(())
}

/// macOS has no `/proc`; listing group members is not implemented.
pub(crate) fn group_members(_pgid: i32) -> Result<Vec<u32>, SandboxError> {
    Err(SandboxError::NotSupported("listing process group members"))
}
