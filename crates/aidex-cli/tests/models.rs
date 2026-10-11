//! `aidex provider|model|offline` end to end against an in-process daemon and a
//! loopback fake OpenAI-compatible server: limits, tool support, locality, and
//! routing reasons are visible; offline mode refuses remote providers.

use aidexd::Daemon;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::Path,
    process::{Command, Output},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

fn aidex(args: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aidex"))
        .args(args)
        .args(["--drive", root.to_str().expect("utf-8 temp path")])
        .env_remove("AIDEX_DRIVE_ROOT")
        .env("AIDEX_OMP", "aidex-test-missing-omp-executable")
        .output()
        .expect("run aidex")
}

fn temp_drive() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    aidex_drive::init_with(
        root.path(),
        aidex_drive::InitOptions {
            require_removable: false,
        },
    )
    .unwrap();
    root
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not JSON ({error}): {} / {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Answers every request with a `/models` listing; counts connections.
fn model_server() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let connections = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&connections);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            count.fetch_add(1, Ordering::SeqCst);
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let reply = r#"{"object":"list","data":[{"id":"qwen","meta":{"n_ctx_train":32768}},{"id":"mystery"}]}"#;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    (port, connections)
}

#[test]
fn providers_models_and_routing_are_inspectable_from_the_cli() {
    let root = temp_drive();
    let daemon = Daemon::new(root.path()).unwrap().start().unwrap();
    let (port, connections) = model_server();
    let base_url = format!("http://127.0.0.1:{port}/v1");

    let added = aidex(
        &[
            "--json",
            "provider",
            "add",
            "llama",
            "--base-url",
            &base_url,
            "--model",
            "coder",
            "--context-window",
            "16384",
            "--tools",
            "structured",
            "--json-schema",
            "--role",
            "code",
        ],
        root.path(),
    );
    assert_eq!(code(&added), 0, "{}", text(&added));
    assert_eq!(json(&added)["descriptor"]["locality"], "local");

    let listed = aidex(&["--json", "model", "list"], root.path());
    assert_eq!(code(&listed), 0, "{}", text(&listed));
    let models = json(&listed)["models"].as_array().unwrap().clone();
    let summary: Vec<_> = models
        .iter()
        .map(|model| {
            (
                format!(
                    "{}/{}",
                    model["model"]["provider_id"].as_str().unwrap(),
                    model["model"]["model_id"].as_str().unwrap()
                ),
                model["context_window"].as_u64().unwrap(),
                model["capabilities"]["tool_calls"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                model["source"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "llama/coder".to_owned(),
                16_384,
                "structured".to_owned(),
                "configured".to_owned()
            ),
            (
                "llama/mystery".to_owned(),
                4096,
                "none".to_owned(),
                "assumed".to_owned()
            ),
            (
                "llama/qwen".to_owned(),
                32_768,
                "none".to_owned(),
                "discovered".to_owned()
            ),
        ]
    );
    let human = aidex(&["model", "list"], root.path());
    let human = text(&human);
    assert!(
        human.contains("llama/coder") && human.contains("16384") && human.contains("structured"),
        "{human}"
    );

    let routed = aidex(
        &[
            "--json",
            "model",
            "route",
            "--task",
            "code",
            "--tools",
            "--privacy",
            "local-only",
        ],
        root.path(),
    );
    assert_eq!(code(&routed), 0, "{}", text(&routed));
    let routed = json(&routed);
    assert_eq!(routed["decision"]["primary"]["model_id"], "coder");
    assert_eq!(routed["model"]["context_window"], 16_384);
    let rejected = routed["decision"]["rejected"].as_array().unwrap();
    assert!(
        rejected.iter().any(|r| r["reason"] == "no_tool_support"),
        "{rejected:?}"
    );
    let human = text(&aidex(
        &["model", "route", "--task", "code", "--context", "20000"],
        root.path(),
    ));
    assert!(
        human.contains("Primary: llama/qwen") && human.contains("Reasons:"),
        "{human}"
    );
    assert!(
        human.contains("llama/coder: context window 16384"),
        "{human}"
    );

    let refused = aidex(
        &["model", "route", "--task", "code", "--context", "999999"],
        root.path(),
    );
    assert_eq!(code(&refused), 7, "{}", text(&refused));
    assert!(
        text(&refused).contains("rejected: llama/coder"),
        "{}",
        text(&refused)
    );

    let used = aidex(&["model", "use", "llama/qwen"], root.path());
    assert_eq!(code(&used), 0, "{}", text(&used));
    let default = aidex(&["--json", "model", "default"], root.path());
    assert_eq!(json(&default)["default_model"]["model_id"], "qwen");
    assert_eq!(
        code(&aidex(&["model", "use", "llama/absent"], root.path())),
        8
    );

    let health = aidex(&["provider", "health", "llama"], root.path());
    assert_eq!(code(&health), 0, "{}", text(&health));
    assert!(text(&health).contains("ready"));

    daemon.stop();
    daemon.wait().unwrap();
    assert!(connections.load(Ordering::SeqCst) > 0);
}

#[test]
fn offline_mode_refuses_remote_providers_and_keys_never_come_from_argv() {
    let root = temp_drive();
    let daemon = Daemon::new(root.path()).unwrap().start().unwrap();
    let (port, connections) = model_server();
    let base_url = format!("http://127.0.0.1:{port}/v1");

    let with_key = aidex(
        &[
            "provider",
            "add",
            "cloud",
            "--base-url",
            &base_url,
            "--api-key",
            "sk-x",
        ],
        root.path(),
    );
    assert_eq!(code(&with_key), 2, "{}", text(&with_key));

    let added = aidex(
        &[
            "provider",
            "add",
            "cloud",
            "--base-url",
            &base_url,
            "--remote",
            "--secret",
            "cloud-key",
        ],
        root.path(),
    );
    assert_eq!(code(&added), 0, "{}", text(&added));
    assert!(text(&added).contains("aidex vault set cloud-key --consumer provider:cloud"));

    let on = aidex(&["--json", "offline", "on"], root.path());
    assert_eq!(json(&on)["offline"], true);
    let status = aidex(&["offline", "status"], root.path());
    assert!(text(&status).contains("Offline mode: on"));

    let listed = aidex(&["--json", "model", "list"], root.path());
    assert_eq!(code(&listed), 0);
    let listed = json(&listed);
    assert_eq!(listed["network"], "offline");
    assert_eq!(listed["failures"][0]["kind"], "offline_refused");
    let health = aidex(&["provider", "health", "cloud"], root.path());
    assert_eq!(code(&health), 7, "{}", text(&health));
    assert!(text(&health).contains("refused"));
    assert_eq!(
        connections.load(Ordering::SeqCst),
        0,
        "offline mode opened a connection"
    );

    let paths = aidex_drive::DrivePaths::open(root.path()).unwrap();
    let config = std::fs::read_to_string(paths.config().join("providers.toml")).unwrap();
    assert!(config.contains("secret_name = \"cloud-key\""), "{config}");

    assert_eq!(
        code(&aidex(&["provider", "remove", "cloud"], root.path())),
        0
    );
    assert_eq!(
        code(&aidex(&["provider", "remove", "cloud"], root.path())),
        8
    );
    aidex(&["offline", "off"], root.path());
    daemon.stop();
    daemon.wait().unwrap();
}
