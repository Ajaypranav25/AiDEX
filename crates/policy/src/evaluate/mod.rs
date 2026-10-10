//! The pure, deterministic precedence algorithm.
//!
//! 1. Hard deny rules (any unit) → `Deny`. Nothing overrides them.
//! 2. Missing or malformed facts (any unit) → `Deny` (fail closed).
//! 3. Per unit, the base verdict: any matching layer `Deny` wins; otherwise the
//!    most specific layer with a matching rule (session > tool > skill > agent >
//!    workspace > machine > global; strictest rule within that layer); otherwise
//!    the effective profile's strictest matching rule; otherwise deny-by-default.
//! 4. Floors tighten the base and can never loosen it: hard approval rules,
//!    project rules, privacy mode, network mode, host trust.
//! 5. Units combine by strictness; the profile's baseline sandbox applies.

mod floors;
mod summary;

use crate::{
    Action, Decision, EvaluationContext, Intent, PolicySet, Profile, SandboxLevel, Verdict,
    decision::{Outcome, RuleOrigin, RuleRef},
    facts::{Facts, NetworkFact},
    hard,
};

/// Evaluate `intent` against `policies` in `context`.
///
/// Pure and deterministic: no I/O, clock, or randomness; the same inputs always
/// produce the same decision. Evaluation never fails — incomplete intents are denied.
pub fn evaluate(policies: &PolicySet, context: &EvaluationContext, intent: &Intent) -> Decision {
    let profile = context.effective_profile();
    let units = units(intent);
    let outcome = units
        .iter()
        .find_map(hard::deny_for)
        .or_else(|| units.iter().find_map(incomplete))
        .unwrap_or_else(|| {
            units
                .iter()
                .map(|unit| evaluate_unit(policies, context, profile, unit))
                .reduce(Outcome::combine)
                .unwrap_or_else(|| default_deny(intent.action))
        });
    finish(outcome, profile, intent)
}

/// Split an intent into evaluation units: its own action once per path, plus each
/// action its command implies, once per known destination.
fn units(intent: &Intent) -> Vec<Facts<'_>> {
    let attributes = &intent.attributes;
    let own_network = match &attributes.network {
        Some(target) => NetworkFact::Target(target),
        None if network_bearing(intent.action) && intent.action != Action::NetworkRequest => {
            NetworkFact::Unknown
        }
        None => NetworkFact::Absent,
    };
    let mut units = Vec::new();
    if attributes.paths.is_empty() {
        units.push(Facts {
            action: intent.action,
            intent,
            path: None,
            network: own_network,
        });
    }
    for path in &attributes.paths {
        units.push(Facts {
            action: intent.action,
            intent,
            path: Some(path),
            network: own_network,
        });
    }
    let Some(command) = &attributes.command else {
        return units;
    };
    let classification = command.classification();
    for &action in &classification.implied_actions {
        if action == intent.action {
            continue;
        }
        let unit = |network| Facts {
            action,
            intent,
            path: None,
            network,
        };
        if !network_bearing(action) {
            units.push(unit(NetworkFact::Absent));
        } else if let Some(target) = &attributes.network {
            units.push(unit(NetworkFact::Target(target)));
        } else if classification.network_targets.is_empty() {
            units.push(unit(NetworkFact::Unknown));
        } else {
            units.extend(
                classification
                    .network_targets
                    .iter()
                    .map(|target| unit(NetworkFact::Target(target))),
            );
        }
    }
    units
}

/// Actions that reach the network even when the intent names no destination.
fn network_bearing(action: Action) -> bool {
    matches!(
        action,
        Action::NetworkRequest | Action::GitPush | Action::GitPull | Action::PackageInstall
    )
}

