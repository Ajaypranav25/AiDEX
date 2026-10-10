//! Fail-closed OS process isolation for AiDEX managed processes.
//!
//! Callers describe what they need in a [`SandboxRequest`]; [`Sandbox::negotiate`]
//! returns an [`EnforcementPlan`] stating exactly what will be enforced, or fails with
//! [`SandboxError::Unsupported`] naming every requirement this host cannot enforce.
//! Nothing is ever silently downgraded. [`Sandbox::spawn`] launches a program under a
//! plan through the `aidex-sandbox-exec` helper and returns a [`ManagedProcess`];
//! every live process is tracked by a [`ProcessRegistry`] for cancel, shutdown,
//! emergency stop, and drive removal.

mod backend;
mod capabilities;
mod env;
mod error;
#[doc(hidden)]
pub mod helper;
mod level;
mod negotiate;
mod paths;
mod plan;
mod process;
mod registry;
mod request;
mod sandbox;

pub use capabilities::{
    BackendCapabilities, BackendKind, CAPABILITIES_VERSION, Capability, Enforcement, HelperStatus,
    Requirement,
};
pub use env::{DEFAULT_ENV_ALLOWLIST, MANAGED_ENV_VARS, is_secret_like, resolve_program};
pub use error::SandboxError;
pub use level::SandboxLevel;
pub use negotiate::{baseline_requirements, requirements_for};
pub use plan::{EnforcementPlan, PlannedControl, PlannedEnv};
pub use process::{
    CapturedOutput, ExitStatusInfo, KillReason, ManagedProcess, StopReason, StreamCapture,
};
pub use registry::{
    DEFAULT_TERMINATION_GRACE, ProcessRegistry, TerminationFailure, TerminationReport,
};
pub use request::{
    CommandSpec, DEFAULT_MAX_OUTPUT_BYTES, Mount, MountAccess, NetworkMode, ResourceLimits,
    SandboxRequest,
};
pub use sandbox::{HELPER_BINARY_NAME, Sandbox, default_helper_path, negotiate, probe};
