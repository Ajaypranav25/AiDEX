//! Linux backend: process groups, rlimits, Landlock, and user/network namespaces,
//! applied by `aidex-sandbox-exec` to itself before it `exec`s the target.

use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(crate) use super::unix::{TreeHandle, Watchdog, configure, contain};
use crate::{
    BackendCapabilities, BackendKind, Capability, Enforcement, EnforcementPlan, HelperStatus,
    Mount, MountAccess, Requirement, SandboxError,
    helper::protocol::{HelperPlan, LINUX_SYSTEM_READ_PATHS, LandlockPlan, ProbeReport},
    paths,
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Run `aidex-sandbox-exec --probe` and read its report.
fn run_probe(helper: &Path) -> Result<ProbeReport, String> {
    let mut child = Command::new(helper)
        .arg("--probe")
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("starting probe: {error}"))?;
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("probe timed out".into());
            }
            Err(error) => return Err(format!("waiting for probe: {error}")),
        }
    }
    let mut json = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout
            .read_to_end(&mut json)
            .map_err(|error| format!("reading probe: {error}"))?;
    }
    serde_json::from_slice(&json).map_err(|error| format!("parsing probe report: {error}"))
}

pub(crate) fn probe(helper: HelperStatus) -> BackendCapabilities {
    let report = match &helper {
        HelperStatus::Available { path } => run_probe(path),
        HelperStatus::Missing { reason, .. } => Err(format!("helper missing: {reason}")),
    };
    let (report, probe_error) = match report {
        Ok(report) => (report, None),
        Err(error) => (ProbeReport::default(), Some(error)),
    };
    let unavailable = |fallback: &Option<String>| {
        Enforcement::unavailable(
            probe_error
                .clone()
                .or_else(|| fallback.clone())
                .unwrap_or_else(|| "not supported by this kernel".into()),
        )
    };

    let mut facts = Vec::new();
    if let Some(error) = &probe_error {
        facts.push(format!("probe_error={error}"));
    }
    facts.push(match report.landlock_abi {
        Some(abi) => format!("landlock_abi={abi}"),
        None => "landlock_abi=unsupported".into(),
    });
    facts.push(format!("user_namespace={}", report.user_namespace));
    facts.push(format!("network_namespace={}", report.network_namespace));

    let filesystem = if report.landlock_enforced && report.landlock_abi.is_some_and(|abi| abi >= 3)
    {
        Enforcement::enforced(format!(
            "Landlock (kernel ABI v{}, ABI v3 filesystem rights handled)",
            report.landlock_abi.unwrap_or(3)
        ))
    } else if report.landlock_abi.is_some_and(|abi| abi < 3) {
        Enforcement::unavailable(
            "Landlock ABI < 3 cannot deny truncate(2) outside the mounts; Linux 6.2+ is required",
        )
    } else {
        unavailable(&report.landlock_error)
    };
    let scope_note = if report.landlock_abi.is_some_and(|abi| abi >= 6) {
        None
    } else {
        Some(
            "signals and abstract unix sockets outside the sandbox are reachable (Landlock ABI < 6)",
        )
    };
    let with_scope_note = |capability: Capability| match scope_note {
        Some(note) => capability.limited(note),
        None => capability,
    };

    let capabilities = vec![
        Capability::new(
            Requirement::ProcessTreeTermination,
            Enforcement::enforced("SIGKILL to the process group led by the target; the target dies with its parent thread (PR_SET_PDEATHSIG)"),
        )
        .limited("a descendant that calls setsid() or setpgid() leaves the group and is not terminated"),
        if report.user_namespace {
            Capability::new(
                Requirement::ProcessCountLimit,
                Enforcement::enforced("RLIMIT_NPROC inside a private user namespace"),
            )
            .limited("the kernel counts threads, not only processes")
        } else {
            Capability::new(
                Requirement::ProcessCountLimit,
                unavailable(&report.user_namespace_error.as_ref().map(|error| {
                    format!("a private user namespace is required so RLIMIT_NPROC counts only the sandbox: {error}")
                })),
            )
        },
        Capability::new(
            Requirement::MemoryLimit,
            Enforcement::enforced("RLIMIT_AS per process"),
        )
        .limited("bounds virtual address space, which can be much larger than resident memory"),
        Capability::new(
            Requirement::CpuTimeLimit,
            Enforcement::enforced("RLIMIT_CPU per process (SIGKILL at the limit)"),
        ),
        Capability::new(
            Requirement::WallTimeout,
            Enforcement::enforced("parent deadline; process group SIGKILL"),
        ),
        Capability::new(
            Requirement::EnvIsolation,
            Enforcement::enforced("cleared environment plus allowlist"),
        )
        .limited("without Landlock a same-user process can read other processes' /proc/<pid>/environ unless they are non-dumpable"),
        with_scope_note(Capability::new(Requirement::FilesystemReadConfinement, filesystem.clone())),
        with_scope_note(Capability::new(Requirement::FilesystemWriteConfinement, filesystem)),
        if report.network_namespace {
            Capability::new(
                Requirement::NetworkDeny,
                Enforcement::enforced("private network namespace without interfaces"),
            )
            .limited("loopback is down too, so local servers are unreachable")
        } else {
            Capability::new(
                Requirement::NetworkDeny,
                unavailable(&report.network_namespace_error),
            )
        },
        Capability::new(
            Requirement::NetworkAllowlist,
            Enforcement::not_enforced("no enforcing egress proxy is implemented"),
        ),
        Capability::new(
            Requirement::EphemeralEnvironment,
            Enforcement::not_enforced("no container, VM, or overlay backend is implemented"),
        ),
    ];
    BackendCapabilities::build(BackendKind::LinuxLandlock, helper, capabilities, facts)
}

