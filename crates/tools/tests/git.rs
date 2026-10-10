//! Git tools against repositories initialised in-test (skipped without `git`).

mod common;

use std::fs;

use aidex_tools::{PermitIssuer, ToolContext, ToolError};
use common::{Fixture, git, git_available, init_repo};
use serde_json::json;

fn repo() -> Option<Fixture> {
    if !git_available() {
        return None;
    }
    let fixture = Fixture::new();
    init_repo(
        fixture.path(),
        &[("README.md", "hello\n"), ("src/lib.rs", "pub fn f() {}\n")],
    );
    Some(fixture)
}

#[test]
fn status_reports_branch_and_changes() {
    let Some(fixture) = repo() else { return };
    fixture.write("README.md", "hello\nworld\n");
    fixture.write("new file.txt", "new\n");
    git(fixture.path(), &["add", "new file.txt"]);
    fixture.write("untracked.txt", "u\n");
    let output = fixture.run("git.status", json!({})).unwrap();
    let value = &output.content;
    assert_eq!(value["branch"]["head"], "main");
    let entries = value["entries"].as_array().unwrap();
    let find = |path: &str| {
        entries
            .iter()
            .find(|entry| entry["path"] == path)
            .unwrap_or_else(|| panic!("{path} missing from {entries:?}"))
    };
    assert_eq!(find("README.md")["worktree"], "M");
    assert_eq!(find("new file.txt")["index"], "A");
    assert_eq!(find("untracked.txt")["kind"], "untracked");
    let limited = fixture
        .run(
            "git.status",
            json!({"paths": ["README.md"], "untracked": false}),
        )
        .unwrap();
    assert_eq!(limited.content["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn diff_log_show_and_branch_inspect_history() {
    let Some(fixture) = repo() else { return };
    fixture.write("README.md", "hello\nchanged\n");
    let diff = fixture.run("git.diff", json!({})).unwrap();
    assert!(diff.content["diff"].as_str().unwrap().contains("+changed"));
    assert_eq!(diff.content["files"][0]["path"], "README.md");
    assert_eq!(diff.content["files"][0]["additions"], 1);
    let staged = fixture.run("git.diff", json!({"mode": "staged"})).unwrap();
    assert_eq!(staged.content["diff"], "");
    git(fixture.path(), &["commit", "-qam", "second commit"]);
    let against = fixture
        .run("git.diff", json!({"mode": "rev", "rev": "HEAD~1"}))
        .unwrap();
    assert!(
        against.content["diff"]
            .as_str()
            .unwrap()
            .contains("+changed")
    );
    let log = fixture.run("git.log", json!({"max_count": 5})).unwrap();
    let commits = log.content["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0]["subject"], "second commit");
    assert_eq!(commits[0]["author_name"], "AiDEX Test");
    assert_eq!(commits[1]["parents"].as_array().unwrap().len(), 0);
    let show = fixture.run("git.show", json!({"rev": "HEAD"})).unwrap();
    assert_eq!(show.content["commit"]["subject"], "second commit");
    assert_eq!(show.content["files"][0]["path"], "README.md");
    git(fixture.path(), &["branch", "feature"]);
    let branches = fixture.run("git.branch", json!({})).unwrap();
    assert_eq!(branches.content["current"], "main");
    let names: Vec<&str> = branches.content["branches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|branch| branch["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["feature", "main"]);
    let error = fixture
        .plan("git.show", json!({"rev": "--output=x"}))
        .unwrap_err();
    assert_eq!(error.reason(), "invalid_input");
    let error = fixture
        .run("git.show", json!({"rev": "no-such-rev"}))
        .unwrap_err();
    assert_eq!(error.reason(), "process_failed");
}

#[test]
fn git_tools_require_a_repository_root_and_a_launcher() {
    if !git_available() {
        return;
    }
    let fixture = Fixture::new();
    let error = fixture.plan("git.status", json!({})).unwrap_err();
    assert!(matches!(error, ToolError::NotARepository));
    // A repository above the workspace is never discovered.
    init_repo(fixture.path(), &[("a.txt", "a\n")]);
    fs::create_dir(fixture.path().join("inner")).unwrap();
    let inner = Fixture {
        root: std::sync::Arc::new(
            aidex_workspace::ConfinedRoot::new(fixture.path().join("inner")).unwrap(),
        ),
        ..Fixture::new()
    };
    assert!(matches!(
        inner.plan("git.status", json!({})).unwrap_err(),
        ToolError::NotARepository
    ));
    // Without a launcher, process tools are unavailable (fail closed).
    let ctx = without_launcher(&fixture);
    let plan = fixture.plan_in(&ctx, "git.status", json!({})).unwrap();
    let error = fixture
        .registry
        .execute(
            &ctx,
            &plan,
            aidex_tools::ExecutionPermit::for_plan(&plan, PermitIssuer::ReadOnly),
        )
        .unwrap_err();
    assert!(matches!(error, ToolError::LauncherUnavailable));
}

fn without_launcher(fixture: &Fixture) -> ToolContext {
    let mut ctx = fixture.ctx();
    ctx.processes = None;
    ctx
}

#[test]
fn commit_stages_and_commits_exactly_the_given_paths() {
    let Some(fixture) = repo() else { return };
    fixture.write("src/lib.rs", "pub fn f() { 1 }\n");
    fixture.write("added.txt", "added\n");
    fixture.write("README.md", "staged but not committed\n");
    git(fixture.path(), &["add", "README.md"]);
    // A hook that would run code is never executed.
    fixture.write(
        ".git/hooks/pre-commit",
        "#!/bin/sh\necho ran > hook-ran\nexit 1\n",
    );
    fixture.write(".git/hooks/post-commit", "#!/bin/sh\necho ran > hook-ran\n");
    #[cfg(unix)]
    for hook in ["pre-commit", "post-commit"] {
        use std::os::unix::fs::PermissionsExt;
        let path = fixture.path().join(".git/hooks").join(hook);
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let (ctx, plan) = fixture
        .plan(
            "git.commit",
            json!({"message": "update lib\n\nwith a body", "paths": ["src/lib.rs", "added.txt"]}),
        )
        .unwrap();
    assert_eq!(plan.action(), aidex_policy::Action::GitCommit);
    let output = fixture.execute(&ctx, &plan).unwrap();
    let oid = output.content["oid"].as_str().unwrap();
    assert_eq!(oid.len(), 40);
    assert!(
        !fixture.path().join("hook-ran").exists(),
        "hooks are disabled"
    );
    let files = git(
        fixture.path(),
        &["show", "--name-only", "--format=%s", "HEAD"],
    );
    assert!(files.starts_with("update lib\n"), "{files}");
    assert!(files.contains("src/lib.rs") && files.contains("added.txt"));
    assert!(
        !files.contains("README.md"),
        "only the named paths are committed"
    );
    let status = git(fixture.path(), &["status", "--porcelain"]);
    assert!(status.contains("M  README.md"), "{status}");
}

#[test]
fn push_uploads_one_branch_to_a_configured_remote() {
    let Some(fixture) = repo() else { return };
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    git(
        fixture.path(),
        &[
            "remote",
            "add",
            "origin",
            &remote.path().display().to_string(),
        ],
    );
    let (ctx, plan) = fixture
        .plan("git.push", json!({"branch": "main", "set_upstream": true}))
        .unwrap();
    assert!(plan.facts().remote_write);
    assert_eq!(
        plan.facts().network.as_ref().unwrap().unresolved,
        ["git remote `origin`"]
    );
    let intent = plan.intent();
    assert!(intent.attributes.remote_write);
    assert!(
        intent.attributes.network.is_none(),
        "destination unknown to policy"
    );
    fixture.execute(&ctx, &plan).unwrap();
    let pushed = git(remote.path(), &["rev-parse", "refs/heads/main"]);
    let local = git(fixture.path(), &["rev-parse", "HEAD"]);
    assert_eq!(pushed, local);
    for bad in [
        json!({"branch": "+main"}),
        json!({"branch": "main", "remote": "https://example.com/x.git"}),
        json!({"branch": "main", "remote": "--receive-pack=evil"}),
    ] {
        assert_eq!(
            fixture.plan("git.push", bad.clone()).unwrap_err().reason(),
            "invalid_input",
            "{bad}"
        );
    }
}
