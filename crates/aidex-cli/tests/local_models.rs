//! `aidex model import|verify|remove|load|run` and `aidex runtime list|plan`
//! against an in-process daemon: imports record licence and source, runtime
//! and model problems exit with the documented codes and advice, and removal
//! needs confirmation.

use aidex_inference::gguf::GgufBuilder;
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

fn code(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!("stdout is not JSON ({error}): {}", text(output));
    })
}

#[test]
fn local_models_are_managed_from_the_cli() {
    let root = temp_drive();
    let daemon = Daemon::new(root.path()).unwrap().start().unwrap();
    let sources = tempfile::tempdir().unwrap();
    let file = sources.path().join("Tiny-Coder-Q4_K_M.gguf");
    GgufBuilder::chat_model("qwen2", "Tiny Coder")
        .write_to(&file)
        .unwrap();
    let file = file.to_str().unwrap();

    let imported = aidex(
        &[
            "--json",
            "model",
            "import",
            file,
            "--id",
            "tiny",
            "--license",
            "Apache-2.0",
            "--source",
            "https://huggingface.co/example/tiny",
        ],
        root.path(),
    );
    assert_eq!(code(&imported), 0, "{}", text(&imported));
    let entry = json(&imported);
    assert_eq!(entry["id"], "tiny");
    assert_eq!(entry["license"]["name"], "Apache-2.0");
    assert_eq!(
        entry["source"]["url"],
        "https://huggingface.co/example/tiny"
    );

    let again = aidex(&["model", "import", file, "--id", "tiny"], root.path());
    assert_eq!(code(&again), 1, "{}", text(&again));
    assert!(
        text(&again).contains("never overwrites"),
        "{}",
        text(&again)
    );

    let human = aidex(&["model", "verify", "tiny", "--force"], root.path());
    assert_eq!(code(&human), 0, "{}", text(&human));
    assert!(
        text(&human).contains("full SHA-256 now"),
        "{}",
        text(&human)
    );

    let listed = aidex(&["--json", "model", "list"], root.path());
    assert!(
        json(&listed)["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["model"]["provider_id"] == "local"
                && model["model"]["model_id"] == "tiny"),
        "{}",
        text(&listed)
    );

    let runtimes = aidex(&["runtime", "list"], root.path());
    assert_eq!(code(&runtimes), 0, "{}", text(&runtimes));
    assert!(
        text(&runtimes).contains("Installed: none"),
        "{}",
        text(&runtimes)
    );

    // No runtime installed: exit 7 (model unavailable) with install advice.
    for args in [&["runtime", "plan", "tiny"][..], &["model", "load", "tiny"]] {
        let refused = aidex(args, root.path());
        assert_eq!(code(&refused), 7, "{args:?}: {}", text(&refused));
        assert!(
            text(&refused).contains("aidex runtime install"),
            "{}",
            text(&refused)
        );
    }
    let missing = aidex(&["model", "run", "nope", "hello"], root.path());
    assert_eq!(code(&missing), 8, "{}", text(&missing));
    assert!(
        text(&missing).contains("aidex model import"),
        "{}",
        text(&missing)
    );

    // Removal deletes data, so it needs --yes when there is no terminal.
    let unconfirmed = aidex(&["model", "remove", "tiny"], root.path());
    assert_eq!(code(&unconfirmed), 2, "{}", text(&unconfirmed));
    assert!(root.path().join("models/tiny/model.gguf").is_file());
    let removed = aidex(&["--json", "model", "remove", "tiny", "--yes"], root.path());
    assert_eq!(code(&removed), 0, "{}", text(&removed));
    assert!(!root.path().join("models/tiny/model.gguf").exists());

    daemon.stop();
    daemon.wait().unwrap();
}

#[test]
fn runtime_install_refuses_unpinned_archives() {
    let root = temp_drive();
    let daemon = Daemon::new(root.path()).unwrap().start().unwrap();
    let archive = root.path().join("tmp").join("random.zip");
    std::fs::write(&archive, b"not a pinned archive").unwrap();
    let refused = aidex(
        &["runtime", "install", archive.to_str().unwrap()],
        root.path(),
    );
    assert_eq!(code(&refused), 2, "{}", text(&refused));
    assert!(
        text(&refused).contains("not a pinned runtime"),
        "{}",
        text(&refused)
    );
    let missing = aidex(&["runtime", "install", "no-such-archive.zip"], root.path());
    assert_eq!(code(&missing), 8, "{}", text(&missing));
    daemon.stop();
    daemon.wait().unwrap();
}
