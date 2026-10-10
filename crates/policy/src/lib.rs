//! Layered, deterministic policy evaluation for brokered agent actions.
//!
//! This stage adds closed-schema TOML policy documents, workspace project rules,
//! and the policy set that holds them.

mod action;
mod classify;
mod context;
mod decision;
mod document;
mod error;
mod facts;
mod fingerprint;
mod hard;
mod intent;
mod macros;
mod network;
mod profile;
mod rules;

pub use action::{Action, ActionPattern};
pub use classify::{
    NetworkKind, NetworkTarget, REDACTED, ShellClassification, ShellPlatform, ShellRisk,
    classify_network, classify_shell, redact_command,
};
pub use context::{EvaluationContext, HostTrust, PrivacyMode};
pub use decision::{
    ApprovalScope, DEFAULT_APPROVAL_EXPIRY, Decision, RateLimit, RuleOrigin, RuleRef, SandboxLevel,
    Verdict,
};
pub use document::{
    MAX_DOCUMENT_BYTES, PROJECT_RULES_FILE, PolicyDocument, PolicySet, ProjectInfo, ProjectRules,
    SCHEMA_VERSION,
};
pub use error::PolicyError;
pub use hard::{HARD_RULES, HardRule, HardVerdict};
pub use intent::{
    Intent, IntentAttributes, ModelLocality, PathClass, PathScope, SecretUse, ShellCommand,
};
pub use network::{DomainPattern, NetworkAccess, NetworkMode, NetworkSettings};
pub use profile::Profile;
pub use rules::{Conditions, Layer, PathGlobs, PathLocation, Rule};
