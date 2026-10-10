//! Precedence: deny-by-default, deny wins, layer specificity, profile defaults, floors.

mod common;

use aidex_policy::{
    Action, ActionPattern, Conditions, Layer, ModelLocality, PathGlobs, PathScope, PolicySet,
    Profile, Rule, RuleOrigin, SandboxLevel, Verdict,
};
use common::*;

fn subject(layer: Layer) -> Option<aidex_core::EntityId> {
    match layer {
        Layer::Global => None,
        Layer::Machine => Some(host()),
        Layer::Workspace => Some(workspace()),
        Layer::Agent => Some(agent()),
        Layer::Skill => Some(skill()),
        Layer::Tool => Some(tool()),
        Layer::Session => Some(session()),
    }
}

fn with_rules(rules: &[(Layer, Rule)]) -> PolicySet {
    let mut set = PolicySet::new();
    for (layer, rule) in rules {
        set.add_rule(*layer, subject(*layer), rule.clone()).unwrap();
    }
    set
}

#[test]
fn actions_without_any_matching_rule_are_denied_by_default() {
    let empty = PolicySet::new();
    // The Safe and Offline Secure profiles have no rule for host mounts or remote actions.
    for profile in [Profile::Safe, Profile::OfflineSecure] {
        for action in [Action::HostMount, Action::RemoteAction] {
            let decision = decide(&empty, &open_context(profile), &intent(action));
            assert_eq!(decision.verdict, Verdict::Deny, "{action} in {profile}");
            assert_eq!(decision.rule.origin, RuleOrigin::Default);
            assert_eq!(decision.rule.id, "default.deny");
        }
    }
    // Offline Secure has no rule for public network requests.
    let decision = decide(
        &empty,
        &open_context(Profile::OfflineSecure),
        &request("example.com", 443),
    );
    assert_eq!(decision.verdict, Verdict::Deny);
}

#[test]
fn incomplete_intents_are_denied_in_every_profile() {
    let policies = allow_everything();
    let incomplete = [
        intent(Action::FsWrite),
        intent(Action::ShellExec),
        intent(Action::ProcessSpawn),
        intent(Action::NetworkRequest),
        intent(Action::ModelInvoke),
        intent(Action::SecretUse),
        intent(Action::FsWrite).with_path(PathScope::inside("/w/../etc", "../etc")),
        intent(Action::FsRead).with_path(PathScope::inside("C:/x", "C:/x")),
    ];
    for profile in Profile::ALL {
        for intent in &incomplete {
            let decision = decide(&policies, &open_context(*profile), intent);
            assert_eq!(
                decision.verdict,
                Verdict::Deny,
                "{} in {profile}",
                intent.action
            );
            assert_eq!(decision.rule.id, "default.incomplete-intent");
        }
    }
}

#[test]
fn a_deny_in_any_layer_beats_allows_in_more_specific_layers() {
    for deny_layer in Layer::ALL {
        let mut rules: Vec<(Layer, Rule)> = Layer::ALL
            .iter()
            .filter(|layer| *layer != deny_layer)
            .map(|layer| (*layer, allow("allow-push", Action::GitPush)))
            .collect();
        rules.push((
            *deny_layer,
            Rule::new("deny-push", Action::GitPush, Verdict::Deny),
        ));
        let decision = decide(
            &with_rules(&rules),
            &open_context(Profile::Autonomous),
            &push_to("github.com"),
        );
        assert_eq!(decision.verdict, Verdict::Deny, "deny in {deny_layer}");
        assert_eq!(decision.rule.origin, RuleOrigin::Layer);
        assert_eq!(decision.rule.scope, deny_layer.as_str());
    }
}

