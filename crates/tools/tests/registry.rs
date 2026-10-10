//! Registry contract: descriptors and risk metadata, schema validation, two-phase
//! permit checks, events, and policy intents.

mod common;

use aidex_events::{EventKind, RiskLevel};
use aidex_policy::{Action, PathClass};
use aidex_tools::{
    BUILTIN_TOOL_IDS, ExecutionPermit, InputError, PermitIssuer, PlanFingerprint, RiskProfile,
    Tool, ToolContext, ToolDescriptor, ToolError, ToolId, ToolOutput, ToolPlan, ToolRegistry,
};
use common::Fixture;
use serde_json::{Value, json};

#[test]
fn registry_exposes_every_builtin_with_risk_metadata() {
    let registry = ToolRegistry::with_builtin_tools();
    let ids: Vec<&str> = registry
        .descriptors()
        .iter()
        .map(|descriptor| descriptor.id.as_str())
        .collect();
    assert_eq!(ids, BUILTIN_TOOL_IDS);
    let risk = |id: &str| {
        registry
            .descriptor(&ToolId::new(id).unwrap())
            .unwrap()
            .risk
            .clone()
    };
    assert_eq!(risk("fs.read").action, Action::FsRead);
    assert!(risk("fs.read").is_read_only());
    assert!(risk("git.status").is_read_only() && risk("git.status").spawns_process);
    assert!(risk("fs.write").writes_workspace && !risk("fs.write").is_read_only());
    assert!(risk("fs.delete").destructive);
    let push = risk("git.push");
    assert_eq!(push.action, Action::GitPush);
    assert!(push.remote_write && push.network);
    assert_eq!(push.base_risk, RiskLevel::High);
    assert_eq!(risk("git.commit").action, Action::GitCommit);
    assert_eq!(risk("shell.exec").action, Action::ShellExec);
    for descriptor in registry.descriptors() {
        assert_eq!(
            descriptor.input_schema["type"], "object",
            "{}",
            descriptor.id
        );
        assert_eq!(
            descriptor.input_schema["additionalProperties"],
            Value::Bool(false),
            "{} input schema must be closed",
            descriptor.id
        );
        assert_eq!(descriptor.output_schema["type"], "object");
        assert!(!descriptor.capabilities.is_empty());
        assert!(descriptor.input_schema.get("$schema").is_none());
    }
}

#[test]
fn model_specs_use_provider_safe_names() {
    let registry = ToolRegistry::with_builtin_tools();
    let specs = registry.specs();
    assert_eq!(specs.len(), BUILTIN_TOOL_IDS.len());
    for spec in &specs {
        assert!(
            spec.name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'),
            "{}",
            spec.name
        );
        assert_eq!(
            registry
                .resolve_model_name(&spec.name)
                .unwrap()
                .model_name(),
            spec.name
        );
    }
    let read = specs.iter().find(|spec| spec.name == "fs_read").unwrap();
    assert_eq!(read.parameters["required"], json!(["path"]));
}

struct Dummy(ToolDescriptor);

impl Tool for Dummy {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.0
    }

    fn plan(&self, _ctx: &ToolContext, _input: Value) -> Result<ToolPlan, ToolError> {
        Err(ToolError::Cancelled)
    }

    fn execute(
        &self,
        _ctx: &ToolContext,
        _plan: &ToolPlan,
        _permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Cancelled)
    }
}

fn dummy(id: &str, input_schema: Value) -> Box<dyn Tool> {
    Box::new(Dummy(ToolDescriptor {
        id: ToolId::new(id).unwrap(),
        version: 1,
        description: "test".into(),
        input_schema,
        output_schema: json!({"type": "object"}),
        risk: RiskProfile::read_only(Action::FsRead),
        capabilities: vec!["test".into()],
    }))
}

