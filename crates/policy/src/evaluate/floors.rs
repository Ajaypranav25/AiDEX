//! Tightening-only floors: hard approvals, project rules, privacy, network, host trust.

use crate::{
    Action, EvaluationContext, HostTrust, ModelLocality, NetworkAccess, NetworkKind, NetworkMode,
    PolicySet, PrivacyMode, Verdict,
    decision::{Outcome, RuleOrigin, RuleRef},
    facts::{Facts, NetworkFact},
    hard,
};

/// Where a unit's traffic goes, if anywhere.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Egress {
    None,
    Loopback,
    LocalNetwork,
    External,
}

fn egress(unit: &Facts<'_>) -> Egress {
    match unit.network {
        NetworkFact::Target(target) => match target.kind() {
            NetworkKind::Loopback => Egress::Loopback,
            NetworkKind::LocalNetwork => Egress::LocalNetwork,
            NetworkKind::PublicIp | NetworkKind::Domain => Egress::External,
        },
        NetworkFact::Unknown => Egress::External,
        NetworkFact::Absent
            if unit.action == Action::ModelInvoke
                && unit.locality() == Some(ModelLocality::Remote) =>
        {
            Egress::External
        }
        NetworkFact::Absent => Egress::None,
    }
}

pub(super) fn collect(
    policies: &PolicySet,
    context: &EvaluationContext,
    unit: &Facts<'_>,
) -> Vec<Outcome> {
    let mut floors = Vec::new();
    hard::floors_for(unit, &mut floors);
    project(policies, unit, &mut floors);
    let egress = egress(unit);
    privacy(context.effective_privacy(), egress, &mut floors);
    network(
        context.effective_network_mode(),
        policies,
        unit,
        egress,
        &mut floors,
    );
    trust(context.host_trust, unit, &mut floors);
    floors
}

fn project(policies: &PolicySet, unit: &Facts<'_>, floors: &mut Vec<Outcome>) {
    let Some(workspace) = &unit.intent.workspace_id else {
        return;
    };
    let Some(rules) = policies.project_rules(workspace) else {
        return;
    };
    let reference = |id: &str| {
        let mut rule = RuleRef::new(RuleOrigin::Project, workspace.as_str(), id);
        rule.source = Some(rules.source.clone());
        rule
    };
    let protected = unit.action.is_filesystem_write()
        && unit
            .path
            .and_then(|path| path.workspace_relative.as_deref())
            .is_some_and(|relative| rules.protects(relative));
    if protected {
        floors.push(Outcome::deny(
            reference("project.never-modify"),
            "project rule: this path is listed in `never_modify`",
        ));
    }
    if rules
        .require_approval()
        .iter()
        .any(|pattern| pattern.matches(unit.action))
    {
        floors.push(Outcome::new(
            Verdict::approve_once_or_session(),
            reference("project.require-approval"),
            format!(
                "project rule: {} requires approval in this workspace",
                unit.action
            ),
        ));
    }
}

fn privacy(mode: PrivacyMode, egress: Egress, floors: &mut Vec<Outcome>) {
    let reference = |id: &str| RuleRef::new(RuleOrigin::Privacy, mode.as_str(), id);
    match mode {
        PrivacyMode::AirGapped if egress != Egress::None => floors.push(Outcome::deny(
            reference("privacy.air-gapped"),
            "air-gapped privacy mode disables all networking",
        )),
        PrivacyMode::LocalOnly if egress == Egress::External => floors.push(Outcome::deny(
            reference("privacy.local-only"),
            "local-only privacy mode forbids external models and services",
        )),
        _ => {}
    }
}

fn network(
    mode: NetworkMode,
    policies: &PolicySet,
    unit: &Facts<'_>,
    egress: Egress,
    floors: &mut Vec<Outcome>,
) {
    if mode == NetworkMode::Unrestricted {
        return;
    }
    let settings = policies.network();
    let reference = |id: &str| RuleRef::new(RuleOrigin::Network, mode.as_str(), id);
    let access = |access: NetworkAccess, id: &str, what: &str| match access {
        NetworkAccess::Allow => None,
        NetworkAccess::Ask => Some(Outcome::new(
            Verdict::approve_once_or_session(),
            reference(id),
            format!("{what} require approval in {mode} network mode"),
        )),
        NetworkAccess::Deny => Some(Outcome::deny(
            reference(id),
            format!("{what} are denied in {mode} network mode"),
        )),
    };
    let floor = match egress {
        Egress::None | Egress::Loopback => None,
        Egress::LocalNetwork => access(
            settings.local_network,
            "network.local-network",
            "local-network destinations",
        ),
        Egress::External => match (mode, unit.network) {
            (NetworkMode::Offline, _) => Some(Outcome::deny(
                reference("network.offline"),
                "offline network mode permits only loopback and allowed local-network destinations",
            )),
            (_, NetworkFact::Target(target)) if target.kind() == NetworkKind::PublicIp => access(
                settings.arbitrary_ip,
                "network.arbitrary-ip",
                "literal public IP destinations",
            ),
            (NetworkMode::Allowlist, NetworkFact::Target(target)) => {
                (!settings.allows_domain(target.host())).then(|| {
                    Outcome::deny(
                        reference("network.allowlist"),
                        format!("`{}` is not in the network allowlist", target.host()),
                    )
                })
            }
            (NetworkMode::Allowlist, _) => Some(Outcome::deny(
                reference("network.allowlist.unknown"),
                "the destination is unknown, so the allowlist cannot permit it",
            )),
            (_, NetworkFact::Target(_)) => None,
            (_, _) => Some(Outcome::new(
                Verdict::approve_once_or_session(),
                reference("network.unknown-destination"),
                "the network destination could not be determined",
            )),
        },
    };
    floors.extend(floor);
}

fn trust(trust: HostTrust, unit: &Facts<'_>, floors: &mut Vec<Outcome>) {
    if matches!(trust, HostTrust::Trusted | HostTrust::Known) {
        return;
    }
    let reference = |id: &str| RuleRef::new(RuleOrigin::HostTrust, trust.as_str(), id);
    let untrusted = trust == HostTrust::Untrusted;
    let restricted = matches!(trust, HostTrust::Restricted | HostTrust::Untrusted);
    match unit.action {
        Action::SecretUse if untrusted => floors.push(Outcome::deny(
            reference("trust.secret-use"),
            "secrets are unavailable on an untrusted host",
        )),
        Action::SecretUse => floors.push(Outcome::new(
            Verdict::approve_once(),
            reference("trust.secret-use"),
            format!("secret use requires one-time approval on a {trust} host"),
        )),
        Action::ModelInvoke if unit.locality() == Some(ModelLocality::Remote) => {
            floors.push(Outcome::new(
                Verdict::approve_once(),
                reference("trust.remote-provider"),
                format!("remote provider credentials require approval on a {trust} host"),
            ));
        }
        Action::HostMount | Action::RemoteAction if restricted => floors.push(Outcome::deny(
            reference("trust.host-exposure"),
            format!("{} is denied on a {trust} host", unit.action),
        )),
        Action::PackageInstall if restricted => floors.push(Outcome::new(
            Verdict::approve_once(),
            reference("trust.package-install"),
            format!("package installs require approval on a {trust} host"),
        )),
        _ => {}
    }
}