#[test]
fn the_most_specific_matching_layer_wins_in_both_directions() {
    let layers = Layer::ALL;
    for (index, general) in layers.iter().enumerate() {
        for specific in &layers[index + 1..] {
            let loosen = with_rules(&[
                (*general, Rule::new("ask", Action::GitCommit, once())),
                (*specific, allow("allow", Action::GitCommit)),
            ]);
            let decision = decide(
                &loosen,
                &open_context(Profile::Safe),
                &intent(Action::GitCommit),
            );
            assert!(
                decision.verdict.permits_execution(),
                "{specific} over {general}"
            );
            assert_eq!(decision.rule.scope, specific.as_str());

            let tighten = with_rules(&[
                (*general, allow("allow", Action::GitCommit)),
                (*specific, Rule::new("ask", Action::GitCommit, once())),
            ]);
            let decision = decide(
                &tighten,
                &open_context(Profile::Autonomous),
                &intent(Action::GitCommit),
            );
            assert_eq!(decision.verdict, once(), "{specific} over {general}");
            assert_eq!(decision.rule.scope, specific.as_str());
        }
    }
}

#[test]
fn the_strictest_matching_rule_wins_within_one_layer() {
    let policies = with_rules(&[
        (Layer::Global, allow("allow", Action::GitPull)),
        (
            Layer::Global,
            Rule::new(
                "ephemeral",
                Action::GitPull,
                Verdict::sandbox(SandboxLevel::Ephemeral),
            ),
        ),
        (
            Layer::Global,
            Rule::new(
                "restricted",
                Action::GitPull,
                Verdict::sandbox(SandboxLevel::Restricted),
            ),
        ),
    ]);
    let decision = decide(
        &policies,
        &open_context(Profile::Balanced),
        &intent(Action::GitPull),
    );
    assert_eq!(decision.verdict, Verdict::sandbox(SandboxLevel::Ephemeral));
    assert_eq!(decision.rule.id, "ephemeral");
}

#[test]
fn profile_defaults_apply_only_when_no_layer_matches() {
    let empty = PolicySet::new();
    let decision = decide(&empty, &open_context(Profile::Balanced), &secret());
    assert_eq!(decision.verdict, once_or_session());
    assert_eq!(decision.rule.origin, RuleOrigin::Profile);
    assert_eq!(decision.rule.scope, "balanced");

    // A session rule may loosen the profile default on a trusted host...
    let loosened = with_rules(&[(Layer::Session, allow("trusted-token", Action::SecretUse))]);
    let decision = decide(&loosened, &open_context(Profile::Balanced), &secret());
    assert_eq!(decision.verdict, Verdict::Allow);
    assert_eq!(decision.rule.origin, RuleOrigin::Layer);

    // ...and a workspace rule may tighten a profile default.
    let tightened = with_rules(&[(
        Layer::Workspace,
        Rule::new("review-commits", Action::GitCommit, once()),
    )]);
    let decision = decide(
        &tightened,
        &open_context(Profile::Autonomous),
        &intent(Action::GitCommit),
    );
    assert_eq!(decision.verdict, once());
}

#[test]
fn layer_rules_apply_only_to_their_own_subject() {
    let mut policies = PolicySet::new();
    policies
        .add_rule(
            Layer::Workspace,
            Some(id("ws_other")),
            Rule::new("deny", Action::GitCommit, Verdict::Deny),
        )
        .unwrap();
    policies
        .add_rule(
            Layer::Machine,
            Some(id("host_other")),
            Rule::new("deny", Action::GitCommit, Verdict::Deny),
        )
        .unwrap();
    let decision = decide(
        &policies,
        &open_context(Profile::Balanced),
        &intent(Action::GitCommit),
    );
    assert_eq!(decision.verdict, Verdict::Allow);

    let without_host = aidex_policy::EvaluationContext {
        host_id: None,
        ..open_context(Profile::Balanced)
    };
    let mut machine = PolicySet::new();
    machine
        .add_rule(
            Layer::Machine,
            Some(host()),
            Rule::new("deny", Action::GitCommit, Verdict::Deny),
        )
        .unwrap();
    assert_eq!(
        decide(&machine, &without_host, &intent(Action::GitCommit)).verdict,
        Verdict::Allow
    );
    assert_eq!(
        decide(
            &machine,
            &open_context(Profile::Balanced),
            &intent(Action::GitCommit)
        )
        .verdict,
        Verdict::Deny
    );
}