#[test]
fn tool_ids_are_validated_and_duplicates_rejected() {
    for bad in ["fs", "FS.read", "fs..read", "fs.read!", ".fs", "fs.1read"] {
        assert!(ToolId::new(bad).is_err(), "{bad}");
    }
    let object = json!({"type": "object"});
    let mut registry = ToolRegistry::with_builtin_tools();
    let error = registry
        .register(dummy("fs.read", object.clone()))
        .unwrap_err();
    assert_eq!(error.reason(), "registration");
    // `fs.read` and `fs_read` would share the model name `fs_read`.
    let mut fresh = ToolRegistry::new();
    fresh.register(dummy("ext.a_b", object.clone())).unwrap();
    assert!(fresh.register(dummy("ext_a.b", object.clone())).is_err());
    assert!(fresh.register(dummy("ext.c", json!(true))).is_err());
    let error = registry
        .plan(
            &Fixture::new().ctx(),
            &ToolId::new("fs.unknown").unwrap(),
            json!({}),
        )
        .unwrap_err();
    assert_eq!(error.reason(), "unknown_tool");
}

fn invalid(error: ToolError) -> (String, InputError) {
    match error {
        ToolError::InvalidInput { field, reason, .. } => (field, reason),
        other => panic!("expected invalid input, got {other:?}"),
    }
}

#[test]
fn schema_validation_errors_are_stable() {
    let fixture = Fixture::new();
    fixture.write("a.txt", "a\n");
    let cases: Vec<(&str, Value, &str, InputError)> = vec![
        (
            "fs.read",
            json!({"path": "a.txt", "bogus": 1}),
            "bogus",
            InputError::UnknownField,
        ),
        ("fs.read", json!({}), "path", InputError::MissingField),
        (
            "fs.read",
            json!({"path": 5}),
            "path",
            InputError::InvalidType,
        ),
        ("fs.read", json!("a.txt"), "", InputError::NotAnObject),
        (
            "fs.read",
            json!({"path": "a.txt", "start_line": 0}),
            "start_line",
            InputError::OutOfRange,
        ),
        (
            "fs.write",
            json!({"path": "b.txt"}),
            "content",
            InputError::MissingField,
        ),
        (
            "fs.write",
            json!({"path": "b.txt", "content": ["x"]}),
            "content",
            InputError::InvalidType,
        ),
        (
            "fs.patch",
            json!({"path": "a.txt", "edits": [{"old": "a", "new": "b", "extra": true}]}),
            "edits[0].extra",
            InputError::UnknownField,
        ),
        (
            "fs.patch",
            json!({"path": "a.txt", "edits": [{"new": "b"}]}),
            "edits[0].old",
            InputError::MissingField,
        ),
        (
            "fs.patch",
            json!({"path": "a.txt"}),
            "edits",
            InputError::MissingField,
        ),
        (
            "fs.patch",
            json!({"path": "a.txt", "edits": [{"old": "a", "new": "b"}], "unified_diff": "@@"}),
            "unified_diff",
            InputError::ConflictingFields,
        ),
        (
            "fs.search",
            json!({"pattern": "(", "regex": true}),
            "pattern",
            InputError::InvalidPattern,
        ),
        (
            "fs.glob",
            json!({"pattern": "../*"}),
            "pattern",
            InputError::InvalidPattern,
        ),
        (
            "fs.read",
            json!({"path": "../a.txt"}),
            "path",
            InputError::InvalidPath,
        ),
        (
            "fs.read",
            json!({"path": "/etc/passwd"}),
            "path",
            InputError::InvalidPath,
        ),
        (
            "git.diff",
            json!({"mode": "sideways"}),
            "mode",
            InputError::InvalidValue,
        ),
        (
            "shell.exec",
            json!({"command": "ls"}),
            "command",
            InputError::ConflictingFields,
        ),
        ("shell.exec", json!({}), "program", InputError::MissingField),
    ];
    for (tool, input, field, reason) in cases {
        let error = fixture.plan(tool, input.clone()).unwrap_err();
        assert_eq!(error.reason(), "invalid_input", "{tool} {input}");
        assert_eq!(invalid(error), (field.to_owned(), reason), "{tool} {input}");
    }
}

