//! Intent fingerprints bind approvals to one exact intent.

mod common;

use std::collections::BTreeSet;

use aidex_policy::{
    Action, Intent, ModelLocality, NetworkKind, PathClass, PathScope, SecretUse, ShellCommand,
    ShellPlatform, ShellRisk, classify_network,
};
use common::*;

fn base() -> Intent {
    intent(Action::ShellExec)
        .with_path(PathScope::inside("/w/src/main.rs", "src/main.rs"))
        .with_command(ShellCommand::classify(
            "git push origin main",
            ShellPlatform::Unix,
        ))
        .with_network(classify_network("github.com", 443).unwrap())
        .with_secret(SecretUse::new("GITHUB_TOKEN", "tool_git"))
        .with_model_locality(ModelLocality::Local)
}

#[test]
fn fingerprint_is_stable_for_equal_intents_and_after_serde_round_trip() {
    let intent = base();
    assert_eq!(intent.fingerprint(), base().fingerprint());
    assert_eq!(intent.fingerprint(), intent.clone().fingerprint());
    let json = serde_json::to_string(&intent).unwrap();
    let restored: Intent = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, intent);
    assert_eq!(restored.fingerprint(), intent.fingerprint());
    let hex = intent.fingerprint_hex();
    assert_eq!(hex.len(), 64);
    assert!(hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn fingerprint_changes_when_any_field_or_attribute_changes() {
    type Mutation = fn(&mut Intent);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("action", |intent| intent.action = Action::ProcessSpawn),
        ("session", |intent| intent.session_id = id("ses_other")),
        ("workspace", |intent| {
            intent.workspace_id = Some(id("ws_other"))
        }),
        ("no workspace", |intent| intent.workspace_id = None),
        ("agent", |intent| intent.agent_id = Some(id("agt_other"))),
        ("tool", |intent| intent.tool_id = None),
        ("skill", |intent| intent.skill_id = Some(id("skl_other"))),
        ("summary", |intent| intent.summary.push('!')),
        ("path", |intent| {
            intent.attributes.paths[0].path = "/w/src/lib.rs".into()
        }),
        ("relative", |intent| {
            intent.attributes.paths[0].workspace_relative = Some("src/lib.rs".into());
        }),
        ("outside", |intent| {
            intent.attributes.paths[0].workspace_relative = None
        }),
        ("class", |intent| {
            intent.attributes.paths[0].class = PathClass::Sensitive
        }),
        ("extra path", |intent| {
            intent
                .attributes
                .paths
                .push(PathScope::outside("/etc/hosts"));
        }),
        ("no paths", |intent| intent.attributes.paths.clear()),
        ("command", |intent| {
            intent.attributes.command = Some(ShellCommand::classify(
                "git push --force origin main",
                ShellPlatform::Unix,
            ));
        }),
        ("platform", |intent| {
            intent.attributes.command = Some(ShellCommand::classify(
                "git push origin main",
                ShellPlatform::Windows,
            ));
        }),
        ("no command", |intent| intent.attributes.command = None),
        ("host", |intent| {
            intent.attributes.network = Some(classify_network("gitlab.com", 443).unwrap());
        }),
        ("port", |intent| {
            intent.attributes.network = Some(classify_network("github.com", 22).unwrap());
        }),
        ("no network", |intent| intent.attributes.network = None),
        ("secret name", |intent| {
            intent.attributes.secret = Some(SecretUse::new("NPM_TOKEN", "tool_git"));
        }),
        ("secret consumer", |intent| {
            intent.attributes.secret = Some(SecretUse::new("GITHUB_TOKEN", "mcp_github"));
        }),
        ("no secret", |intent| intent.attributes.secret = None),
        ("locality", |intent| {
            intent.attributes.model_locality = Some(ModelLocality::Remote)
        }),
        ("no locality", |intent| {
            intent.attributes.model_locality = None
        }),
        ("remote write", |intent| {
            intent.attributes.remote_write = true
        }),
        ("destructive", |intent| intent.attributes.destructive = true),
    ];
    let mut seen = BTreeSet::from([base().fingerprint()]);
    for (name, mutate) in mutations {
        let mut altered = base();
        mutate(&mut altered);
        assert!(
            seen.insert(altered.fingerprint()),
            "mutation `{name}` did not change the fingerprint"
        );
    }
}

#[test]
fn deserialized_commands_and_targets_are_reclassified_not_trusted() {
    let json = serde_json::json!({
        "action": "shell.exec",
        "session_id": "ses_x",
        "summary": "clean",
        "attributes": {
            "command": {
                "line": "rm -rf /",
                "platform": "unix",
                "classification": { "risk": "low" }
            },
            "network": { "host": "Example.COM", "port": 443, "kind": "loopback" }
        }
    });
    let intent: Intent = serde_json::from_value(json).unwrap();
    let command = intent.attributes.command.as_ref().unwrap();
    assert_eq!(command.classification().risk, ShellRisk::Destructive);
    let target = intent.attributes.network.as_ref().unwrap();
    assert_eq!(target.kind(), NetworkKind::Domain);
    assert_eq!(target.host(), "example.com");

    let unknown_field = serde_json::json!({
        "action": "fs.read", "session_id": "ses_x", "summary": "", "risk": "low"
    });
    assert!(serde_json::from_value::<Intent>(unknown_field).is_err());
    let unknown_action = serde_json::json!({
        "action": "fs.teleport", "session_id": "ses_x", "summary": ""
    });
    assert!(serde_json::from_value::<Intent>(unknown_action).is_err());
}

#[test]
fn command_debug_output_redacts_credentials() {
    let command = ShellCommand::classify(
        "curl -H 'Authorization: Bearer sk-live-abcdefghijklmnopqrstuvwxyz' https://api.example.com",
        ShellPlatform::Unix,
    );
    let debug = format!("{command:?}");
    assert!(!debug.contains("abcdefghijklmnopqrstuvwxyz"), "{debug}");
    assert!(debug.contains("[REDACTED]"));
    assert!(command.line().contains("abcdefghijklmnopqrstuvwxyz"));
}
