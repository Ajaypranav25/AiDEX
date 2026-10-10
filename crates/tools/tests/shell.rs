//! `shell.exec`: classification facts, argument vectors, shell form, timeouts.

mod common;

use aidex_events::RiskLevel;
use aidex_policy::{Action, ShellRisk};
use common::{Fixture, git_available};
use serde_json::json;

#[test]
fn argument_vectors_are_classified_and_run_without_a_shell() {
    if !git_available() {
        return;
    }
    let fixture = Fixture::new();
    let (ctx, plan) = fixture
        .plan(
            "shell.exec",
            json!({"program": "git", "args": ["--version"]}),
        )
        .unwrap();
    let command = plan.facts().command.as_ref().unwrap();
    assert_eq!(command.classification().program, "git");
    assert_eq!(plan.intent().action, Action::ShellExec);
    assert!(plan.intent().attributes.command.is_some());
    let output = fixture.execute(&ctx, &plan).unwrap();
    assert_eq!(output.content["exit_code"], 0);
    assert!(
        output.content["stdout"]
            .as_str()
            .unwrap()
            .starts_with("git version")
    );
    assert_eq!(output.content["sandbox_level"], "direct");
    // Metacharacters in arguments are literal text, never shell syntax.
    let (_, plan) = fixture
        .plan(
            "shell.exec",
            json!({"program": "git", "args": ["log", "; rm -rf /"]}),
        )
        .unwrap();
    let classification = plan.facts().command.as_ref().unwrap().classification();
    assert!(
        classification.risk < ShellRisk::Destructive,
        "{classification:?}"
    );
    // A non-zero exit is a result, not a tool failure.
    let output = fixture
        .run(
            "shell.exec",
            json!({"program": "git", "args": ["no-such-subcommand"]}),
        )
        .unwrap();
    assert_ne!(output.content["exit_code"], 0);
}

#[test]
fn shell_form_is_explicit_and_higher_risk() {
    let fixture = Fixture::new();
    let (ctx, plan) = fixture
        .plan(
            "shell.exec",
            json!({"command": "echo hello", "shell": true}),
        )
        .unwrap();
    assert!(plan.risk() >= RiskLevel::High);
    let output = fixture.execute(&ctx, &plan).unwrap();
    assert_eq!(output.content["exit_code"], 0, "{:?}", output.content);
    assert!(output.content["stdout"].as_str().unwrap().contains("hello"));
    let (_, destructive) = fixture
        .plan("shell.exec", json!({"command": "rm -rf /", "shell": true}))
        .unwrap();
    assert!(destructive.facts().destructive);
    assert_eq!(destructive.risk(), RiskLevel::Critical);
    let (_, push) = fixture
        .plan(
            "shell.exec",
            json!({"command": "git push origin main", "shell": true}),
        )
        .unwrap();
    assert!(push.facts().remote_write);
    assert!(push.facts().network.is_some());
}

#[test]
fn processes_are_stopped_at_their_timeout_and_output_is_redacted() {
    let fixture = Fixture::new();
    let sleep = if cfg!(windows) {
        "Start-Sleep -Seconds 20"
    } else {
        "sleep 20"
    };
    let (ctx, plan) = fixture
        .plan(
            "shell.exec",
            json!({"command": sleep, "shell": true, "timeout_ms": 300}),
        )
        .unwrap();
    let output = fixture.execute(&ctx, &plan).unwrap();
    assert_eq!(output.content["timed_out"], true);
    assert!(output.content["duration_ms"].as_u64().unwrap() < 15_000);
    let token = format!("ghp_{}", "b".repeat(36));
    let output = fixture
        .run(
            "shell.exec",
            json!({"command": format!("echo {token}"), "shell": true}),
        )
        .unwrap();
    let stdout = output.content["stdout"].as_str().unwrap();
    assert!(!stdout.contains(&token), "{stdout}");
    let events = serde_json::to_string(&fixture.events.events()).unwrap();
    assert!(!events.contains(&token));
}

#[test]
fn input_rules_are_enforced() {
    let fixture = Fixture::new();
    for (input, reason) in [
        (json!({"shell": true}), "invalid_input"),
        (json!({"program": "git", "command": "x"}), "invalid_input"),
        (
            json!({"shell": true, "command": "x", "program": "git"}),
            "invalid_input",
        ),
        (json!({"program": "git", "timeout_ms": 0}), "invalid_input"),
        (json!({"program": "git", "cwd": "missing"}), "not_found"),
    ] {
        assert_eq!(
            fixture
                .plan("shell.exec", input.clone())
                .unwrap_err()
                .reason(),
            reason,
            "{input}"
        );
    }
}