#[test]
fn output_conforms_to_the_typed_contract() {
    let fixture = Fixture::new();
    fixture.write("a.txt", "one\ntwo\n");
    let output = fixture.run("fs.read", json!({"path": "a.txt"})).unwrap();
    let object = output.content.as_object().unwrap();
    for field in ["path", "size", "sha256", "binary", "content", "truncated"] {
        assert!(object.contains_key(field), "{field}");
    }
    let properties = fixture
        .registry
        .descriptor(&ToolId::new("fs.read").unwrap())
        .unwrap()
        .output_schema["properties"]
        .as_object()
        .unwrap()
        .clone();
    for key in object.keys() {
        assert!(
            properties.contains_key(key),
            "{key} missing from the output schema"
        );
    }
}

#[test]
fn execute_refuses_a_permit_for_another_plan() {
    let fixture = Fixture::new();
    let (ctx, plan) = fixture
        .plan("fs.write", json!({"path": "a.txt", "content": "x"}))
        .unwrap();
    let wrong = ExecutionPermit::new(
        PlanFingerprint::from_bytes([7; 32]),
        PermitIssuer::Development,
    );
    let error = fixture.registry.execute(&ctx, &plan, wrong).unwrap_err();
    assert_eq!(error.reason(), "plan_mismatch");
    // A permit for a second plan of the same input is still a different plan.
    let other = fixture
        .plan_in(&ctx, "fs.write", json!({"path": "a.txt", "content": "x"}))
        .unwrap();
    assert_ne!(other.fingerprint(), plan.fingerprint());
    let error = fixture
        .registry
        .execute(
            &ctx,
            &plan,
            ExecutionPermit::for_plan(&other, PermitIssuer::Development),
        )
        .unwrap_err();
    assert_eq!(error.reason(), "plan_mismatch");
    assert!(
        !fixture.path().join("a.txt").exists(),
        "nothing was written"
    );
}

#[test]
fn execute_refuses_a_plan_used_in_another_context() {
    let fixture = Fixture::new();
    let (_, plan) = fixture
        .plan("fs.write", json!({"path": "a.txt", "content": "x"}))
        .unwrap();
    let other_call = fixture.ctx();
    let error = fixture.execute(&other_call, &plan).unwrap_err();
    assert!(matches!(error, ToolError::PlanMismatch(_)), "{error}");
}

#[test]
fn read_only_permits_only_authorize_read_only_tools() {
    let fixture = Fixture::new();
    fixture.write("a.txt", "a");
    let (ctx, read) = fixture.plan("fs.read", json!({"path": "a.txt"})).unwrap();
    fixture
        .registry
        .execute(
            &ctx,
            &read,
            ExecutionPermit::for_plan(&read, PermitIssuer::ReadOnly),
        )
        .unwrap();
    let (ctx, write) = fixture
        .plan("fs.write", json!({"path": "a.txt", "content": "b"}))
        .unwrap();
    let error = fixture
        .registry
        .execute(
            &ctx,
            &write,
            ExecutionPermit::for_plan(&write, PermitIssuer::ReadOnly),
        )
        .unwrap_err();
    assert_eq!(error.reason(), "permit_rejected");
    assert_eq!(fixture.read("a.txt"), "a");
}

#[test]
fn plans_execute_at_most_once() {
    let fixture = Fixture::new();
    let (ctx, plan) = fixture
        .plan("fs.write", json!({"path": "a.txt", "content": "x"}))
        .unwrap();
    fixture.execute(&ctx, &plan).unwrap();
    let error = fixture.execute(&ctx, &plan).unwrap_err();
    assert!(matches!(error, ToolError::AlreadyExecuted));
}

