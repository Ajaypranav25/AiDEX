//! Shell command classification: risk, effects, and the actions a command implies.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    lexer::{Dialect, lex},
    network::{NetworkTarget, classify_network},
    programs::{SegmentFacts, classify_segment},
};
use crate::{Action, macros::string_enum};

/// Nested shells (`sh -c`, `cmd /c`, `eval`, ...) deeper than this are not unpacked.
const MAX_DEPTH: usize = 4;
/// Longer command lines are not lexed; they are classified as high risk.
const MAX_COMMAND_LEN: usize = 64 * 1024;

string_enum! {
    /// Shell syntax family. `Windows` classifies with both cmd.exe and PowerShell
    /// rules and keeps the stricter result, because either may run the line.
    pub enum ShellPlatform("shell platform") {
        Unix => "unix",
        Windows => "windows",
    }
}

impl ShellPlatform {
    /// The platform this build targets.
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

string_enum! {
    /// Ordered command risk. Unknown programs are at least `Medium`.
    pub enum ShellRisk("shell risk") {
        /// Read-only inspection.
        Low => "low",
        /// Local writes, builds, tests, unknown programs, ordinary network fetches.
        Medium => "medium",
        /// Remote writes, privilege elevation, host configuration, obfuscation.
        High => "high",
        /// Irreversible data loss (recursive deletes, history rewrites, disk writes).
        Destructive => "destructive",
    }
}

/// The result of classifying one command line. Every fact is derived from the
/// command text by [`classify_shell`]; none is taken from the requesting agent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellClassification {
    /// The first program the line runs (empty for an empty line).
    pub program: String,
    /// Every distinct program the line runs, in order of appearance.
    pub programs: Vec<String>,
    pub risk: ShellRisk,
    /// Human-readable reasons for the risk; they never quote argument values.
    pub reasons: Vec<String>,
    /// The command contacts the network.
    pub network: bool,
    /// The command modifies files or system state.
    pub writes: bool,
    /// Brokered actions the command performs in addition to running (e.g.
    /// `package.install` for `npm install`, `disk.format` for `mkfs`).
    pub implied_actions: Vec<Action>,
    /// Destinations named in URLs or remote-host arguments.
    pub network_targets: Vec<NetworkTarget>,
}

/// Classify a command line for `platform`.
///
/// The classifier is deliberately conservative: it unpacks nested shells,
/// substitutions, and pipelines, treats unknown programs as at least `Medium`, and
/// raises risk for obfuscation (decoded payloads, `eval`, encoded PowerShell,
/// runtime-computed program names). It is a heuristic, not a shell parser; the
/// sandbox remains responsible for containing what it misses.
pub fn classify_shell(command: &str, platform: ShellPlatform) -> ShellClassification {
    let mut acc = Accumulator::default();
    if command.trim().is_empty() {
        acc.raise(ShellRisk::Medium, "empty command");
    } else if command.len() > MAX_COMMAND_LEN {
        acc.raise(ShellRisk::High, "command is too long to classify");
    } else {
        let dialects: &[Dialect] = match platform {
            ShellPlatform::Unix => &[Dialect::Posix],
            ShellPlatform::Windows => &[Dialect::Cmd, Dialect::PowerShell],
        };
        for dialect in dialects {
            classify_text(command, *dialect, 0, &mut acc);
        }
    }
    acc.finish()
}

/// Classify a (possibly nested) command string into `acc`.
pub(crate) fn classify_text(text: &str, dialect: Dialect, depth: usize, acc: &mut Accumulator) {
    if depth > MAX_DEPTH {
        acc.raise(ShellRisk::High, "command nesting is too deep to classify");
        return;
    }
    let lexed = lex(text, dialect);
    if lexed.unbalanced {
        acc.raise(ShellRisk::High, "unbalanced quoting or substitution");
    }
    if lexed.substitution {
        acc.raise(ShellRisk::Medium, "uses command substitution");
    }
    for inner in &lexed.embedded {
        classify_text(inner, dialect, depth + 1, acc);
    }
    let mut pipeline = SegmentFacts::default();
    for segment in &lexed.segments {
        if !segment.piped {
            pipeline = SegmentFacts::default();
        }
        let mut facts = SegmentFacts::default();
        classify_segment(segment, dialect, depth, acc, &mut facts);
        if segment.piped && facts.stdin_executor {
            acc.raise(ShellRisk::Medium, "pipes data into an interpreter");
            if pipeline.fetcher {
                acc.raise(
                    ShellRisk::High,
                    "pipes downloaded content into an interpreter",
                );
            }
            if pipeline.decoder {
                acc.raise(
                    ShellRisk::High,
                    "pipes decoded (obfuscated) content into an interpreter",
                );
            }
            if let Some(echoed) = pipeline.echo_text.take() {
                classify_text(&echoed, dialect, depth + 1, acc);
            }
        }
        pipeline.fetcher |= facts.fetcher;
        pipeline.decoder |= facts.decoder;
        if facts.echo_text.is_some() {
            pipeline.echo_text = facts.echo_text;
        }
    }
}

