//! End-to-end checks of the `aidex` binary: `--json`, stable exit codes, and
//! daemon commands against an in-process daemon on a temporary, initialized
//! developer drive (`init_with` without the removable-media check).

use aidex_events::{EventKind, EventLog};
use aidexd::{Daemon, StopReason, endpoint, services::EVENT_FILE};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn aidex(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aidex"))
        .args(args)
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

fn drive_args<'a>(args: &[&'a str], root: &'a Path) -> Vec<&'a str> {
    let mut all = args.to_vec();
    all.extend(["--drive", root.to_str().expect("utf-8 temp path")]);
    all
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not JSON ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn stderr_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stderr).unwrap_or_else(|error| {
        panic!(
            "stderr is not JSON ({error}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

#[test]
fn daemon_commands_report_status_and_stop_over_ipc() {
    let root = temp_drive();
    let daemon = Daemon::new(root.path()).unwrap().start().unwrap();

    let status = aidex(&drive_args(&["--json", "daemon", "status"], root.path()));
    assert_eq!(
        code(&status),
        0,
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let report = stdout_json(&status);
    assert_eq!(report["state"], "ready");
    assert_eq!(report["contract_version"], 2);
    assert_eq!(report["pid"], std::process::id());

    let human = aidex(&drive_args(&["daemon", "status"], root.path()));
    assert_eq!(code(&human), 0);
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(text.contains("Daemon: ready"), "{text}");
    assert!(text.contains("IPC contract version: 2"), "{text}");

    let duplicate = aidex(&drive_args(&["daemon", "start"], root.path()));
    assert_eq!(code(&duplicate), 1);
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already running"));

    let stopped = aidex(&drive_args(&["--json", "daemon", "stop"], root.path()));
    assert_eq!(
        code(&stopped),
        0,
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    assert_eq!(stdout_json(&stopped)["state"], "stopped");
    assert_eq!(daemon.wait().unwrap(), StopReason::Requested);

    let unavailable = aidex(&drive_args(&["--json", "daemon", "status"], root.path()));
    assert_eq!(code(&unavailable), 3);
    assert!(unavailable.stdout.is_empty());
    let error = &stderr_json(&unavailable)["error"];
    assert_eq!(error["code"], "daemon_unavailable");
    assert_eq!(error["exit_code"], 3);
    assert!(
        error["hint"]
            .as_str()
            .unwrap()
            .contains("aidex daemon start --drive")
    );

    let human = aidex(&drive_args(&["daemon", "stop"], root.path()));
    assert_eq!(code(&human), 3);
    let text = String::from_utf8_lossy(&human.stderr);
    assert!(
        text.starts_with("aidex daemon stop: daemon not running"),
        "{text}"
    );
}

#[test]
fn incompatible_daemon_endpoint_exits_with_code_10() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(endpoint::state_dir(root.path())).unwrap();
    fs::write(
        endpoint::state_path(root.path()),
        r#"{"version":1,"pid":1,"port":1,"token":"opaque"}"#,
    )
    .unwrap();
    let output = aidex(&drive_args(&["--json", "daemon", "status"], root.path()));
    assert_eq!(code(&output), 10);
    assert_eq!(
        stderr_json(&output)["error"]["code"],
        "incompatible_version"
    );
}

#[test]
fn drive_and_usage_failures_use_stable_exit_codes() {
    let root = tempfile::tempdir().unwrap();
    // A plain temporary directory is not a removable drive.
    let status = aidex(&drive_args(&["--json", "status"], root.path()));
    assert_eq!(code(&status), 4);
    assert!(status.stdout.is_empty());
    assert_eq!(stderr_json(&status)["error"]["code"], "drive_error");

    let missing = root.path().join("missing");
    let missing_drive = aidex(&["status", "--drive", missing.to_str().unwrap()]);
    assert_eq!(code(&missing_drive), 4);

    let no_drive = aidex(&["--json", "daemon", "status"]);
    assert_eq!(code(&no_drive), 4);

    assert_eq!(code(&aidex(&["--bogus"])), 2);
    assert_eq!(code(&aidex(&["init"])), 2);
    assert_eq!(code(&aidex(&["--help"])), 0);
    assert_eq!(code(&aidex(&["--version"])), 0);
}

#[test]
fn agent_list_and_doctor_emit_json_reports() {
    let agents = aidex(&["--json", "agent", "list"]);
    assert_eq!(code(&agents), 0);
    let report = stdout_json(&agents);
    assert_eq!(report["agents"][0]["id"], "omp");
    assert_eq!(report["agents"][0]["available"], false);

    let doctor = aidex(&["--json", "doctor"]);
    assert_eq!(code(&doctor), 1);
    let report = stdout_json(&doctor);
    assert_eq!(report["ok"], false);
    assert_eq!(report["drive"]["ok"], false);
    assert_eq!(report["daemon"]["running"], false);
    assert_eq!(report["oh_my_pi"]["available"], false);
    assert!(report["host"]["os"].is_string());
}

#[test]
fn events_command_replays_over_ipc_from_the_cursor() {
    let root = temp_drive();
    {
        let runtime = endpoint::ensure_state_dir(root.path()).unwrap();
        let log = EventLog::open(runtime.join(EVENT_FILE), 20).unwrap();
        log.publish(
            "workspace",
            Some("session".into()),
            EventKind::SessionStarted,
        )
        .unwrap();
        log.publish(
            "workspace",
            Some("session".into()),
            EventKind::SessionFinished,
        )
        .unwrap();
    }
    let daemon = Daemon::new(root.path()).unwrap().start().unwrap();
    let sequences = |output: &Output| -> Vec<u64> {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| {
                serde_json::from_str::<Value>(line).unwrap()["sequence"]
                    .as_u64()
                    .unwrap()
            })
            .collect()
    };
    let all = aidex(&drive_args(
        &["events", "--workspace", "workspace"],
        root.path(),
    ));
    assert_eq!(code(&all), 0, "{}", String::from_utf8_lossy(&all.stderr));
    assert_eq!(sequences(&all), [1, 2]);
    let after = aidex(&drive_args(
        &["events", "--workspace", "workspace", "--after", "1"],
        root.path(),
    ));
    assert_eq!(sequences(&after), [2]);
    let invalid = aidex(&drive_args(
        &["events", "--workspace", "bad id!"],
        root.path(),
    ));
    assert_eq!(code(&invalid), 2);
    daemon.stop();
    daemon.wait().unwrap();

    let down = aidex(&drive_args(
        &["events", "--workspace", "workspace"],
        root.path(),
    ));
    assert_eq!(code(&down), 3);
}