pub(crate) fn request_enforcement(
    capabilities: &BackendCapabilities,
    requirement: Requirement,
    _mounts: &[Mount],
) -> Enforcement {
    capabilities.enforcement(requirement)
}

fn mount_paths(plan: &EnforcementPlan, access: MountAccess) -> Result<Vec<String>, SandboxError> {
    plan.mounts
        .iter()
        .filter(|mount| mount.access == access)
        .map(|mount| paths::utf8(&mount.host_path, "mount"))
        .collect()
}

pub(crate) fn helper_plan(
    plan: &EnforcementPlan,
    mut base: HelperPlan,
) -> Result<HelperPlan, SandboxError> {
    let count = plan.applies(Requirement::ProcessCountLimit);
    let network = plan.applies(Requirement::NetworkDeny);
    base.user_namespace = count || network;
    base.network_namespace = network;
    if count {
        base.rlimits.max_tasks = plan.limits.max_processes.map(u64::from);
    }
    if plan.applies(Requirement::MemoryLimit) {
        base.rlimits.address_space_bytes = plan.limits.max_memory_bytes;
    }
    if plan.applies(Requirement::CpuTimeLimit) {
        base.rlimits.cpu_seconds = plan.limits.max_cpu_time.map(super::cpu_seconds);
    }
    if plan.applies(Requirement::FilesystemReadConfinement)
        || plan.applies(Requirement::FilesystemWriteConfinement)
    {
        base.landlock = Some(LandlockPlan {
            read_only: mount_paths(plan, MountAccess::ReadOnly)?,
            read_write: mount_paths(plan, MountAccess::ReadWrite)?,
        });
    }
    Ok(base)
}

/// Under Landlock the program must live in a mount or a read-only system path.
pub(crate) fn check_program(plan: &EnforcementPlan, program: &Path) -> Result<(), SandboxError> {
    if !plan.applies(Requirement::FilesystemReadConfinement) {
        return Ok(());
    }
    let allowed = plan
        .mounts
        .iter()
        .map(|mount| mount.host_path.as_path())
        .chain(LINUX_SYSTEM_READ_PATHS.iter().map(Path::new))
        .any(|root| paths::is_within(program, root));
    if allowed {
        Ok(())
    } else {
        Err(SandboxError::invalid(format!(
            "{} is outside every mount and system path, so Landlock would deny executing it",
            program.display()
        )))
    }
}

/// Live (non-zombie) members of a process group, from `/proc`.
pub(crate) fn group_members(pgid: i32) -> Result<Vec<u32>, SandboxError> {
    let entries =
        fs::read_dir("/proc").map_err(|error| SandboxError::io("reading /proc", &error))?;
    let mut members = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // Fields after the parenthesised command: state ppid pgrp ...
        let Some(after) = stat.rfind(')').map(|index| &stat[index + 1..]) else {
            continue;
        };
        let fields: Vec<&str> = after.split_whitespace().collect();
        if fields.len() > 2 && fields[0] != "Z" && fields[2].parse::<i32>().ok() == Some(pgid) {
            members.push(pid);
        }
    }
    members.sort_unstable();
    Ok(members)
}
