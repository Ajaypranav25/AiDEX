//! Linux security tests: Landlock filesystem escape attempts and network-namespace
//! denial. Where the host lacks the facility, negotiation must fail closed instead.
#![cfg(target_os = "linux")]

mod common;

use std::{io::Read, net::TcpListener, time::Duration};

use aidex_sandbox::{Mount, NetworkMode, Requirement, SandboxError, SandboxLevel};
use common::{finish, read, request, sandbox, shell, workspace};

fn landlock_available() -> bool {
    sandbox()
        .capabilities()
        .enforcement(Requirement::FilesystemWriteConfinement)
        .satisfies(Requirement::FilesystemWriteConfinement)
}

#[test]
fn restricted_fails_closed_without_landlock() {
    if landlock_available() {
        return;
    }
    let workspace = workspace();
    match sandbox().negotiate(&request(SandboxLevel::Restricted, &workspace)) {
        Err(SandboxError::Unsupported { missing, .. }) => {
            assert!(missing.contains(&Requirement::FilesystemWriteConfinement));
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

#[test]
fn landlock_denies_writes_outside_mounts() {
    if !landlock_available() {
        return;
    }
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Restricted, &workspace))
        .expect("plan");
    let inside = workspace.work.join("inside.txt");
    let outside = workspace.outside.join("escape.txt");
    let script = format!(
        "echo ok > '{}'; echo escaped > '{}'; echo escaped > /tmp/aidex-escape-{}; true",
        inside.display(),
        outside.display(),
        std::process::id()
    );
    let process = sandbox.spawn(&plan, shell(&plan, &script)).expect("spawn");
    let status = finish(&process);
    assert!(status.success(), "{status:?}");
    assert_eq!(read(&inside).trim(), "ok");
    assert!(!outside.exists(), "write escaped the mounts");
    assert!(!std::path::Path::new(&format!("/tmp/aidex-escape-{}", std::process::id())).exists());
    assert!(
        process
            .output_so_far()
            .stderr
            .text()
            .contains("Permission denied")
    );
}

#[test]
fn landlock_denies_reads_outside_mounts() {
    if !landlock_available() {
        return;
    }
    let workspace = workspace();
    let secret = workspace.outside.join("secret.txt");
    std::fs::write(&secret, "outside-secret-content").expect("secret");
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Restricted, &workspace))
        .expect("plan");
    let process = sandbox
        .spawn(&plan, shell(&plan, &format!("cat '{}'", secret.display())))
        .expect("spawn");
    let status = finish(&process);
    assert!(!status.success());
    assert!(
        !process
            .output_so_far()
            .stdout
            .text()
            .contains("outside-secret-content")
    );
}

#[test]
fn read_only_mounts_reject_writes() {
    if !landlock_available() {
        return;
    }
    let workspace = workspace();
    let request = request(SandboxLevel::Restricted, &workspace)
        .with_mount(Mount::read_only(&workspace.outside));
    let sandbox = sandbox();
    let plan = sandbox.negotiate(&request).expect("plan");
    std::fs::write(workspace.outside.join("visible.txt"), "readable").expect("file");
    let script = format!(
        "cat '{0}/visible.txt'; echo nope > '{0}/new.txt'",
        workspace.outside.display()
    );
    let process = sandbox.spawn(&plan, shell(&plan, &script)).expect("spawn");
    finish(&process);
    assert!(process.output_so_far().stdout.text().contains("readable"));
    assert!(!workspace.outside.join("new.txt").exists());
}

#[test]
fn programs_outside_mounts_are_rejected_before_launch() {
    if !landlock_available() {
        return;
    }
    let workspace = workspace();
    let tool = workspace.outside.join("tool.sh");
    std::fs::write(&tool, "#!/bin/sh\necho ran\n").expect("tool");
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Restricted, &workspace))
        .expect("plan");
    match sandbox.spawn(&plan, aidex_sandbox::CommandSpec::new(&tool)) {
        Err(SandboxError::InvalidRequest(message)) => assert!(message.contains("Landlock")),
        other => panic!("expected InvalidRequest, got {other:?}"),
    }
}

#[test]
fn helper_failures_are_reported_and_nothing_runs() {
    if !landlock_available() {
        return;
    }
    let workspace = workspace();
    let request = request(SandboxLevel::Restricted, &workspace)
        .with_mount(Mount::read_only(&workspace.outside));
    let sandbox = sandbox();
    let plan = sandbox.negotiate(&request).expect("plan");
    std::fs::remove_dir_all(&workspace.outside).expect("remove mount");
    let marker = workspace.work.join("ran.txt");
    let script = format!("echo ran > '{}'", marker.display());
    match sandbox.spawn(&plan, shell(&plan, &script)) {
        Err(SandboxError::EnforcementFailed { stage, .. }) => assert_eq!(stage, "landlock"),
        other => panic!("expected EnforcementFailed, got {other:?}"),
    }
    assert!(!marker.exists());
}

fn connect_script(port: u16) -> String {
    format!("exec 3<>/dev/tcp/127.0.0.1/{port} && echo connected >&3 && echo connected")
}

#[test]
fn offline_network_blocks_connections_or_fails_closed() {
    let workspace = workspace();
    let sandbox = sandbox();
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    listener.set_nonblocking(true).expect("nonblocking");
    let port = listener.local_addr().expect("addr").port();

    let offline = request(SandboxLevel::Brokered, &workspace).with_network(NetworkMode::Offline);
    let plan = match sandbox.negotiate(&offline) {
        Ok(plan) => plan,
        Err(SandboxError::Unsupported { missing, .. }) => {
            assert_eq!(missing, vec![Requirement::NetworkDeny]);
            return;
        }
        Err(error) => panic!("unexpected error: {error}"),
    };
    let bash = plan.resolve_program("bash").expect("bash");
    let command = aidex_sandbox::CommandSpec::new(&bash).args(["-c", &connect_script(port)]);
    let process = sandbox.spawn(&plan, command).expect("spawn");
    let status = finish(&process);
    assert!(!status.success(), "{status:?}");
    assert!(!process.output_so_far().stdout.text().contains("connected"));
    assert!(
        listener.accept().is_err(),
        "a connection reached the listener"
    );

    // Control: the same connection succeeds with normal networking.
    let normal = request(SandboxLevel::Brokered, &workspace);
    let plan = sandbox.negotiate(&normal).expect("plan");
    let command = aidex_sandbox::CommandSpec::new(&bash).args(["-c", &connect_script(port)]);
    let process = sandbox.spawn(&plan, command).expect("spawn");
    assert!(finish(&process).success());
    let accepted = common::eventually(Duration::from_secs(5), || match listener.accept() {
        Ok((mut stream, _)) => {
            let mut buffer = String::new();
            let _ = stream.read_to_string(&mut buffer);
            true
        }
        Err(_) => false,
    });
    assert!(accepted);
}
