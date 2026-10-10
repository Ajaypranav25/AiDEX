//! Shared builders for policy integration tests.
#![allow(dead_code)]

use aidex_core::EntityId;
use aidex_policy::{
    Action, ActionPattern, ApprovalScope, Decision, EvaluationContext, HostTrust, Intent,
    ModelLocality, NetworkMode, PathScope, PolicySet, PrivacyMode, Profile, Rule, SecretUse,
    ShellCommand, ShellPlatform, Verdict, classify_network, evaluate,
};

pub fn id(value: &str) -> EntityId {
    EntityId::new(value).expect("valid test id")
}

pub fn session() -> EntityId {
    id("ses_test")
}

pub fn workspace() -> EntityId {
    id("ws_test")
}

pub fn agent() -> EntityId {
    id("agt_test")
}

pub fn skill() -> EntityId {
    id("skl_test")
}

pub fn tool() -> EntityId {
    id("tool_test")
}

pub fn host() -> EntityId {
    id("host_test")
}

/// Trusted host, hybrid privacy, unrestricted network: no floors beyond hard rules.
pub fn open_context(profile: Profile) -> EvaluationContext {
    EvaluationContext::new(
        profile,
        PrivacyMode::Hybrid,
        HostTrust::Trusted,
        NetworkMode::Unrestricted,
    )
    .with_host(host())
}

/// Trusted host, hybrid privacy, normal network mode.
pub fn context(profile: Profile) -> EvaluationContext {
    EvaluationContext::new(
        profile,
        PrivacyMode::Hybrid,
        HostTrust::Trusted,
        NetworkMode::Normal,
    )
    .with_host(host())
}

/// An intent carrying every subject id, so every layer can apply.
pub fn intent(action: Action) -> Intent {
    Intent::new(action, session(), "test intent")
        .with_workspace(workspace())
        .with_agent(agent())
        .with_skill(skill())
        .with_tool(tool())
}

pub fn write_inside(relative: &str) -> Intent {
    intent(Action::FsWrite).with_path(PathScope::inside(
        format!("/work/repo/{relative}"),
        relative,
    ))
}

pub fn write_outside(path: &str) -> Intent {
    intent(Action::FsWrite).with_path(PathScope::outside(path))
}

pub fn shell(line: &str) -> Intent {
    intent(Action::ShellExec).with_command(ShellCommand::classify(line, ShellPlatform::Unix))
}

pub fn request(host: &str, port: u16) -> Intent {
    intent(Action::NetworkRequest).with_network(classify_network(host, port).expect("valid host"))
}

pub fn push_to(host: &str) -> Intent {
    intent(Action::GitPush)
        .with_network(classify_network(host, 443).expect("valid host"))
        .with_remote_write()
}

pub fn model(locality: ModelLocality) -> Intent {
    intent(Action::ModelInvoke).with_model_locality(locality)
}

pub fn secret() -> Intent {
    intent(Action::SecretUse).with_secret(SecretUse::new("GITHUB_TOKEN", "tool_git_push"))
}

pub fn decide(policies: &PolicySet, context: &EvaluationContext, intent: &Intent) -> Decision {
    evaluate(policies, context, intent)
}

pub fn allow(rule_id: &str, action: impl Into<ActionPattern>) -> Rule {
    Rule::new(rule_id, action, Verdict::Allow)
}

/// A policy set with an allow-everything rule in every layer.
pub fn allow_everything() -> PolicySet {
    let mut set = PolicySet::new();
    let subjects = [
        (aidex_policy::Layer::Global, None),
        (aidex_policy::Layer::Machine, Some(host())),
        (aidex_policy::Layer::Workspace, Some(workspace())),
        (aidex_policy::Layer::Agent, Some(agent())),
        (aidex_policy::Layer::Skill, Some(skill())),
        (aidex_policy::Layer::Tool, Some(tool())),
        (aidex_policy::Layer::Session, Some(session())),
    ];
    for (layer, subject) in subjects {
        set.add_rule(layer, subject, allow("allow-all", ActionPattern::Any))
            .expect("valid rule");
    }
    set
}

pub fn once() -> Verdict {
    Verdict::approval(&[ApprovalScope::Once])
}

pub fn once_or_session() -> Verdict {
    Verdict::approval(&[ApprovalScope::Once, ApprovalScope::Session])
}