fn incomplete(unit: &Facts<'_>) -> Option<Outcome> {
    let attributes = &unit.intent.attributes;
    let action = unit.action;
    let primary = action == unit.intent.action;
    let missing = if action.is_filesystem() && unit.path.is_none() {
        Some("it names no resolved path")
    } else if unit.path.is_some_and(|path| {
        path.workspace_relative
            .as_deref()
            .is_some_and(|relative| !valid_relative(relative))
    }) {
        Some("a workspace-relative path is absolute, uses `\\`, or escapes the workspace")
    } else if action.is_command() && attributes.command.is_none() {
        Some("it has no classified command")
    } else if primary && action == Action::NetworkRequest && attributes.network.is_none() {
        Some("it names no classified destination")
    } else if action == Action::ModelInvoke && attributes.model_locality.is_none() {
        Some("model locality is unknown")
    } else if action == Action::SecretUse && attributes.secret.is_none() {
        Some("it names no secret")
    } else {
        None
    };
    missing.map(|why| {
        Outcome::deny(
            RuleRef::new(RuleOrigin::Default, "", "default.incomplete-intent"),
            format!("{action} intent is incomplete: {why} (fail closed)"),
        )
    })
}

fn valid_relative(relative: &str) -> bool {
    !(relative.starts_with('/')
        || relative.contains('\\')
        || relative.as_bytes().get(1) == Some(&b':')
        || relative.split('/').any(|part| part == ".."))
}

fn evaluate_unit(
    policies: &PolicySet,
    context: &EvaluationContext,
    profile: Profile,
    unit: &Facts<'_>,
) -> Outcome {
    let base = layered(policies, context, unit)
        .or_else(|| profile_default(profile, unit))
        .unwrap_or_else(|| default_deny(unit.action));
    floors::collect(policies, context, unit)
        .into_iter()
        .fold(base, Outcome::combine)
}

/// Deny wins across layers; otherwise the most specific layer with a match.
fn layered(policies: &PolicySet, context: &EvaluationContext, unit: &Facts<'_>) -> Option<Outcome> {
    let mut deny = None;
    let mut most_specific = None;
    for (layer, rules) in policies.applicable(context, unit.intent) {
        let matched = rules
            .iter()
            .filter(|loaded| loaded.rule.matches(unit))
            .map(|loaded| {
                let mut rule = RuleRef::new(RuleOrigin::Layer, layer.as_str(), &loaded.rule.id);
                rule.source.clone_from(&loaded.source);
                let reason = format!(
                    "{layer} rule `{}`: {}",
                    loaded.rule.id,
                    loaded.rule.reason.as_deref().unwrap_or("matched")
                );
                Outcome::new(loaded.rule.verdict.clone(), rule, reason)
            })
            .reduce(Outcome::combine);
        if let Some(outcome) = matched {
            if outcome.verdict.is_deny() {
                deny = Some(outcome.clone());
            }
            most_specific = Some(outcome);
        }
    }
    deny.or(most_specific)
}

fn profile_default(profile: Profile, unit: &Facts<'_>) -> Option<Outcome> {
    profile
        .rules()
        .iter()
        .filter(|rule| rule.matches(unit))
        .map(|rule| {
            Outcome::new(
                rule.verdict.clone(),
                RuleRef::new(RuleOrigin::Profile, profile.as_str(), &rule.id),
                format!("{profile} profile default `{}`", rule.id),
            )
        })
        .reduce(Outcome::combine)
}

fn default_deny(action: Action) -> Outcome {
    Outcome::deny(
        RuleRef::new(RuleOrigin::Default, "", "default.deny"),
        format!("no rule permits {action} (deny by default)"),
    )
}

fn finish(outcome: Outcome, profile: Profile, intent: &Intent) -> Decision {
    let sandbox = outcome.sandbox.max(profile.baseline_sandbox());
    let verdict = match outcome.verdict {
        Verdict::Deny => Verdict::Deny,
        approval @ Verdict::RequireApproval { .. } => approval,
        _ if sandbox >= SandboxLevel::Restricted => Verdict::Sandbox { level: sandbox },
        _ => match outcome.rate_limit {
            Some(limit) => Verdict::RateLimit {
                max: limit.max,
                per: limit.per,
            },
            None => Verdict::Allow,
        },
    };
    Decision {
        verdict,
        reason: outcome.reason,
        rule: outcome.rule,
        effect_summary: summary::effect_summary(intent),
        sandbox,
        rate_limit: outcome.rate_limit,
    }
}
