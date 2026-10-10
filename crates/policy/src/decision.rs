//! Verdicts, decisions, rule references, and the strictness order used to combine them.

use std::{fmt, time::Duration};

use serde::{Deserialize, Serialize};

use crate::macros::string_enum;

/// Default lifetime of a pending approval request.
pub const DEFAULT_APPROVAL_EXPIRY: Duration = Duration::from_secs(600);

string_enum! {
    /// Isolation level an allowed action runs at (spec §12.1), weakest first.
    pub enum SandboxLevel("sandbox level") {
        /// Runs directly as the current user. Never chosen by policy.
        Direct => "direct",
        /// Every sensitive action passes through the broker (the floor for agents).
        Brokered => "brokered",
        /// Native OS restrictions (job objects, namespaces, allowlists, limits).
        Restricted => "restricted",
        /// A stronger isolated environment (container, VM, OS sandbox facility).
        Ephemeral => "ephemeral",
    }
}

string_enum! {
    /// How long a human approval may apply.
    pub enum ApprovalScope("approval scope") {
        /// This exact intent (bound by fingerprint), once.
        Once => "once",
        /// Identical intents for the rest of the session.
        Session => "session",
    }
}

/// A throttle on how often an allowed action may run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimit {
    pub max: u32,
    #[serde(rename = "per_secs", with = "duration_secs")]
    pub per: Duration,
}

impl RateLimit {
    /// Whether `self` permits fewer runs per unit of time than `other`.
    fn stricter_than(self, other: Self) -> bool {
        u128::from(self.max) * other.per.as_millis() < u128::from(other.max) * self.per.as_millis()
    }
}

/// What the broker must do with an intent.
///
/// The spec's `REWRITE` outcome is intentionally not supported: silently changing
/// what an agent asked for makes the executed action differ from the one shown,
/// approved, fingerprinted, and audited. Policy may only allow, constrain, ask, or deny.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Verdict {
    Allow,
    Deny,
    /// Allowed only at `level` isolation or stronger.
    Sandbox {
        level: SandboxLevel,
    },
    /// A human must approve; `scope_options` lists what the approver may choose.
    RequireApproval {
        scope_options: Vec<ApprovalScope>,
        #[serde(rename = "expires_in_secs", with = "duration_secs")]
        expires_in: Duration,
    },
    /// Allowed at most `max` times per `per`.
    RateLimit {
        max: u32,
        #[serde(rename = "per_secs", with = "duration_secs")]
        per: Duration,
    },
}

impl Verdict {
    /// Require approval with the given scopes and the default expiry.
    pub fn approval(scopes: &[ApprovalScope]) -> Self {
        Self::RequireApproval {
            scope_options: scopes.to_vec(),
            expires_in: DEFAULT_APPROVAL_EXPIRY,
        }
    }

    /// Require approval that can only apply once.
    pub fn approve_once() -> Self {
        Self::approval(&[ApprovalScope::Once])
    }

    /// Require approval that may apply once or for the session.
    pub fn approve_once_or_session() -> Self {
        Self::approval(&[ApprovalScope::Once, ApprovalScope::Session])
    }

    pub fn sandbox(level: SandboxLevel) -> Self {
        Self::Sandbox { level }
    }

    /// Strictness rank: Allow < RateLimit < Sandbox < RequireApproval < Deny.
    pub fn rank(&self) -> u8 {
        match self {
            Self::Allow => 0,
            Self::RateLimit { .. } => 1,
            Self::Sandbox { .. } => 2,
            Self::RequireApproval { .. } => 3,
            Self::Deny => 4,
        }
    }

    pub fn is_deny(&self) -> bool {
        matches!(self, Self::Deny)
    }

    pub fn requires_approval(&self) -> bool {
        matches!(self, Self::RequireApproval { .. })
    }

    /// The action may run without a human (possibly constrained).
    pub fn permits_execution(&self) -> bool {
        matches!(
            self,
            Self::Allow | Self::Sandbox { .. } | Self::RateLimit { .. }
        )
    }
}

string_enum! {
    /// Which part of the precedence algorithm produced a decision.
    pub enum RuleOrigin("rule origin") {
        /// An immutable hard safety rule.
        Hard => "hard",
        /// A rule in a policy layer (global ... session).
        Layer => "layer",
        /// A tightening-only project rule from the workspace.
        Project => "project",
        /// The active built-in profile.
        Profile => "profile",
        /// The privacy mode.
        Privacy => "privacy",
        /// The host trust level.
        HostTrust => "host_trust",
        /// The network mode and settings.
        Network => "network",
        /// Deny-by-default (no rule matched, or required facts were missing).
        Default => "default",
    }
}

