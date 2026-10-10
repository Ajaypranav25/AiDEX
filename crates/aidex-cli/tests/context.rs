//! End-to-end `aidex index|search|context|memory|rules` against an in-process
//! daemon: build an index, search it, inspect a pack, edit memory, show rules.

use aidexd::Daemon;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn aidex(args: &[&str], drive: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aidex"))
        .args(args)
        .args(["--drive", drive.to_str().expect("utf-8 temp path")])
        .env_remove("AIDEX_DRIVE_ROOT")
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

fn ok(output: &Output) -> String {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn ok_json(output: &Output) -> Value {
    serde_json::from_str(&ok(output)).expect("stdout is JSON")
}

fn stderr_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stderr).expect("stderr is JSON")
}

#[test]
fn index_search_pack_memory_and_rules_from_the_command_line() {
    let drive = temp_drive();
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("main.rs"),
        "fn main() {\n    start_server(8080);\n}\n\npub fn start_server(port: u16) {}\n",
    )
    .unwrap();
    fs::write(project.path().join(".env"), "PORT=8080\n").unwrap();
    fs::create_dir_all(project.path().join(".aidex")).unwrap();
    fs::write(
        project.path().join(".aidex").join("rules.toml"),
        "schema_version = 1\n[project]\nbuild = \"cargo build\"\n[agent]\nnever_modify = [\"vendor\"]\n",
    )
    .unwrap();
    let daemon = Daemon::new(drive.path()).unwrap().start().unwrap();
    let path = project.path().to_str().unwrap();
    ok_json(&aidex(
        &["--json", "workspace", "add", path, "--name", "app"],
        drive.path(),
    ));
    ok(&aidex(&["workspace", "select", "app"], drive.path()));

    let built = ok_json(&aidex(&["--json", "index", "build"], drive.path()));
    assert_eq!(built["job"]["state"], "completed", "{built}");
    assert_eq!(built["job"]["report"]["added"], 1);
    assert_eq!(built["job"]["report"]["skipped"]["secret_name"], 1);
    assert_eq!(built["index"]["state"], "complete");
    let human = ok(&aidex(
        &["index", "status", "--workspace", "app"],
        drive.path(),
    ));
    assert!(human.contains("Files: 1"), "{human}");

    let hits = ok_json(&aidex(
        &["--json", "search", "--symbol", "start_server"],
        drive.path(),
    ));
    assert_eq!(hits["hits"][0]["path"], "main.rs");
    assert_eq!(hits["hits"][0]["symbol"]["kind"], "function");
    let human = ok(&aidex(&["search", "start", "server"], drive.path()));
    assert!(human.contains("main.rs:"), "{human}");
    let files = ok_json(&aidex(
        &["--json", "search", "--file", ".env"],
        drive.path(),
    ));
    assert_eq!(files["hits"].as_array().unwrap().len(), 0);

    let added = ok_json(&aidex(
        &[
            "--json", "memory", "add", "--tag", "ops", "server", "listens", "on", "8080",
        ],
        drive.path(),
    ));
    let memory_id = added["id"].as_str().unwrap().to_owned();
    assert_eq!(added["source"]["kind"], "user");
    let user = ok_json(&aidex(
        &[
            "--json", "memory", "--user", "add", "prefer", "short", "answers",
        ],
        drive.path(),
    ));
    assert_eq!(user["text"], "prefer short answers");
    let list = ok_json(&aidex(&["--json", "memory", "list"], drive.path()));
    assert_eq!(list["entries"].as_array().unwrap().len(), 1);
    let token = format!("{}{}", "ghp_", "FAKE0fake1FAKE2fake3FAKE4fake5");
    let refused = aidex(&["--json", "memory", "add", &token], drive.path());
    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
    assert!(
        stderr_json(&refused)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("secret")
    );
    ok_json(&aidex(
        &[
            "--json",
            "memory",
            "edit",
            &memory_id,
            "--text",
            "server listens on 9090",
        ],
        drive.path(),
    ));

    let pack = ok_json(&aidex(
        &[
            "--json",
            "context",
            "pack",
            "why",
            "does",
            "start_server",
            "fail",
            "--budget",
            "800",
        ],
        drive.path(),
    ));
    assert!(pack["total_tokens"].as_u64().unwrap() <= 800);
    let items = pack["items"].as_array().unwrap();
    assert_eq!(items[0]["source"]["type"], "rule");
    assert!(items.iter().any(|item| item["source"]["type"] == "file"
        && item["text"].as_str().unwrap().contains("start_server")));
    assert!(
        items
            .iter()
            .any(|item| item["source"]["type"] == "memory" && item["trust"] == "user")
    );
    let human = ok(&aidex(
        &[
            "context",
            "pack",
            "start_server",
            "--budget",
            "800",
            "--text",
        ],
        drive.path(),
    ));
    assert!(human.contains("trust=untrusted"), "{human}");

    let rules = ok_json(&aidex(&["--json", "rules", "show"], drive.path()));
    assert_eq!(rules["rules"]["project"]["build"], "cargo build");
    assert_eq!(rules["rules"]["never_modify"][0], "vendor");
    let human = ok(&aidex(&["rules", "show"], drive.path()));
    assert!(human.contains("only tighten"), "{human}");

    ok_json(&aidex(
        &["--json", "memory", "remove", &memory_id],
        drive.path(),
    ));
    let cancel = ok_json(&aidex(&["--json", "index", "cancel"], drive.path()));
    assert_eq!(cancel["cancelled"], false);
    daemon.stop();
}

#[test]
fn context_commands_report_usage_and_missing_workspaces() {
    let drive = temp_drive();
    let daemon = Daemon::new(drive.path()).unwrap().start().unwrap();
    let none = aidex(&["--json", "index", "status"], drive.path());
    assert_eq!(none.status.code(), Some(2));
    assert!(
        stderr_json(&none)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no workspace")
    );
    let missing = aidex(
        &["--json", "search", "x", "--workspace", "nope"],
        drive.path(),
    );
    assert_eq!(missing.status.code(), Some(8));
    let usage = aidex(&["context", "pack", "x"], drive.path());
    assert_eq!(usage.status.code(), Some(2));
    let conflict = aidex(
        &["memory", "--user", "--session", "s", "list"],
        drive.path(),
    );
    assert_eq!(conflict.status.code(), Some(2));
    let nothing = aidex(&["memory", "edit", "mem_x"], drive.path());
    assert_eq!(nothing.status.code(), Some(2));
    daemon.stop();
}
