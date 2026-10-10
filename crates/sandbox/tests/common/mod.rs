//! Shared helpers for sandbox integration tests. Every test uses its own temp dirs.
#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use aidex_sandbox::{
    CommandSpec, EnforcementPlan, ExitStatusInfo, ManagedProcess, Mount, NetworkMode, Sandbox,
    SandboxLevel, SandboxRequest,
};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tempfile::TempDir;

pub const HELPER: &str = env!("CARGO_BIN_EXE_aidex-sandbox-exec");

pub fn sandbox() -> Sandbox {
    Sandbox::new(HELPER)
}

/// A temp dir with a `work/` mount and a `scratch/` directory.
pub struct Workspace {
    pub dir: TempDir,
    pub work: PathBuf,
    pub scratch: PathBuf,
    pub outside: PathBuf,
}

pub fn workspace() -> Workspace {
    let dir = tempfile::tempdir().expect("tempdir");
    let work = dir.path().join("work");
    let scratch = dir.path().join("scratch");
    let outside = dir.path().join("outside");
    for path in [&work, &scratch, &outside] {
        std::fs::create_dir_all(path).expect("create dir");
    }
    Workspace {
        dir,
        work,
        scratch,
        outside,
    }
}

/// A request with `work/` mounted read-write as cwd and normal network.
pub fn request(level: SandboxLevel, workspace: &Workspace) -> SandboxRequest {
    let mut request = SandboxRequest::new(level, &workspace.work, &workspace.scratch)
        .with_mount(Mount::read_write(&workspace.work))
        .with_network(NetworkMode::Normal);
    if level == SandboxLevel::Direct {
        request = request.allow_direct();
    }
    request
}

/// The platform shell running `script`.
pub fn shell(plan: &EnforcementPlan, script: &str) -> CommandSpec {
    if cfg!(windows) {
        CommandSpec::new(plan.resolve_program("cmd.exe").expect("cmd.exe"))
            .args(["/D", "/S", "/C", script])
    } else {
        CommandSpec::new("/bin/sh").args(["-c", script])
    }
}

/// Windows PowerShell running `script`.
pub fn powershell(plan: &EnforcementPlan, script: &str) -> CommandSpec {
    CommandSpec::new(
        plan.resolve_program("powershell.exe")
            .expect("powershell.exe"),
    )
    .args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ])
}

/// Wait for the process with a generous test timeout.
pub fn finish(process: &ManagedProcess) -> ExitStatusInfo {
    process
        .wait_timeout(Duration::from_secs(60))
        .expect("wait")
        .expect("process finished within 60 s")
}

/// Poll `condition` every 25 ms until it holds or `timeout` elapses.
pub fn eventually(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

/// Process ids whose command line contains `marker`.
pub fn processes_with_marker(marker: &str) -> Vec<u32> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    system
        .processes()
        .iter()
        .filter(|(_, process)| {
            process
                .cmd()
                .iter()
                .any(|arg| arg.to_string_lossy().contains(marker))
        })
        .map(|(pid, _)| pid.as_u32())
        .collect()
}

/// Whether `pid` is a live (non-zombie) process.
pub fn alive(pid: u32) -> bool {
    let mut system = System::new();
    let target = sysinfo::Pid::from_u32(pid);
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system
        .process(target)
        .is_some_and(|process| process.status() != sysinfo::ProcessStatus::Zombie)
}

/// Wait until every process carrying `marker` has disappeared.
pub fn marker_gone(marker: &str) -> bool {
    eventually(Duration::from_secs(10), || {
        processes_with_marker(marker).is_empty()
    })
}

/// A unique marker per test so concurrent tests never confuse each other's processes.
pub fn marker(name: &str) -> String {
    format!("aidexsbx{}{}", name, std::process::id())
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}
