//! Project rules: closed schema, tighten-only policy floors, confinement, and
//! instruction files.

mod common;

use aidex_context::{ContextError, ProjectRules};
use aidex_core::EntityId;
use aidex_policy::{
    Action, ActionPattern, EvaluationContext, HostTrust, Intent, Layer, NetworkMode, PathScope,
    PolicySet, PrivacyMode, Profile, Rule, ShellCommand, ShellPlatform, Verdict, classify_network,
    evaluate,
};
use common::*;

const RULES: &str = r#"
schema_version = 1

[project]
package_manager = "bun"
tests = "bun test"
lint = "bun lint"
build = "bun run build"

[agent]
never_modify = ["migrations/production"]
require_approval = ["git.push", "package.*"]
"#;

fn workspace() -> EntityId {
    EntityId::new("wsp_rules").unwrap()
}

fn intent(action: Action) -> Intent {
    Intent::new(action, EntityId::new("ses_rules").unwrap(), "test").with_workspace(workspace())
}

fn intents() -> Vec<Intent> {
    let write = |path: &str| {
        intent(Action::FsWrite).with_path(PathScope::inside(format!("/repo/{path}"), path))
    };
    vec![
        write("migrations/production/001.sql"),
        write("src/main.rs"),
        intent(Action::FsDelete).with_path(PathScope::inside(
            "/repo/migrations/production",
            "migrations/production",
        )),
        intent(Action::FsRead).with_path(PathScope::inside(
            "/repo/migrations/production/001.sql",
            "migrations/production/001.sql",
        )),
        intent(Action::GitPush)
            .with_network(classify_network("github.com", 443).unwrap())
            .with_remote_write(),
        intent(Action::ShellExec)
            .with_command(ShellCommand::classify("bun add zod", ShellPlatform::Unix)),
        intent(Action::ShellExec)
            .with_command(ShellCommand::classify("ls", ShellPlatform::Unix)),
        intent(Action::DiskFormat),
    ]
}

/// Allow everything in the workspace layer, so only floors can tighten.
fn permissive() -> PolicySet {
    let mut policies = PolicySet::new();
    policies
        .add_rule(
            Layer::Workspace,
            Some(workspace()),
            Rule::new("allow-all", ActionPattern::Any, Verdict::Allow),
        )
        .unwrap();
    policies
}

fn strictness(verdict: &Verdict) -> u8 {
    match verdict {
        Verdict::Allow => 0,
        Verdict::RateLimit { .. } => 1,
        Verdict::Sandbox { .. } => 2,
        Verdict::RequireApproval { .. } => 3,
        Verdict::Deny => 4,
    }
}

#[test]
fn repository_rules_only_tighten_policy() {
    let fixture = Fixture::new(&[(".aidex/rules.toml", RULES)]);
    let rules = ProjectRules::load(&fixture.confined()).unwrap();
    let commands = rules.project();
    assert_eq!(commands.package_manager.as_deref(), Some("bun"));
    assert_eq!(commands.build.as_deref(), Some("bun run build"));
    assert_eq!(rules.never_modify(), ["migrations/production"]);
    assert_eq!(rules.require_approval().len(), 2);

    let context = EvaluationContext::new(
        Profile::Autonomous,
        PrivacyMode::Hybrid,
        HostTrust::Trusted,
        NetworkMode::Unrestricted,
    );
    let without = permissive();
    let mut with = permissive();
    assert!(rules.apply_to(&mut with, &workspace()));
    for intent in intents() {
        let before = evaluate(&without, &context, &intent);
        let after = evaluate(&with, &context, &intent);
        assert!(
            strictness(&after.verdict) >= strictness(&before.verdict),
            "{:?} loosened {:?} -> {:?}",
            intent.action,
            before.verdict,
            after.verdict
        );
    }
    let decide = |intent: &Intent| evaluate(&with, &context, intent).verdict;
    let all = intents();
    assert_eq!(decide(&all[0]), Verdict::Deny);
    assert_eq!(decide(&all[1]), Verdict::Allow);
    assert_eq!(decide(&all[2]), Verdict::Deny);
    assert_eq!(decide(&all[3]), Verdict::Allow);
    assert!(decide(&all[4]).requires_approval());
    assert!(decide(&all[5]).requires_approval());
    assert_eq!(decide(&all[7]), Verdict::Deny);
    // The policy layer is the same data.
    assert_eq!(
        rules.to_policy_layer().unwrap().never_modify(),
        ["migrations/production"]
    );
}

#[test]
fn rules_files_cannot_grant_anything_or_escape_the_workspace() {
    for text in [
        "schema_version = 1\n[agent]\nallow = [\"*\"]\n",
        "schema_version = 1\n[agent]\nrequire_approval = [\"git.yeet\"]\n",
        "schema_version = 1\n[policy]\nprofile = \"autonomous\"\n",
        "schema_version = 2\n",
        "[agent]\nnever_modify = [\"src\"]\n",
    ] {
        let fixture = Fixture::new(&[(".aidex/rules.toml", text)]);
        let error = ProjectRules::load(&fixture.confined()).unwrap_err();
        assert!(matches!(error, ContextError::Rules(_)), "{text}: {error}");
        assert_eq!(error.reason(), "invalid_rules");
    }
    // A rules directory linked outside the workspace is refused, not followed.
    let fixture = Fixture::new(&[("src/lib.rs", "")]);
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "rules.toml", RULES);
    if dir_link(outside.path(), &fixture.root().join(".aidex")) {
        let error = ProjectRules::load(&fixture.confined()).unwrap_err();
        assert!(matches!(error, ContextError::Rules(_)), "{error}");
    }
    // No rules file: nothing attached, nothing fails.
    let empty = Fixture::new(&[("src/lib.rs", "")]);
    let rules = ProjectRules::load(&empty.confined()).unwrap();
    assert!(rules.is_empty());
    let mut policies = PolicySet::new();
    assert!(!rules.apply_to(&mut policies, &workspace()));
    assert!(rules.summary_text().is_none());
}

#[test]
fn instruction_files_are_surfaced_redacted_and_bounded() {
    let token = fake_token();
    let big = "line of guidance\n".repeat(5_000);
    let fixture = Fixture::new(&[
        ("AGENTS.md", &format!("Use bun.\nDeploy with {token}\n")),
        ("CLAUDE.md", &big),
        (".aidex/instructions.md", "Prefer small commits.\n"),
        ("docs/AGENTS.md", "nested files are not root instructions\n"),
    ]);
    let rules = ProjectRules::load(&fixture.confined()).unwrap();
    let paths: Vec<&str> = rules
        .instructions()
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(paths, ["AGENTS.md", "CLAUDE.md", ".aidex/instructions.md"]);
    let agents = &rules.instructions()[0];
    assert!(agents.redacted);
    assert!(!agents.text.contains(&token));
    let claude = &rules.instructions()[1];
    assert!(claude.truncated);
    assert!(claude.text.len() <= aidex_context::MAX_INSTRUCTION_BYTES);
    assert_eq!(claude.bytes, big.len() as u64);
    let view = rules.view();
    assert!(view.source.is_none());
    assert_eq!(view.instructions.len(), 3);
}
