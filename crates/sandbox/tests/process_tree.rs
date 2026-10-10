//! Process-tree termination: grandchildren die on kill, drop, timeout, registry stop,
//! and when the launched program exits.

mod common;

use std::time::Duration;

use aidex_sandbox::{
    EnforcementPlan, KillReason, ManagedProcess, ProcessRegistry, ResourceLimits, Sandbox,
    SandboxError, SandboxLevel, StopReason,
};
use common::{
    HELPER, eventually, finish, marker_gone, processes_with_marker, request, sandbox, shell,
    workspace,
};

/// A per-test number that appears in the command line of every process in the tree.
fn marker(index: u64) -> String {
    (5_000_000 + u64::from(std::process::id() % 100_000) * 100 + index).to_string()
}

/// A shell with one background and one foreground long-running grandchild.
fn grandchildren(plan: &EnforcementPlan, marker: &str) -> aidex_sandbox::CommandSpec {
    if cfg!(windows) {
        shell(
            plan,
            &format!("start /b ping -n {marker} 127.0.0.1 >nul & ping -n {marker} 127.0.0.1 >nul"),
        )
    } else {
        shell(plan, &format!("sleep {marker} & sleep {marker}; wait"))
    }
}

/// Spawn the tree and wait until the shell and both grandchildren are running.
fn spawn_tree(sandbox: &Sandbox, plan: &EnforcementPlan, marker: &str) -> ManagedProcess {
    let process = sandbox
        .spawn(plan, grandchildren(plan, marker))
        .expect("spawn");
    assert!(
        eventually(Duration::from_secs(20), || processes_with_marker(marker)
            .len()
            >= 3),
        "tree did not start: {:?}",
        processes_with_marker(marker)
    );
    process
}

#[test]
fn kill_tree_terminates_grandchildren() {
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let marker = marker(1);
    let process = spawn_tree(&sandbox, &plan, &marker);
    let tree = processes_with_marker(&marker);
    process.kill_tree().expect("kill");
    let status = finish(&process);
    assert_eq!(status.killed_by, Some(KillReason::Requested));
    assert!(!status.success());
    assert!(
        marker_gone(&marker),
        "survivors: {:?}",
        processes_with_marker(&marker)
    );
    assert!(tree.iter().all(|pid| !common::alive(*pid)));
}

#[test]
fn dropping_the_handle_terminates_the_tree() {
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let marker = marker(2);
    let process = spawn_tree(&sandbox, &plan, &marker);
    drop(process);
    assert!(
        marker_gone(&marker),
        "survivors: {:?}",
        processes_with_marker(&marker)
    );
}

#[test]
fn wall_timeout_terminates_the_tree() {
    let workspace = workspace();
    let sandbox = sandbox();
    let request = request(SandboxLevel::Brokered, &workspace).with_limits(ResourceLimits {
        wall_timeout: Some(Duration::from_secs(5)),
        ..ResourceLimits::default()
    });
    let plan = sandbox.negotiate(&request).expect("plan");
    let marker = marker(3);
    let process = spawn_tree(&sandbox, &plan, &marker);
    let status = finish(&process);
    assert_eq!(status.killed_by, Some(KillReason::WallTimeout));
    assert!(status.elapsed >= Duration::from_secs(5), "{status:?}");
    assert!(status.elapsed < Duration::from_secs(40), "{status:?}");
    assert!(
        marker_gone(&marker),
        "survivors: {:?}",
        processes_with_marker(&marker)
    );
}

#[test]
fn registry_terminate_all_stops_every_tree() {
    let workspace = workspace();
    let registry = ProcessRegistry::new();
    let sandbox = Sandbox::with_registry(HELPER, registry.clone());
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let (first_marker, second_marker) = (marker(4), marker(5));
    let first = spawn_tree(&sandbox, &plan, &first_marker);
    let second = spawn_tree(&sandbox, &plan, &second_marker);
    assert_eq!(registry.len(), 2);

    let report = registry.terminate_all(StopReason::EmergencyStop);
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.terminated.len(), 2, "{report:?}");
    assert!(report.terminated.contains(first.id()));
    assert!(report.terminated.contains(second.id()));
    for process in [&first, &second] {
        assert_eq!(
            finish(process).killed_by,
            Some(KillReason::Stopped(StopReason::EmergencyStop))
        );
    }
    assert!(marker_gone(&first_marker));
    assert!(marker_gone(&second_marker));
    assert!(registry.is_empty());
}

#[test]
fn shutdown_closes_the_registry_until_reopened() {
    let workspace = workspace();
    let registry = ProcessRegistry::new();
    let sandbox = Sandbox::with_registry(HELPER, registry.clone());
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let marker = marker(6);
    let process = spawn_tree(&sandbox, &plan, &marker);
    let report = registry.shutdown(StopReason::DriveRemoved);
    assert_eq!(report.terminated, vec![process.id().clone()]);
    assert!(marker_gone(&marker));
    assert!(!registry.is_accepting());
    assert!(matches!(
        sandbox.spawn(&plan, shell(&plan, "exit 0")),
        Err(SandboxError::RegistryClosed)
    ));
    registry.reopen();
    let process = sandbox
        .spawn(&plan, shell(&plan, "exit 0"))
        .expect("spawn after reopen");
    assert!(finish(&process).success());
}

#[test]
fn leftover_descendants_die_when_the_program_exits() {
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let marker = marker(7);
    let script = if cfg!(windows) {
        format!("start /b ping -n {marker} 127.0.0.1 >nul & ping -n 3 127.0.0.1 >nul & exit 0")
    } else {
        format!("sleep {marker} & sleep 2; exit 0")
    };
    let process = sandbox.spawn(&plan, shell(&plan, &script)).expect("spawn");
    let status = finish(&process);
    assert!(status.success(), "{status:?}");
    assert!(
        marker_gone(&marker),
        "survivors: {:?}",
        processes_with_marker(&marker)
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn member_pids_lists_the_running_tree() {
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let marker = marker(8);
    let process = spawn_tree(&sandbox, &plan, &marker);
    let members = process.member_pids().expect("members");
    assert!(members.contains(&process.pid()), "{members:?}");
    for pid in processes_with_marker(&marker) {
        assert!(members.contains(&pid), "{pid} not in {members:?}");
    }
    process.kill_tree().expect("kill");
    finish(&process);
    assert!(process.member_pids().expect("members").is_empty());
}

#[test]
fn exit_codes_are_reported() {
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let process = sandbox.spawn(&plan, shell(&plan, "exit 7")).expect("spawn");
    let status = finish(&process);
    assert_eq!(status.code, Some(7));
    assert_eq!(status.killed_by, None);
    assert!(!status.success());
    assert!(status.output_complete);
}

#[test]
fn spawn_fails_closed_when_the_working_directory_vanished() {
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    std::fs::remove_dir_all(&workspace.work).expect("remove cwd");
    match sandbox.spawn(&plan, shell(&plan, "exit 0")) {
        Err(SandboxError::Spawn(_)) => {}
        other => panic!("expected Spawn error, got {other:?}"),
    }
    assert!(sandbox.registry().is_empty());
}