#[test]
fn plan_intents_carry_policy_facts() {
    let fixture = Fixture::new();
    fixture.write(".env", "TOKEN=1");
    let (_, plan) = fixture
        .plan("fs.write", json!({"path": ".env", "content": "TOKEN=2"}))
        .unwrap();
    let intent = plan.intent();
    assert_eq!(intent.action, Action::FsWrite);
    assert_eq!(intent.tool_id.as_ref().unwrap().as_str(), "fs.write");
    assert_eq!(intent.workspace_id.as_ref(), Some(&fixture.workspace));
    let path = &intent.attributes.paths[0];
    assert_eq!(path.workspace_relative.as_deref(), Some(".env"));
    assert_eq!(path.class, PathClass::Sensitive);
    // Writes beneath `.aidex/` also need a `policy.change` decision.
    let (_, rules) = fixture
        .plan(
            "fs.write",
            json!({"path": ".aidex/rules.toml", "content": "x"}),
        )
        .unwrap();
    let actions: Vec<Action> = rules.intents().iter().map(|intent| intent.action).collect();
    assert_eq!(actions, [Action::FsWrite, Action::PolicyChange]);
    // Evaluates without missing facts under the balanced profile.
    let decision = aidex_policy::evaluate(
        &aidex_policy::PolicySet::default(),
        &aidex_policy::EvaluationContext::new(
            aidex_policy::Profile::Balanced,
            aidex_policy::PrivacyMode::Hybrid,
            aidex_policy::HostTrust::Trusted,
            aidex_policy::NetworkMode::Normal,
        ),
        &plan.intent(),
    );
    assert_ne!(decision.rule.id, "default.incomplete-intent");
}

#[test]
fn lifecycle_events_are_emitted_and_redacted() {
    let fixture = Fixture::new();
    let token = format!("ghp_{}", "a".repeat(36));
    let name = format!("notes-{token}.txt");
    fixture.write(&name, "hello\n");
    fixture.run("fs.read", json!({"path": name})).unwrap();
    fixture
        .run(
            "fs.write",
            json!({"path": "out.txt", "content": "one\ntwo\n"}),
        )
        .unwrap();
    let missing = format!("missing-{token}.txt");
    fixture
        .run("fs.read", json!({"path": missing}))
        .unwrap_err();
    let names = fixture.events.names();
    assert_eq!(
        names,
        [
            "tool.started",
            "tool.completed",
            "tool.started",
            "file.changed",
            "diff.created",
            "tool.completed",
        ],
        "planning failures emit nothing; the broker owns tool.requested/denied"
    );
    let encoded = serde_json::to_string(&fixture.events.events()).unwrap();
    assert!(!encoded.contains(&token), "{encoded}");
    let events = fixture.events.events();
    match &events[4].event {
        EventKind::DiffCreated {
            files,
            additions,
            deletions,
            ..
        } => {
            assert_eq!(files[0].as_str(), "out.txt");
            assert_eq!((*additions, *deletions), (2, 0));
        }
        other => panic!("unexpected {}", other.name()),
    }
    // An execution failure emits tool.failed with the stable reason.
    fixture.write("gone.txt", "x");
    let (ctx, plan) = fixture
        .plan("fs.read", json!({"path": "gone.txt"}))
        .unwrap();
    std::fs::remove_file(fixture.path().join("gone.txt")).unwrap();
    fixture.execute(&ctx, &plan).unwrap_err();
    match &fixture.events.events().last().unwrap().event {
        EventKind::ToolFailed { code, .. } => assert_eq!(code, "not_found"),
        other => panic!("unexpected {}", other.name()),
    }
}

#[test]
fn cancelled_calls_do_not_execute() {
    let fixture = Fixture::new();
    let (ctx, plan) = fixture
        .plan("fs.write", json!({"path": "a.txt", "content": "x"}))
        .unwrap();
    ctx.cancel.cancel();
    assert!(matches!(
        fixture.execute(&ctx, &plan).unwrap_err(),
        ToolError::Cancelled
    ));
    assert!(!fixture.path().join("a.txt").exists());
}
