//! Capability negotiation: derive requirements from a request and refuse to plan
//! anything the backend cannot enforce.

use std::{collections::BTreeSet, ffi::OsString, fs, path::PathBuf};

use aidex_core::{EntityId, Timestamp};

use crate::{
    BackendCapabilities, CAPABILITIES_VERSION, Enforcement, EnforcementPlan, HelperStatus, Mount,
    MountAccess, NetworkMode, PlannedControl, PlannedEnv, Requirement, SandboxError, SandboxLevel,
    SandboxRequest, backend,
    env::{self, ScratchDirs},
    paths,
};

/// Requirements every request at `level` carries regardless of its options.
pub fn baseline_requirements(level: SandboxLevel) -> Vec<Requirement> {
    let mut requirements = vec![
        Requirement::ProcessTreeTermination,
        Requirement::EnvIsolation,
    ];
    if level >= SandboxLevel::Restricted {
        requirements.push(Requirement::FilesystemReadConfinement);
        requirements.push(Requirement::FilesystemWriteConfinement);
    }
    if level >= SandboxLevel::Ephemeral {
        requirements.push(Requirement::EphemeralEnvironment);
    }
    requirements
}

/// Every requirement implied by the request's level, limits, and network mode.
pub fn requirements_for(request: &SandboxRequest) -> Vec<Requirement> {
    let mut set: BTreeSet<Requirement> = baseline_requirements(request.level).into_iter().collect();
    let limits = &request.limits;
    if limits.max_processes.is_some() {
        set.insert(Requirement::ProcessCountLimit);
    }
    if limits.max_memory_bytes.is_some() {
        set.insert(Requirement::MemoryLimit);
    }
    if limits.max_cpu_time.is_some() {
        set.insert(Requirement::CpuTimeLimit);
    }
    if limits.wall_timeout.is_some() {
        set.insert(Requirement::WallTimeout);
    }
    match request.network {
        NetworkMode::Offline => {
            set.insert(Requirement::NetworkDeny);
        }
        NetworkMode::Allowlist(_) => {
            set.insert(Requirement::NetworkAllowlist);
        }
        NetworkMode::Normal | NetworkMode::Unrestricted => {}
    }
    Requirement::ALL
        .iter()
        .copied()
        .filter(|requirement| set.contains(requirement))
        .collect()
}

/// Paths validated and canonicalized for the launch.
pub(crate) struct ValidatedPaths {
    pub mounts: Vec<Mount>,
    pub cwd: PathBuf,
    pub temp_dir: PathBuf,
    pub home_dir: PathBuf,
}

fn validate_paths(request: &SandboxRequest) -> Result<ValidatedPaths, SandboxError> {
    let scratch = paths::canonical_dir(&request.scratch_dir, "scratch_dir")?;
    let temp_dir = scratch.join("tmp");
    let home_dir = scratch.join("home");

    let mut mounts: Vec<Mount> = Vec::new();
    for mount in &request.mounts {
        let host_path = paths::canonical_dir(&mount.host_path, "mount")?;
        paths::utf8(&host_path, "mount")?;
        push_unique(
            &mut mounts,
            Mount {
                host_path,
                access: mount.access,
            },
        );
    }
    push_unique(&mut mounts, Mount::read_write(scratch));

    for read_only in mounts.iter().filter(|m| m.access == MountAccess::ReadOnly) {
        if let Some(writable) = mounts.iter().find(|m| {
            m.access == MountAccess::ReadWrite
                && paths::is_within(&read_only.host_path, &m.host_path)
        }) {
            return Err(SandboxError::invalid(format!(
                "read-only mount {} is inside read-write mount {}; nested read-only mounts cannot be enforced",
                read_only.host_path.display(),
                writable.host_path.display()
            )));
        }
    }

    let cwd = paths::canonical_dir(&request.cwd, "cwd")?;
    paths::utf8(&cwd, "cwd")?;
    if !mounts
        .iter()
        .any(|mount| paths::is_within(&cwd, &mount.host_path))
    {
        return Err(SandboxError::invalid(format!(
            "cwd {} is not inside any mount",
            cwd.display()
        )));
    }
    Ok(ValidatedPaths {
        mounts,
        cwd,
        temp_dir,
        home_dir,
    })
}

/// Create the scratch `tmp/` and `home/` directories once negotiation succeeded.
fn create_scratch(validated: &ValidatedPaths) -> Result<(), SandboxError> {
    let mut to_create = vec![validated.temp_dir.clone(), validated.home_dir.clone()];
    if cfg!(windows) {
        to_create.push(validated.home_dir.join("AppData").join("Roaming"));
        to_create.push(validated.home_dir.join("AppData").join("Local"));
    }
    for directory in &to_create {
        fs::create_dir_all(directory)
            .map_err(|error| SandboxError::io("creating sandbox scratch directory", &error))?;
    }
    Ok(())
}

