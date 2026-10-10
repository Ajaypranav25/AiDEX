//! Layered, deterministic policy evaluation for brokered agent actions.
//!
//! This stage adds normalized intents with SHA-256 fingerprints and the verdict
//! and decision types policy evaluation returns.

mod action;
mod classify;
mod decision;
mod error;
mod fingerprint;
mod intent;
mod macros;

pub use action::{Action, ActionPattern};
pub use classify::{
    NetworkKind, NetworkTarget, REDACTED, ShellClassification, ShellPlatform, ShellRisk,
    classify_network, classify_shell, redact_command,
};
pub use decision::{
    ApprovalScope, DEFAULT_APPROVAL_EXPIRY, Decision, RateLimit, RuleOrigin, RuleRef, SandboxLevel,
    Verdict,
};
pub use error::PolicyError;
pub use intent::{
    Intent, IntentAttributes, ModelLocality, PathClass, PathScope, SecretUse, ShellCommand,
};
