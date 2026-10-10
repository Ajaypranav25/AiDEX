//! Layered, deterministic policy evaluation for brokered agent actions.
//!
//! This stage provides the closed action catalogue and the shell and network
//! classifiers that turn raw command lines and destinations into policy facts.

mod action;
mod classify;
mod error;
mod macros;

pub use action::{Action, ActionPattern};
pub use classify::{
    NetworkKind, NetworkTarget, REDACTED, ShellClassification, ShellPlatform, ShellRisk,
    classify_network, classify_shell, redact_command,
};
pub use error::PolicyError;
