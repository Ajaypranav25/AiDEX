//! Classifiers that turn raw command lines and destinations into policy facts.

mod files;
mod git;
mod interpreters;
mod lexer;
mod net;
mod network;
mod packages;
mod programs;
mod redact;
mod shell;
mod system;

pub use network::{NetworkKind, NetworkTarget, classify_network};
pub use redact::{REDACTED, redact_command};
pub use shell::{ShellClassification, ShellPlatform, ShellRisk, classify_shell};