#[test]
fn rules_without_a_subject_are_rejected_except_for_global() {
    let mut policies = PolicySet::new();
    assert!(
        policies
            .add_rule(Layer::Session, None, allow("a", Action::FsRead))
            .is_err()
    );
    assert!(
        policies
            .add_rule(Layer::Global, Some(session()), allow("a", Action::FsRead))
            .is_err()
    );
    assert!(
        policies
            .add_rule(Layer::Global, None, allow("Bad Id", Action::FsRead))
            .is_err()
    );
    policies
        .add_rule(Layer::Global, None, allow("a", Action::FsRead))
        .unwrap();
    let duplicate = policies.add_rule(Layer::Global, None, allow("a", Action::FsWrite));
    assert!(matches!(
        duplicate,
        Err(aidex_policy::PolicyError::DuplicateRule { .. })
    ));
}

#[test]
fn allowing_shell_does_not_bypass_actions_the_command_implies() {
    let policies = with_rules(&[(Layer::Session, allow("any-shell", Action::ShellExec))]);
    let decision = decide(
        &policies,
        &open_context(Profile::Balanced),
        &shell("npm install left-pad"),
    );
    assert_eq!(decision.verdict, once_or_session());
    assert_eq!(decision.rule.id, "package.install");

    let decision = decide(
        &policies,
        &open_context(Profile::Balanced),
        &shell("git push origin main"),
    );
    assert!(decision.verdict.requires_approval());
    assert_eq!(decision.rule.id, "git.push");
}

#[test]
fn program_conditions_distinguish_every_program_from_any_program() {
    let only_cargo = Conditions {
        only_programs: vec!["cargo".into()],
        ..Conditions::default()
    };
    let any_curl = Conditions {
        any_programs: vec!["curl".into()],
        ..Conditions::default()
    };
    let policies = with_rules(&[
        (
            Layer::Session,
            allow("cargo-only", Action::ShellExec).when(only_cargo),
        ),
        (
            Layer::Global,
            Rule::new("no-curl", Action::ShellExec, Verdict::Deny).when(any_curl),
        ),
    ]);
    let context = open_context(Profile::Safe);
    assert!(
        decide(&policies, &context, &shell("cargo test"))
            .verdict
            .permits_execution()
    );
    // A second program means the cargo-only allow does not match; Safe asks for Medium risk.
    assert!(
        decide(&policies, &context, &shell("cargo test && python tool.py"))
            .verdict
            .requires_approval()
    );
    assert_eq!(
        decide(
            &policies,
            &context,
            &shell("ls && curl https://example.com")
        )
        .verdict,
        Verdict::Deny
    );
}

#[test]
fn path_globs_match_workspace_relative_paths_only() {
    let secrets = Conditions {
        paths: Some(PathGlobs::new(["config/secrets", "**/*.pem"]).unwrap()),
        ..Conditions::default()
    };
    let policies = with_rules(&[(
        Layer::Workspace,
        Rule::new("no-secrets", Action::FsWrite, Verdict::Deny).when(secrets),
    )]);
    let context = open_context(Profile::Balanced);
    assert_eq!(
        decide(
            &policies,
            &context,
            &write_inside("config/secrets/prod.json")
        )
        .verdict,
        Verdict::Deny
    );
    assert_eq!(
        decide(&policies, &context, &write_inside("Config/Secrets")).verdict,
        Verdict::Deny
    );
    assert_eq!(
        decide(&policies, &context, &write_inside("certs/dev/server.pem")).verdict,
        Verdict::Deny
    );
    assert_eq!(
        decide(&policies, &context, &write_inside("src/main.rs")).verdict,
        Verdict::Allow
    );
    assert_eq!(
        decide(&policies, &context, &write_inside("config/secrets.md")).verdict,
        Verdict::Allow
    );
}