/// Mutable classification state shared by every segment and nesting level.
#[derive(Debug, Default)]
pub(crate) struct Accumulator {
    risk: Option<ShellRisk>,
    reasons: Vec<String>,
    network: bool,
    writes: bool,
    implied: BTreeSet<Action>,
    programs: Vec<String>,
    targets: Vec<NetworkTarget>,
    /// Some segment downloads remote content.
    pub fetcher: bool,
    /// Some segment evaluates a string as code (`eval`, `iex`, `-c`, `-e`).
    pub evaluator: bool,
    /// Some segment decodes base64/hex payloads.
    pub decoder: bool,
    /// Some word references credential material.
    pub credentials: bool,
}

impl Accumulator {
    pub(crate) fn raise(&mut self, risk: ShellRisk, reason: impl Into<String>) {
        self.risk = Some(self.risk.map_or(risk, |current| current.max(risk)));
        let reason = reason.into();
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }

    pub(crate) fn current_risk(&self) -> ShellRisk {
        self.risk.unwrap_or(ShellRisk::Low)
    }

    pub(crate) fn imply(&mut self, action: Action) {
        self.implied.insert(action);
    }

    pub(crate) fn network(&mut self) {
        self.network = true;
    }

    pub(crate) fn writes(&mut self) {
        self.writes = true;
    }

    pub(crate) fn program(&mut self, program: &str) {
        if !self.programs.iter().any(|known| known == program) {
            self.programs.push(program.to_owned());
        }
    }

    /// Record a destination; hosts that cannot be classified are ignored, which
    /// leaves the network effect without a known target (treated as unknown).
    pub(crate) fn target(&mut self, host: &str, port: u16) {
        if let Ok(target) = classify_network(host, port)
            && !self.targets.contains(&target)
        {
            self.targets.push(target);
        }
    }

    /// Merge `other`, first raising its risk by one level (used for `eval`-like
    /// evaluation of constructed code).
    pub(crate) fn merge_escalated(&mut self, other: Self, reason: &str) {
        let inner = other.current_risk();
        let escalated = next(inner);
        for reason in other.reasons {
            self.raise(inner, reason);
        }
        self.raise(escalated, reason);
        self.network |= other.network;
        self.writes |= other.writes;
        self.implied.extend(other.implied);
        for program in other.programs {
            self.program(&program);
        }
        for target in other.targets {
            if !self.targets.contains(&target) {
                self.targets.push(target);
            }
        }
        self.fetcher |= other.fetcher;
        self.evaluator |= other.evaluator;
        self.decoder |= other.decoder;
        self.credentials |= other.credentials;
    }

    fn finish(mut self) -> ShellClassification {
        if self.fetcher && self.evaluator {
            self.raise(ShellRisk::High, "evaluates remotely fetched code");
        }
        if self.decoder && self.evaluator {
            self.raise(ShellRisk::High, "evaluates a decoded (obfuscated) payload");
        }
        if self.credentials && self.network {
            self.imply(Action::CredentialExport);
            self.raise(
                ShellRisk::Destructive,
                "may send credential material over the network",
            );
        }
        if self.risk.is_none() {
            self.raise(ShellRisk::Low, "read-only command");
        }
        ShellClassification {
            program: self.programs.first().cloned().unwrap_or_default(),
            programs: self.programs,
            risk: self.risk.unwrap_or(ShellRisk::Medium),
            reasons: self.reasons,
            network: self.network,
            writes: self.writes,
            implied_actions: self.implied.into_iter().collect(),
            network_targets: self.targets,
        }
    }
}

fn next(risk: ShellRisk) -> ShellRisk {
    match risk {
        ShellRisk::Low => ShellRisk::Medium,
        ShellRisk::Medium => ShellRisk::High,
        ShellRisk::High | ShellRisk::Destructive => ShellRisk::Destructive,
    }
}
