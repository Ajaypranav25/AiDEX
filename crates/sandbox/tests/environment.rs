//! Environment isolation: parent secrets never reach sandboxed children.

mod common;

use std::process::Command;

use aidex_sandbox::SandboxLevel;
use common::{finish, request, sandbox, shell, workspace};

const SECRET_VAR: &str = "AIDEX_TEST_SECRET_TOKEN";
const SECRET_VALUE: &str = "aidex-parent-secret-4f9b2c";

/// Re-run `test_name` in a child test process whose environment holds the secret.
/// Returns true when the caller is that child and should run the real assertions.
fn in_parent_with_secret(test_name: &str) -> bool {
    if std::env::var_os(SECRET_VAR).is_some() {
        return true;
    }
    let status = Command::new(std::env::current_exe().expect("test binary"))
        .args([test_name, "--exact", "--nocapture", "--test-threads=1"])
        .env(SECRET_VAR, SECRET_VALUE)
        .status()
        .expect("re-run test with secret");
    assert!(status.success(), "child test run failed: {status}");
    false
}

#[test]
fn parent_secret_is_not_visible_to_the_child() {
    if !in_parent_with_secret("parent_secret_is_not_visible_to_the_child") {
        return;
    }
    assert_eq!(std::env::var(SECRET_VAR).as_deref(), Ok(SECRET_VALUE));
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    assert!(!plan.env.names().iter().any(|name| name == SECRET_VAR));

    // Expand the variable directly and dump the whole environment.
    let script = if cfg!(windows) {
        format!("echo [%{SECRET_VAR}%] & set")
    } else {
        format!("echo \"[${SECRET_VAR}]\"; env")
    };
    let process = sandbox.spawn(&plan, shell(&plan, &script)).expect("spawn");
    let status = finish(&process);
    assert!(status.success(), "{status:?}");
    let stdout = process.output_so_far().stdout.text();
    assert!(!stdout.contains(SECRET_VALUE), "secret leaked: {stdout}");
    assert!(
        !stdout.contains(&format!("{SECRET_VAR}=")),
        "variable leaked: {stdout}"
    );
    if cfg!(windows) {
        assert!(stdout.contains(&format!("[%{SECRET_VAR}%]")), "{stdout}");
    } else {
        assert!(stdout.contains("[]"), "{stdout}");
    }

    #[cfg(windows)]
    {
        let script = format!(
            "Write-Output ('[' + $env:{SECRET_VAR} + ']'); Get-ChildItem env: | ForEach-Object {{ $_.Name + '=' + $_.Value }}"
        );
        let process = sandbox
            .spawn(&plan, common::powershell(&plan, &script))
            .expect("spawn powershell");
        let status = finish(&process);
        assert!(
            status.success(),
            "{status:?} {}",
            process.output_so_far().stderr.text()
        );
        let stdout = process.output_so_far().stdout.text();
        assert!(stdout.contains("[]"), "{stdout}");
        assert!(!stdout.contains(SECRET_VALUE), "secret leaked: {stdout}");
    }
}

#[test]
fn explicit_additions_reach_only_the_child_that_was_given_them() {
    let workspace = workspace();
    let sandbox = sandbox();
    let granted =
        request(SandboxLevel::Brokered, &workspace).with_env("TOOL_TOKEN", "scoped-grant-81");
    let plan = sandbox.negotiate(&granted).expect("plan");
    let script = if cfg!(windows) {
        "echo [%TOOL_TOKEN%]"
    } else {
        "echo \"[$TOOL_TOKEN]\""
    };
    let process = sandbox.spawn(&plan, shell(&plan, script)).expect("spawn");
    finish(&process);
    assert!(
        process
            .output_so_far()
            .stdout
            .text()
            .contains("[scoped-grant-81]")
    );

    let plain = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let process = sandbox.spawn(&plain, shell(&plain, script)).expect("spawn");
    finish(&process);
    assert!(
        !process
            .output_so_far()
            .stdout
            .text()
            .contains("scoped-grant-81")
    );
}

#[test]
fn temp_and_home_point_into_the_scratch_directory() {
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let script = if cfg!(windows) {
        "echo %TEMP%& echo %USERPROFILE%"
    } else {
        "echo \"$TMPDIR\"; echo \"$HOME\""
    };
    let process = sandbox.spawn(&plan, shell(&plan, script)).expect("spawn");
    assert!(finish(&process).success());
    let stdout = process.output_so_far().stdout.text();
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(lines.first().copied(), plan.temp_dir.to_str(), "{stdout}");
    assert_eq!(lines.get(1).copied(), plan.home_dir.to_str(), "{stdout}");
    assert!(plan.temp_dir.is_dir());
    assert!(plan.home_dir.is_dir());
}

#[test]
fn stdin_bytes_reach_the_target_after_the_plan() {
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let command = if cfg!(windows) {
        shell(&plan, "sort")
    } else {
        shell(&plan, "cat")
    }
    .stdin(b"line-from-parent\n".to_vec());
    let process = sandbox.spawn(&plan, command).expect("spawn");
    assert!(finish(&process).success());
    assert!(
        process
            .output_so_far()
            .stdout
            .text()
            .contains("line-from-parent")
    );
}