fn push_unique(mounts: &mut Vec<Mount>, mount: Mount) {
    if !mounts.contains(&mount) {
        mounts.push(mount);
    }
}

fn validate_options(request: &SandboxRequest) -> Result<(), SandboxError> {
    if let NetworkMode::Allowlist(domains) = &request.network
        && (domains.is_empty()
            || domains
                .iter()
                .any(|domain| domain.is_empty() || domain.chars().any(char::is_whitespace)))
    {
        return Err(SandboxError::invalid(
            "network allowlist must list non-empty domains without whitespace",
        ));
    }
    let limits = &request.limits;
    if limits.max_processes == Some(0) {
        return Err(SandboxError::invalid("max_processes must be at least 1"));
    }
    if limits.max_memory_bytes == Some(0) {
        return Err(SandboxError::invalid("max_memory_bytes must be positive"));
    }
    if limits
        .max_cpu_time
        .is_some_and(|duration| duration.is_zero())
        || limits
            .wall_timeout
            .is_some_and(|duration| duration.is_zero())
    {
        return Err(SandboxError::invalid("time limits must be positive"));
    }
    Ok(())
}

/// Why an unrequired control is not applied.
fn not_required_reason(requirement: Requirement, request: &SandboxRequest) -> String {
    match requirement {
        Requirement::FilesystemReadConfinement
        | Requirement::FilesystemWriteConfinement
        | Requirement::EphemeralEnvironment => {
            format!("not applied at level {}", request.level)
        }
        Requirement::NetworkDeny | Requirement::NetworkAllowlist => match request.network {
            NetworkMode::Normal => "network mode normal: no OS-level restriction".into(),
            NetworkMode::Unrestricted => "network mode unrestricted".into(),
            NetworkMode::Offline => "superseded by network deny".into(),
            NetworkMode::Allowlist(_) => "superseded by network allowlist".into(),
        },
        _ => "not requested".into(),
    }
}

/// Negotiate `request` against `capabilities` using the given parent environment.
pub(crate) fn negotiate_with(
    capabilities: &BackendCapabilities,
    request: &SandboxRequest,
    parent_env: Vec<(OsString, OsString)>,
) -> Result<EnforcementPlan, SandboxError> {
    if request.level == SandboxLevel::Direct && !request.allow_direct {
        return Err(SandboxError::DirectNotAllowed);
    }
    validate_options(request)?;
    let helper = match &capabilities.helper {
        HelperStatus::Available { path } => path.clone(),
        HelperStatus::Missing { path, reason } => {
            return Err(SandboxError::HelperUnavailable(format!(
                "{}: {reason}",
                path.display()
            )));
        }
    };
    let validated = validate_paths(request)?;
    let required = requirements_for(request);

    let mut missing = Vec::new();
    let mut controls = Vec::with_capacity(Requirement::ALL.len());
    for requirement in Requirement::ALL {
        let is_required = required.contains(&requirement);
        let capability = capabilities.get(requirement);
        let offered = backend::request_enforcement(capabilities, requirement, &validated.mounts);
        if is_required && !offered.satisfies(requirement) {
            missing.push(requirement);
        }
        let (enforcement, limitations) = if is_required {
            let limitations = capability
                .map(|c| c.limitations.clone())
                .unwrap_or_default();
            (offered, limitations)
        } else {
            (
                Enforcement::not_enforced(not_required_reason(requirement, request)),
                Vec::new(),
            )
        };
        controls.push(PlannedControl {
            requirement,
            required: is_required,
            enforcement,
            limitations,
        });
    }
    if !missing.is_empty() {
        return Err(SandboxError::Unsupported {
            level: request.level,
            missing,
        });
    }

    let vars = env::build_environment(
        &request.env_allowlist,
        &request.env,
        parent_env,
        &ScratchDirs {
            temp: &validated.temp_dir,
            home: &validated.home_dir,
        },
    )?;
    create_scratch(&validated)?;
    let plan_id =
        EntityId::generate("plan").map_err(|error| SandboxError::Io(error.to_string()))?;
    Ok(EnforcementPlan {
        version: CAPABILITIES_VERSION,
        plan_id,
        negotiated_at: Timestamp::now(),
        backend: capabilities.backend,
        level: request.level,
        direct_mode: request.level == SandboxLevel::Direct,
        controls,
        mounts: validated.mounts,
        cwd: validated.cwd,
        temp_dir: validated.temp_dir,
        home_dir: validated.home_dir,
        network: request.network.clone(),
        limits: request.limits.clone(),
        env: PlannedEnv { vars },
        helper,
    })
}
