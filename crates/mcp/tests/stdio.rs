//! Stdio round trips against the Python fixture server, launched through the
//! sandbox with a cleared environment and a vault-style secret.

mod support;

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use aidex_core::CancelToken;
use aidex_mcp::{
    EnvSecret, LATEST_PROTOCOL_VERSION, LogBuffer, McpClient, McpConfigStore, McpErrorKind,
    McpHost, McpStatus, McpSupervisor, Scrubber, Timeouts,
};
use serde_json::{Value, json};
use support::{StaticSecrets, host, sentinel, stdio_config};

fn cancel() -> CancelToken {
    CancelToken::new().with_timeout(Duration::from_secs(60))
}

fn log_text(logs: &LogBuffer) -> String {
    logs.tail(1000)
        .lines
        .iter()
        .map(|line| line.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_client_speaks_mcp_with_a_sandboxed_stdio_server() {
    let scratch = tempfile::tempdir().unwrap();
    let secret = sentinel();
    let mut config = stdio_config("fixture", "noisy");
    config.env_allowlist = vec!["CARGO_PKG_NAME".into()];
    config.env_from_secrets = vec![EnvSecret {
        env: "FIXTURE_TOKEN".into(),
        secret_name: "fixture-token".into(),
    }];
    config.validate().unwrap();
    let host = host(
        scratch.path(),
        StaticSecrets::default().with("fixture-token", "mcp:fixture", &secret),
    );
    let logs = Arc::new(LogBuffer::new());
    let launched = host.launch_stdio(&config, &logs).unwrap();
    assert!(!format!("{launched:?}").contains(&secret));
    let mut client = McpClient::stdio(
        launched.process,
        Arc::clone(&logs),
        Scrubber::new(launched.secrets),
        Timeouts::default(),
    )
    .unwrap();

    let identity = client.initialize(&cancel()).unwrap();
    assert_eq!(identity.name, "fixture");
    assert_eq!(identity.version, "1.0.0");
    assert_eq!(identity.protocol_version, LATEST_PROTOCOL_VERSION);
    assert!(identity.tools);

    // Seven tools over four pages; the schema without a type is normalized.
    let listing = client.list_tools(&cancel()).unwrap();
    let names: Vec<&str> = listing.tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        ["echo", "add", "fail", "env", "image", "slow", "crash"]
    );
    assert_eq!(listing.tools[1].input_schema["type"], "object");
    assert_eq!(listing.tools[0].description, "Echo the given text");

    let echo = client
        .call_tool("echo", json!({"text": "hello"}), &cancel())
        .unwrap();
    assert_eq!(echo.content_text, "hello");
    assert!(!echo.is_error && !echo.truncated);
    let sum = client
        .call_tool("add", json!({"a": 2, "b": 3}), &cancel())
        .unwrap();
    assert_eq!(sum.content_text, "5");
    let failed = client.call_tool("fail", Value::Null, &cancel()).unwrap();
    assert!(failed.is_error);
    assert_eq!(failed.content_text, "the fixture tool failed");
    let image = client.call_tool("image", json!({}), &cancel()).unwrap();
    assert!(image.content_text.starts_with("[image: image/png"));
    let unknown = client
        .call_tool("missing", json!({}), &cancel())
        .unwrap_err();
    assert_eq!(unknown.kind, McpErrorKind::ServerError);
    assert!(unknown.message.contains("-32602"), "{}", unknown.message);
    let refused = client.call_tool("echo", json!([1]), &cancel()).unwrap_err();
    assert_eq!(refused.kind, McpErrorKind::InvalidRequest);
    client.ping(&cancel()).unwrap();

    // The environment is cleared: allowlisted names pass, others do not, and
    // the secret reaches the server but never comes back.
    let env = client
        .call_tool(
            "env",
            json!({"names": ["CARGO_PKG_NAME", "CARGO_MANIFEST_DIR", "FIXTURE_TOKEN"]}),
            &cancel(),
        )
        .unwrap();
    assert!(!env.content_text.contains(&secret));
    let reported: Value = serde_json::from_str(&env.content_text).unwrap();
    assert_eq!(reported["CARGO_PKG_NAME"], "aidex-mcp");
    assert_eq!(reported["CARGO_MANIFEST_DIR"], Value::Null);
    assert_eq!(reported["FIXTURE_TOKEN"], "[REDACTED]");

    // A request that times out is abandoned; its late answer is ignored.
    let started = Instant::now();
    let slow = client
        .call_tool(
            "slow",
            json!({"seconds": 1.5}),
            &CancelToken::new().with_timeout(Duration::from_millis(300)),
        )
        .unwrap_err();
    assert_eq!(slow.kind, McpErrorKind::Timeout);
    assert!(started.elapsed() < Duration::from_secs(1));
    let after = client
        .call_tool("echo", json!({"text": "after"}), &cancel())
        .unwrap();
    assert_eq!(after.content_text, "after");

    // The server exiting ends the connection with its exit code and last words.
    let crashed = client.call_tool("crash", json!({}), &cancel()).unwrap_err();
    assert_eq!(crashed.kind, McpErrorKind::Disconnected);
    assert!(crashed.message.contains("code 3"), "{}", crashed.message);
    assert!(!client.is_alive());
    client.close();

    let text = log_text(&logs);
    assert!(text.contains("fixture starting (noisy)"), "{text}");
    assert!(text.contains("ignored a non-JSON line"), "{text}");
    assert!(text.contains("info: listing from 0"), "{text}");
    assert!(
        text.contains("declined the server's `roots/list` request"),
        "{text}"
    );
    assert!(text.contains("fixture initialized"), "{text}");
    assert!(!text.contains(&secret));
}

fn supervisor(
    scratch: &std::path::Path,
    secrets: StaticSecrets,
    timeouts: Timeouts,
) -> McpSupervisor {
    let store = McpConfigStore::in_dir(scratch.join("mcp"));
    McpSupervisor::with_timeouts(
        store,
        Arc::new(host(&scratch.join("scratch"), secrets)),
        timeouts,
    )
}

#[test]
fn the_supervisor_starts_on_demand_restarts_and_stops() {
    let scratch = tempfile::tempdir().unwrap();
    let supervisor = supervisor(
        scratch.path(),
        StaticSecrets::default(),
        Timeouts::default(),
    );
    supervisor
        .store()
        .add(&stdio_config("fixture", "normal"))
        .unwrap();
    assert_eq!(supervisor.state("fixture").status, McpStatus::Stopped);

    // The first tool listing starts the server.
    let tools = supervisor.list_tools("fixture", &cancel()).unwrap();
    assert_eq!(tools.len(), 7);
    let state = supervisor.state("fixture");
    assert_eq!(state.status, McpStatus::Running);
    assert_eq!(state.server.as_ref().unwrap().name, "fixture");
    assert!(state.process_id.as_deref().unwrap().starts_with("proc_"));
    assert!(state.started_at.is_some());
    assert!(supervisor.has_active());

    let echo = supervisor
        .call_tool("fixture", "echo", json!({"text": "hi"}), &cancel())
        .unwrap();
    assert_eq!(echo.content_text, "hi");
    let unknown = supervisor
        .call_tool("fixture", "nope", json!({}), &cancel())
        .unwrap_err();
    assert_eq!(unknown.kind, McpErrorKind::InvalidRequest);
    supervisor.ping("fixture", &cancel()).unwrap();

    let toolsets = supervisor.running_toolsets(Some("ws_any")).unwrap();
    assert_eq!(toolsets.len(), 1);
    assert_eq!(toolsets[0].server_id, "fixture");
    assert_eq!(toolsets[0].tools.len(), 7);

    // A crash marks the server failed; the next call starts it again.
    let crashed = supervisor
        .call_tool("fixture", "crash", json!({}), &cancel())
        .unwrap_err();
    assert_eq!(crashed.kind, McpErrorKind::Disconnected);
    let state = supervisor.state("fixture");
    assert_eq!(state.status, McpStatus::Failed);
    assert_eq!(state.error.unwrap().reason, McpErrorKind::Disconnected);
    assert!(supervisor.running_toolsets(None).unwrap().is_empty());
    let again = supervisor
        .call_tool("fixture", "add", json!({"a": 1, "b": 1}), &cancel())
        .unwrap();
    assert_eq!(again.content_text, "2");

    let restarted = supervisor.restart("fixture", &cancel()).unwrap();
    assert_eq!(restarted.status, McpStatus::Running);
    let stopped = supervisor.stop("fixture");
    assert_eq!(stopped.status, McpStatus::Stopped);
    assert!(stopped.tools.is_empty());
    assert!(!supervisor.has_active());
    let logs = supervisor.logs("fixture", 1000);
    let text: Vec<&str> = logs.lines.iter().map(|l| l.text.as_str()).collect();
    assert!(text.contains(&"stopped"), "{text:?}");

    // Disabled servers neither start nor answer.
    supervisor.store().set_enabled("fixture", false).unwrap();
    let disabled = supervisor.start("fixture", &cancel()).unwrap_err();
    assert_eq!(disabled.kind, McpErrorKind::Disabled);
    let disabled = supervisor
        .call_tool("fixture", "echo", json!({"text": "x"}), &cancel())
        .unwrap_err();
    assert_eq!(disabled.kind, McpErrorKind::Disabled);

    supervisor.store().set_enabled("fixture", true).unwrap();
    supervisor.start("fixture", &cancel()).unwrap();
    assert_eq!(supervisor.stop_all("vault locked"), 1);
    assert_eq!(supervisor.state("fixture").status, McpStatus::Stopped);
    assert_eq!(supervisor.stop_all("again"), 0);
}

#[test]
fn start_failures_set_the_server_status() {
    let scratch = tempfile::tempdir().unwrap();
    let supervisor = supervisor(
        scratch.path(),
        StaticSecrets::default(),
        Timeouts {
            initialize: Duration::from_millis(1500),
            ..Timeouts::default()
        },
    );
    let store = supervisor.store();

    let mut needs_secret = stdio_config("needs-secret", "normal");
    needs_secret.env_from_secrets = vec![EnvSecret {
        env: "FIXTURE_TOKEN".into(),
        secret_name: "missing-token".into(),
    }];
    store.add(&needs_secret).unwrap();
    let error = supervisor.start("needs-secret", &cancel()).unwrap_err();
    assert_eq!(error.kind, McpErrorKind::NeedsAuth);
    let state = supervisor.state("needs-secret");
    assert_eq!(state.status, McpStatus::NeedsAuth);
    assert!(state.process_id.is_none());

    store.add(&stdio_config("old", "old-version")).unwrap();
    let error = supervisor.start("old", &cancel()).unwrap_err();
    assert_eq!(error.kind, McpErrorKind::Protocol);
    assert_eq!(supervisor.state("old").status, McpStatus::Failed);

    store.add(&stdio_config("exits", "exit")).unwrap();
    let error = supervisor.start("exits", &cancel()).unwrap_err();
    assert_eq!(error.kind, McpErrorKind::Disconnected);
    assert!(error.message.contains("code 2"), "{}", error.message);
    assert!(
        error.message.contains("fixture refusing to start"),
        "{}",
        error.message
    );

    store.add(&stdio_config("silent", "silent")).unwrap();
    let error = supervisor.start("silent", &cancel()).unwrap_err();
    assert_eq!(error.kind, McpErrorKind::Timeout);
    assert!(error.message.contains("initialize"), "{}", error.message);

    let mut missing = stdio_config("missing", "normal");
    missing.command = Some("aidex-no-such-mcp-server".into());
    store.add(&missing).unwrap();
    let error = supervisor.start("missing", &cancel()).unwrap_err();
    assert_eq!(error.kind, McpErrorKind::LaunchFailed);

    let error = supervisor.start("unknown", &cancel()).unwrap_err();
    assert_eq!(error.kind, McpErrorKind::NotFound);
    assert!(!supervisor.has_active());
}

#[test]
fn stop_interrupts_a_call_in_flight() {
    let scratch = tempfile::tempdir().unwrap();
    let supervisor = Arc::new(supervisor(
        scratch.path(),
        StaticSecrets::default(),
        Timeouts::default(),
    ));
    supervisor
        .store()
        .add(&stdio_config("fixture", "normal"))
        .unwrap();
    supervisor.start("fixture", &cancel()).unwrap();
    let calling = {
        let supervisor = Arc::clone(&supervisor);
        thread::spawn(move || {
            supervisor.call_tool("fixture", "slow", json!({"seconds": 20}), &cancel())
        })
    };
    thread::sleep(Duration::from_millis(400));
    let started = Instant::now();
    let stopped = supervisor.stop("fixture");
    assert_eq!(stopped.status, McpStatus::Stopped);
    let outcome = calling.join().unwrap().unwrap_err();
    assert_eq!(outcome.kind, McpErrorKind::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "stop took {:?}",
        started.elapsed()
    );
    supervisor.forget("fixture");
    assert!(supervisor.logs("fixture", 10).lines.is_empty());
}