#[test]
fn multi_path_intents_take_the_strictest_path() {
    let intent = intent(Action::FsWrite)
        .with_path(PathScope::inside("/w/src/a.rs", "src/a.rs"))
        .with_path(PathScope::outside("/etc/hosts"));
    let decision = decide(&PolicySet::new(), &open_context(Profile::Balanced), &intent);
    assert_eq!(decision.verdict, once());
    assert_eq!(decision.rule.id, "fs.write.outside");
}

#[test]
fn approvals_combine_to_the_narrowest_scope_and_shortest_expiry() {
    let short = Verdict::RequireApproval {
        scope_options: vec![
            aidex_policy::ApprovalScope::Once,
            aidex_policy::ApprovalScope::Session,
        ],
        expires_in: std::time::Duration::from_secs(30),
    };
    let policies = with_rules(&[
        (Layer::Global, Rule::new("short", Action::GitPull, short)),
        (Layer::Global, Rule::new("once", Action::GitPull, once())),
    ]);
    let decision = decide(
        &policies,
        &open_context(Profile::Balanced),
        &intent(Action::GitPull),
    );
    assert_eq!(
        decision.verdict,
        Verdict::RequireApproval {
            scope_options: vec![aidex_policy::ApprovalScope::Once],
            expires_in: std::time::Duration::from_secs(30),
        }
    );
}

#[test]
fn rate_limits_survive_approval_and_sandbox_requirements_survive_rate_limits() {
    let limit = Verdict::RateLimit {
        max: 5,
        per: std::time::Duration::from_secs(60),
    };
    let policies = with_rules(&[(
        Layer::Global,
        Rule::new("throttle", ActionPattern::Any, limit),
    )]);
    let decision = decide(
        &policies,
        &open_context(Profile::Balanced),
        &model(ModelLocality::Local),
    );
    assert!(matches!(
        decision.verdict,
        Verdict::RateLimit { max: 5, .. }
    ));

    let decision = decide(
        &policies,
        &open_context(Profile::Safe),
        &model(ModelLocality::Local),
    );
    assert_eq!(decision.verdict, Verdict::sandbox(SandboxLevel::Restricted));
    assert_eq!(decision.rate_limit.map(|limit| limit.max), Some(5));

    let decision = decide(
        &policies,
        &open_context(Profile::Balanced),
        &intent(Action::PolicyChange),
    );
    assert_eq!(decision.verdict, once());
    assert_eq!(decision.rate_limit.map(|limit| limit.max), Some(5));
}

#[test]
fn evaluation_is_deterministic_and_reports_an_effect_summary() {
    let policies = allow_everything();
    let context = context(Profile::Balanced);
    let intent = shell("git push origin main");
    let first = decide(&policies, &context, &intent);
    for _ in 0..10 {
        assert_eq!(decide(&policies, &context, &intent), first);
    }
    assert!(first.effect_summary.starts_with("Run a shell command"));
    assert!(first.effect_summary.contains("git.push"));
    assert!(!first.reason.is_empty());
}

#[test]
fn decisions_serialize_with_a_stable_tagged_shape() {
    let decision = decide(
        &PolicySet::new(),
        &open_context(Profile::Safe),
        &write_outside("/etc/hosts"),
    );
    let json = serde_json::to_value(&decision).unwrap();
    assert_eq!(json["verdict"]["kind"], "require_approval");
    assert_eq!(
        json["verdict"]["scope_options"],
        serde_json::json!(["once"])
    );
    assert_eq!(json["verdict"]["expires_in_secs"], 600);
    assert_eq!(json["rule"]["origin"], "profile");
    assert_eq!(json["rule"]["scope"], "safe");
    assert_eq!(json["sandbox"], "restricted");
    let restored: aidex_policy::Decision = serde_json::from_value(json).unwrap();
    assert_eq!(restored, decision);
}
