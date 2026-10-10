//! Linux helper: namespaces, rlimits, and Landlock applied to itself, then `exec`.

use std::{fs, os::unix::process::CommandExt, process::Command};

use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreatedAttr, RulesetStatus, Scope, path_beneath_rules,
};
use nix::{
    sched::{CloneFlags, unshare},
    sys::{
        prctl,
        resource::{Resource, getrlimit, setrlimit},
        signal::Signal,
    },
    unistd::{getgid, getppid, getuid},
};

use super::{
    HelperFailure,
    protocol::{
        HelperPlan, LINUX_DEVICE_FILES, LINUX_SYSTEM_READ_PATHS, LandlockPlan, PROTOCOL_VERSION,
        ProbeReport, RlimitPlan,
    },
    signal_ready,
};

/// Filesystem rights are a hard requirement at this ABI: v3 is the first that can
/// deny `truncate(2)` outside the allowed hierarchies.
pub(crate) const FILESYSTEM_ABI: ABI = ABI::V3;

pub(crate) fn exec(plan: &HelperPlan) -> Result<i32, HelperFailure> {
    if plan.seatbelt.is_some() {
        return Err(HelperFailure::new(
            "plan",
            "seatbelt profiles are macOS-only",
        ));
    }
    // Die with the parent thread that launched us (it lives until we are reaped).
    prctl::set_pdeathsig(Signal::SIGKILL).map_err(|error| HelperFailure::new("parent", error))?;
    if u32::try_from(getppid().as_raw()).ok() != Some(plan.parent_pid) {
        return Err(HelperFailure::new(
            "parent",
            "the AiDEX parent exited before the sandbox started",
        ));
    }
    if plan.network_namespace && !plan.user_namespace {
        return Err(HelperFailure::new(
            "plan",
            "a network namespace requires a user namespace",
        ));
    }
    if plan.user_namespace {
        enter_user_namespace().map_err(|message| HelperFailure::new("user_namespace", message))?;
    }
    if plan.network_namespace {
        unshare(CloneFlags::CLONE_NEWNET)
            .map_err(|error| HelperFailure::new("network_namespace", error))?;
    }
    apply_rlimits(&plan.rlimits)?;
    if let Some(landlock) = &plan.landlock {
        apply_landlock(landlock)?;
    }
    signal_ready()?;
    let error = Command::new(&plan.program).args(&plan.args).exec();
    Err(HelperFailure::new("exec", error))
}

/// Enter a private user namespace that maps our own uid/gid to themselves.
fn enter_user_namespace() -> Result<(), String> {
    let uid = getuid();
    let gid = getgid();
    unshare(CloneFlags::CLONE_NEWUSER)
        .map_err(|error| format!("unshare(CLONE_NEWUSER): {error}"))?;
    fs::write("/proc/self/setgroups", "deny").map_err(|error| format!("setgroups: {error}"))?;
    fs::write("/proc/self/uid_map", format!("{uid} {uid} 1\n"))
        .map_err(|error| format!("uid_map: {error}"))?;
    fs::write("/proc/self/gid_map", format!("{gid} {gid} 1\n"))
        .map_err(|error| format!("gid_map: {error}"))?;
    Ok(())
}

fn set_limit(resource: Resource, value: u64, stage: &'static str) -> Result<(), HelperFailure> {
    let (_, hard) = getrlimit(resource).map_err(|error| HelperFailure::new(stage, error))?;
    let value = value.min(hard);
    setrlimit(resource, value, value).map_err(|error| HelperFailure::new(stage, error))
}

fn apply_rlimits(limits: &RlimitPlan) -> Result<(), HelperFailure> {
    set_limit(Resource::RLIMIT_CORE, 0, "rlimit_core")?;
    if let Some(tasks) = limits.max_tasks {
        set_limit(Resource::RLIMIT_NPROC, tasks, "rlimit_nproc")?;
    }
    if let Some(bytes) = limits.address_space_bytes {
        set_limit(Resource::RLIMIT_AS, bytes, "rlimit_as")?;
    }
    if let Some(seconds) = limits.cpu_seconds {
        set_limit(Resource::RLIMIT_CPU, seconds, "rlimit_cpu")?;
    }
    Ok(())
}

fn apply_landlock(plan: &LandlockPlan) -> Result<(), HelperFailure> {
    let failure = |error: landlock::RulesetError| HelperFailure::new("landlock", error);
    let all = AccessFs::from_all(FILESYSTEM_ABI);
    let read = AccessFs::from_read(FILESYSTEM_ABI);
    let mut ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(all)
        .map_err(failure)?
        // Signal and abstract-socket scoping need ABI v6; applied when available.
        .set_compatibility(CompatLevel::BestEffort)
        .scope(Scope::AbstractUnixSocket | Scope::Signal)
        .map_err(failure)?
        .set_compatibility(CompatLevel::HardRequirement)
        .create()
        .map_err(failure)?
        .add_rules(path_beneath_rules(LINUX_SYSTEM_READ_PATHS, read))
        .map_err(failure)?
        .add_rules(path_beneath_rules(LINUX_DEVICE_FILES, all))
        .map_err(failure)?;
    for (paths, access) in [(&plan.read_only, read), (&plan.read_write, all)] {
        for path in paths {
            let fd = PathFd::new(path)
                .map_err(|error| HelperFailure::new("landlock", format!("{path}: {error}")))?;
            ruleset = ruleset
                .add_rule(PathBeneath::new(fd, access))
                .map_err(failure)?;
        }
    }
    let status = ruleset.restrict_self().map_err(failure)?;
    if status.ruleset == RulesetStatus::NotEnforced || !status.no_new_privs {
        return Err(HelperFailure::new(
            "landlock",
            "the kernel did not enforce the Landlock ruleset",
        ));
    }
    Ok(())
}

/// Observe kernel facilities. Runs in a disposable helper process, so entering
/// namespaces and restricting itself has no effect on the parent.
pub(crate) fn probe() -> ProbeReport {
    let mut report = ProbeReport {
        version: PROTOCOL_VERSION,
        ..ProbeReport::default()
    };
    for (abi, number) in [
        (ABI::V6, 6),
        (ABI::V5, 5),
        (ABI::V4, 4),
        (ABI::V3, 3),
        (ABI::V2, 2),
        (ABI::V1, 1),
    ] {
        let created = Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(AccessFs::from_all(abi))
            .and_then(Ruleset::create);
        match created {
            Ok(_) => {
                report.landlock_abi = Some(number);
                break;
            }
            Err(error) => report.landlock_error = Some(error.to_string()),
        }
    }
    if report.landlock_abi.is_some() {
        report.landlock_error = None;
    }

    match enter_user_namespace() {
        Ok(()) => {
            report.user_namespace = true;
            match unshare(CloneFlags::CLONE_NEWNET) {
                Ok(()) => report.network_namespace = true,
                Err(error) => {
                    report.network_namespace_error =
                        Some(format!("unshare(CLONE_NEWNET): {error}"));
                }
            }
        }
        Err(error) => {
            report.network_namespace_error = Some(format!("requires a user namespace: {error}"));
            report.user_namespace_error = Some(error);
        }
    }

    if report.landlock_abi.is_some_and(|abi| abi >= 3) {
        let plan = LandlockPlan {
            read_only: Vec::new(),
            read_write: vec!["/".into()],
        };
        match apply_landlock(&plan) {
            Ok(()) => report.landlock_enforced = true,
            Err(failure) => report.landlock_error = Some(failure.message),
        }
    }
    report
}
