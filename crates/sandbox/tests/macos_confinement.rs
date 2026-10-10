//! macOS security tests: `sandbox-exec` write confinement and network denial. Where
//! `sandbox-exec` is missing, negotiation must fail closed instead.
#![cfg(target_os = "macos")]

mod common;

use std::net::TcpListener;

use aidex_sandbox::{Mount, NetworkMode, Requirement, SandboxError, SandboxLevel};
use common::{finish, read, request, sandbox, shell, workspace};

#[test]
fn seatbelt_denies_writes_outside_mounts() {
    let workspace = workspace();
    let sandbox = sandbox();
    let request = request(SandboxLevel::Restricted, &workspace).with_mount(Mount::read_only("/"));
    let plan = match sandbox.negotiate(&request) {
        Ok(plan) => plan,
        Err(SandboxError::Unsupported { missing, .. }) => {
            assert!(missing.contains(&Requirement::FilesystemWriteConfinement));
            return;
        }
        Err(error) => panic!("unexpected error: {error}"),
    };
    let inside = workspace.work.join("inside.txt");
    let outside = workspace.outside.join("escape.txt");
    let script = format!(
        "echo ok > '{}'; echo escaped > '{}'; true",
        inside.display(),
        outside.display()
    );
    let process = sandbox.spawn(&plan, shell(&plan, &script)).expect("spawn");
    let status = finish(&process);
    assert!(
        status.success(),
        "{status:?} {}",
        process.output_so_far().stderr.text()
    );
    assert_eq!(read(&inside).trim(), "ok");
    assert!(!outside.exists(), "write escaped the mounts");
}

#[test]
fn seatbelt_blocks_network_when_offline() {
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
    let script = format!("exec 3<>/dev/tcp/127.0.0.1/{port} && echo connected");
    let command = aidex_sandbox::CommandSpec::new("/bin/bash").args(["-c", &script]);
    let process = sandbox.spawn(&plan, command).expect("spawn");
    let status = finish(&process);
    assert!(!status.success(), "{status:?}");
    assert!(!process.output_so_far().stdout.text().contains("connected"));
    assert!(
        listener.accept().is_err(),
        "a connection reached the listener"
    );
}
