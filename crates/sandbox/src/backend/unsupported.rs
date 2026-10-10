//! Placeholder for operating systems without a backend: everything fails closed.

use std::{
    path::Path,
    process::{Child, Command},
};

use crate::{
    BackendCapabilities, BackendKind, Capability, Enforcement, EnforcementPlan, HelperStatus,
    Mount, Requirement, SandboxError, helper::protocol::HelperPlan,
};

pub(crate) fn probe(helper: HelperStatus) -> BackendCapabilities {
    let capabilities = Requirement::ALL
        .iter()
        .map(|requirement| {
            Capability::new(
                *requirement,
                Enforcement::not_enforced("no sandbox backend exists for this operating system"),
            )
        })
        .collect();
    BackendCapabilities::build(BackendKind::Unsupported, helper, capabilities, Vec::new())
}

pub(crate) fn request_enforcement(
    capabilities: &BackendCapabilities,
    requirement: Requirement,
    _mounts: &[Mount],
) -> Enforcement {
    capabilities.enforcement(requirement)
}

pub(crate) fn helper_plan(
    _plan: &EnforcementPlan,
    _base: HelperPlan,
) -> Result<HelperPlan, SandboxError> {
    Err(SandboxError::NotSupported("sandboxed launch"))
}

pub(crate) fn check_program(_plan: &EnforcementPlan, _program: &Path) -> Result<(), SandboxError> {
    Err(SandboxError::NotSupported("sandboxed launch"))
}

pub(crate) fn configure(_command: &mut Command) {}

pub(crate) fn contain(_child: &Child) -> Result<TreeHandle, SandboxError> {
    Err(SandboxError::NotSupported("process containment"))
}

pub(crate) struct TreeHandle;

impl TreeHandle {
    pub(crate) fn kill(&self) -> Result<(), SandboxError> {
        Err(SandboxError::NotSupported("process tree termination"))
    }

    pub(crate) fn members(&self) -> Result<Vec<u32>, SandboxError> {
        Err(SandboxError::NotSupported("listing sandbox members"))
    }
}

pub(crate) struct Watchdog;

impl Watchdog {
    pub(crate) fn for_plan(_plan: &EnforcementPlan) -> Option<Self> {
        None
    }

    pub(crate) fn check(&mut self, _tree: &TreeHandle) -> Option<Requirement> {
        None
    }
}