/// Identifies the rule behind a decision.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleRef {
    pub origin: RuleOrigin,
    /// Layer name, profile id, privacy mode, trust level, or network mode.
    pub scope: String,
    pub id: String,
    /// Policy file the rule was loaded from, when it came from disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl RuleRef {
    pub(crate) fn new(origin: RuleOrigin, scope: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            origin,
            scope: scope.into(),
            id: id.into(),
            source: None,
        }
    }
}

impl fmt::Display for RuleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scope.is_empty() {
            write!(f, "{}:{}", self.origin, self.id)
        } else {
            write!(f, "{}:{}:{}", self.origin, self.scope, self.id)
        }
    }
}

/// The result of evaluating an intent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub verdict: Verdict,
    /// Why this verdict was reached, naming the deciding rule.
    pub reason: String,
    pub rule: RuleRef,
    /// Human-readable description of what the action would do.
    pub effect_summary: String,
    /// Minimum isolation if the action proceeds, including after approval.
    pub sandbox: SandboxLevel,
    /// Strictest rate limit a contributing rule imposed, kept even when approval is required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimit>,
}

/// An intermediate verdict with the constraints accumulated so far.
#[derive(Clone, Debug)]
pub(crate) struct Outcome {
    pub verdict: Verdict,
    pub rule: RuleRef,
    pub reason: String,
    pub sandbox: SandboxLevel,
    pub rate_limit: Option<RateLimit>,
}

impl Outcome {
    pub fn new(verdict: Verdict, rule: RuleRef, reason: impl Into<String>) -> Self {
        let sandbox = match verdict {
            Verdict::Sandbox { level } => level,
            _ => SandboxLevel::Direct,
        };
        let rate_limit = match verdict {
            Verdict::RateLimit { max, per } => Some(RateLimit { max, per }),
            _ => None,
        };
        Self {
            verdict,
            rule,
            reason: reason.into(),
            sandbox,
            rate_limit,
        }
    }

    pub fn deny(rule: RuleRef, reason: impl Into<String>) -> Self {
        Self::new(Verdict::Deny, rule, reason)
    }

    /// Keep the stricter verdict (ties keep `self`) and the union of constraints.
    pub fn combine(self, other: Self) -> Self {
        let sandbox = self.sandbox.max(other.sandbox);
        let rate_limit = match (self.rate_limit, other.rate_limit) {
            (Some(a), Some(b)) => Some(if b.stricter_than(a) { b } else { a }),
            (a, b) => a.or(b),
        };
        let mut winner = match self.verdict.rank().cmp(&other.verdict.rank()) {
            std::cmp::Ordering::Greater => self,
            std::cmp::Ordering::Less => other,
            std::cmp::Ordering::Equal => tie(self, other),
        };
        winner.sandbox = sandbox;
        winner.rate_limit = rate_limit;
        winner
    }
}

fn tie(first: Outcome, second: Outcome) -> Outcome {
    match (&first.verdict, &second.verdict) {
        (
            Verdict::RequireApproval {
                scope_options: a,
                expires_in: a_expiry,
            },
            Verdict::RequireApproval {
                scope_options: b,
                expires_in: b_expiry,
            },
        ) => {
            let mut scopes: Vec<ApprovalScope> = ApprovalScope::ALL
                .iter()
                .copied()
                .filter(|scope| a.contains(scope) && b.contains(scope))
                .collect();
            if scopes.is_empty() {
                scopes.push(ApprovalScope::Once);
            }
            let expires_in = (*a_expiry).min(*b_expiry);
            let mut winner = if b.len() < a.len() { second } else { first };
            winner.verdict = Verdict::RequireApproval {
                scope_options: scopes,
                expires_in,
            };
            winner
        }
        (Verdict::Sandbox { level: a }, Verdict::Sandbox { level: b }) if b > a => second,
        (Verdict::RateLimit { max: a, per: a_per }, Verdict::RateLimit { max: b, per: b_per })
            if RateLimit {
                max: *b,
                per: *b_per,
            }
            .stricter_than(RateLimit {
                max: *a,
                per: *a_per,
            }) =>
        {
            second
        }
        _ => first,
    }
}

pub(crate) mod duration_secs {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(value.as_secs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        u64::deserialize(deserializer).map(Duration::from_secs)
    }
}
