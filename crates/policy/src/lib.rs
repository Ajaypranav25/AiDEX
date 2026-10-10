//! Layered, deterministic policy evaluation for brokered agent actions.
//!
//! This crate decides; it does not enforce. The execution broker submits a
//! normalized [`Intent`] (with facts it computed: resolved paths, a classified
//! command, a classified destination) and receives a [`Decision`]: allow, deny,
//! sandbox, rate-limit, or require approval, with the deciding rule and a
//! human-readable effect summary. Immutable hard safety rules, deny-wins layer
//! precedence, built-in profiles, privacy modes, host trust, network modes, and
//! tightening-only project rules are combined by [`evaluate`]; see the crate
//! README for the precedence table.

mod action;
mod classify;
mod context;
mod decision;
mod document;
mod error;
mod evaluate;
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
pub use evaluate::evaluate;
pub use hard::{HARD_RULES, HardRule, HardVerdict};
pub use intent::{
    Intent, IntentAttributes, ModelLocality, PathClass, PathScope, SecretUse, ShellCommand,
};
pub use network::{DomainPattern, NetworkAccess, NetworkMode, NetworkSettings};
pub use profile::Profile;
pub use rules::{Conditions, Layer, PathGlobs, PathLocation, Rule};
