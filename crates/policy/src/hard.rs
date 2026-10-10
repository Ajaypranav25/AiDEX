//! Immutable hard safety rules (spec §50). No layer, profile, or approval overrides them.

use crate::{
    Action, PathClass, ShellRisk, Verdict,
    decision::{Outcome, RuleOrigin, RuleRef},
    facts::Facts,
};

/// A documented hard safety rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HardRule {
    pub id: &'static str,
    /// `Deny` rules always deny; `RequireApproval` rules cap any looser verdict at a
    /// one-time approval.
    pub verdict: HardVerdict,
    pub description: &'static str,
}

/// The fixed outcome of a hard rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardVerdict {
    Deny,
    RequireApproval,
}

/// Every hard rule, in evaluation order.
pub const HARD_RULES: [HardRule; 11] = [
    deny(
        "hard.disk-format",
        "Formatting or repartitioning a disk is never permitted.",
    ),
    deny(
        "hard.drive-raw-write",
        "Writing raw data to a block device is never permitted.",
    ),
    deny(
        "hard.credential-export",
        "Exporting credentials is never permitted.",
    ),
    deny(
        "hard.secret-dump",
        "Dumping stored secrets in bulk is never permitted.",
    ),
    deny(
        "hard.directory-delete-arbitrary",
        "Deleting a root, home, or system directory is never permitted.",
    ),
    deny(
        "hard.vault-modify",
        "Agents may never modify the AiDEX vault directly.",
    ),
    deny(
        "hard.broker-disable",
        "The execution broker can never be disabled by policy.",
    ),
    deny(
        "hard.protected-path",
        "Filesystem actions on AiDEX-protected state (vault, identity, broker runtime) are never permitted.",
    ),
    approval(
        "hard.policy-change-authorization",
        "Security policy changes always need explicit one-time human authorization.",
    ),
    approval(
        "hard.destructive-command",
        "Destructive commands always need one-time approval, even in the Autonomous profile.",
    ),
    approval(
        "hard.destructive-effect",
        "Actions the broker marks irreversible always need one-time approval.",
    ),
];

const fn deny(id: &'static str, description: &'static str) -> HardRule {
    HardRule {
        id,
        verdict: HardVerdict::Deny,
        description,
    }
}

const fn approval(id: &'static str, description: &'static str) -> HardRule {
    HardRule {
        id,
        verdict: HardVerdict::RequireApproval,
        description,
    }
}

fn hard_ref(index: usize) -> (RuleRef, &'static str) {
    let rule = HARD_RULES[index];
    (
        RuleRef::new(RuleOrigin::Hard, "", rule.id),
        rule.description,
    )
}

/// The hard deny that applies to `facts`, if any.
pub(crate) fn deny_for(facts: &Facts<'_>) -> Option<Outcome> {
    let index = match facts.action {
        Action::DiskFormat => 0,
        Action::DriveRawWrite => 1,
        Action::CredentialExport => 2,
        Action::SecretDump => 3,
        Action::DirectoryDeleteArbitrary => 4,
        Action::VaultModify => 5,
        Action::BrokerDisable => 6,
        action
            if action.is_filesystem()
                && facts
                    .path
                    .is_some_and(|path| path.class == PathClass::Protected) =>
        {
            7
        }
        _ => return None,
    };
    let (rule, description) = hard_ref(index);
    Some(Outcome::deny(
        rule,
        format!("hard safety rule: {description}"),
    ))
}

/// Hard approval floors that apply to `facts`.
pub(crate) fn floors_for(facts: &Facts<'_>, floors: &mut Vec<Outcome>) {
    let mut push = |index: usize| {
        let (rule, description) = hard_ref(index);
        floors.push(Outcome::new(
            Verdict::approve_once(),
            rule,
            format!("hard safety rule: {description}"),
        ));
    };
    if facts.action == Action::PolicyChange {
        push(8);
    }
    if facts
        .command()
        .is_some_and(|command| command.risk == ShellRisk::Destructive)
    {
        push(9);
    }
    if facts.destructive() {
        push(10);
    }
}
