//! The `aidexd` executable: detached start, duplicate start, status, stop,
//! restart, and recovery after the daemon process is killed.

use aidexd::endpoint;
use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

fn aidexd(args: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aidexd"))
        .args(args)
        .arg("--drive")
        .arg(root)
        .output()
        .expect("run aidexd")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn status(root: &Path) -> Value {
    let output = aidexd(&["status"], root);
    assert!(output.status.success(), "{}", text(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("status JSON")
}

fn kill(pid: u64) {
    let status = if cfg!(windows) {
        Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output()
    } else {
        Command::new("kill").args(["-9", &pid.to_string()]).output()
    }
    .expect("run kill");
    assert!(status.status.success(), "{}", text(&status.stderr));
}

fn wait_unreachable(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while aidexd(&["status"], root).status.success() {
        assert!(Instant::now() < deadline, "killed daemon still answers");
        thread::sleep(Duration::from_millis(100));
    }
}

/// Stop any daemon left running if an assertion fails midway.
struct StopOnDrop<'a>(&'a Path);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        if endpoint::state_path(self.0).exists() {
            let _ = aidexd(&["stop"], self.0);
        }
    }
}

#[test]
fn detached_daemon_lifecycle_through_the_executable() {
    let root = tempfile::tempdir().unwrap();
    aidex_drive::init_with(
        root.path(),
        aidex_drive::InitOptions {
            require_removable: false,
        },
    )
    .unwrap();
    let root = root.path();
    let _cleanup = StopOnDrop(root);

    let started = aidexd(&["start"], root);
    assert!(started.status.success(), "{}", text(&started.stderr));
    assert_eq!(text(&started.stdout).trim(), "ready");
    let first = status(root);
    assert_eq!(first["state"], "ready");
    assert_eq!(first["contract_version"], 2);

    let duplicate = aidexd(&["start"], root);
    assert!(!duplicate.status.success());
    assert!(
        text(&duplicate.stderr).contains("already running"),
        "{}",
        text(&duplicate.stderr)
    );

    let stopped = aidexd(&["stop"], root);
    assert!(stopped.status.success(), "{}", text(&stopped.stderr));
    assert!(!endpoint::state_path(root).exists());
    assert!(!aidexd(&["status"], root).status.success());

    // Restart, then simulate a crash and recover the stale endpoint.
    assert!(aidexd(&["start"], root).status.success());
    let pid = status(root)["pid"].as_u64().unwrap();
    assert_ne!(Some(pid), first["pid"].as_u64());
    kill(pid);
    wait_unreachable(root);
    assert!(
        endpoint::state_path(root).exists(),
        "crash leaves the endpoint"
    );
    let recovered = aidexd(&["start"], root);
    assert!(recovered.status.success(), "{}", text(&recovered.stderr));
    assert_ne!(status(root)["pid"].as_u64(), Some(pid));
    assert!(aidexd(&["stop"], root).status.success());
}
