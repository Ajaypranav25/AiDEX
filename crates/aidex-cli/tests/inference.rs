//! `aidex inference ...` against an in-process daemon: import, catalog, host
//! facts, a start without a runtime, guarded removal, and status.

use aidexd::Daemon;
use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output, Stdio},
};

fn aidex(args: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aidex"))
        .args(args)
        .args(["--drive", root.to_str().expect("utf-8 temp path")])
        .env_remove("AIDEX_DRIVE_ROOT")
        .env("AIDEX_OMP", "aidex-test-missing-omp-executable")
        .stdin(Stdio::null())
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

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {}", text(output)))
}

/// A minimal valid GGUF v3 file: one 1-element F32 tensor.
fn tiny_gguf(path: &Path) {
    let mut gguf = b"GGUF".to_vec();
    gguf.extend(3u32.to_le_bytes());
    gguf.extend(1u64.to_le_bytes());
    gguf.extend(0u64.to_le_bytes());
    gguf.extend(1u64.to_le_bytes());
    gguf.push(b'x');
    gguf.extend(1u32.to_le_bytes());
    gguf.extend(1u64.to_le_bytes());
    gguf.extend(0u32.to_le_bytes());
    gguf.extend(0u64.to_le_bytes());
    gguf.resize(gguf.len().div_ceil(32) * 32, 0);
    gguf.extend(0f32.to_le_bytes());
    std::fs::write(path, gguf).unwrap();
}

#[test]
fn local_models_are_managed_from_the_cli() {
    let root = temp_drive();
    let daemon = Daemon::new(root.path()).unwrap().start().unwrap();
    let sources = tempfile::tempdir().unwrap();
    let file = sources.path().join("tiny.gguf");
    tiny_gguf(&file);

    let imported = aidex(
        &[
            "--json",
            "inference",
            "import",
            file.to_str().unwrap(),
            "--name",
            "Tiny",
        ],
        root.path(),
    );
    assert_eq!(imported.status.code(), Some(0), "{}", text(&imported));
    let model = json(&imported);
    let id = model["id"].as_str().unwrap().to_owned();
    assert!(id.starts_with("m-"), "{id}");
    assert_eq!(model["name"], "Tiny");

    let catalog = aidex(&["inference", "catalog"], root.path());
    assert_eq!(catalog.status.code(), Some(0), "{}", text(&catalog));
    assert!(text(&catalog).contains(&id) && text(&catalog).contains("Runtime: not installed"));

    let host = aidex(&["--json", "inference", "host"], root.path());
    assert_eq!(host.status.code(), Some(0), "{}", text(&host));
    let host = json(&host);
    assert!(host["resources"]["total_ram_bytes"].as_u64().unwrap() > 0);
    assert_eq!(host["gpu"]["state"], "not_probed");

    let start = aidex(&["inference", "start", &id], root.path());
    assert_eq!(start.status.code(), Some(8), "{}", text(&start));
    assert!(text(&start).contains("runtime"), "{}", text(&start));

    let unconfirmed = aidex(&["inference", "remove", &id], root.path());
    assert_eq!(unconfirmed.status.code(), Some(2), "{}", text(&unconfirmed));
    let removed = aidex(&["inference", "remove", &id, "--yes"], root.path());
    assert_eq!(removed.status.code(), Some(0), "{}", text(&removed));

    let status = aidex(&["--json", "inference", "status"], root.path());
    assert_eq!(status.status.code(), Some(0), "{}", text(&status));
    assert_eq!(json(&status)["state"], "stopped");

    daemon.stop();
    daemon.wait().unwrap();
}
