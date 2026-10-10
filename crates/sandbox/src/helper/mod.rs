//! `aidex-sandbox-exec`: the small trusted helper that applies a plan to itself and
//! then starts the target. It must ship next to `aidexd`.
//!
//! * Windows: the parent assigns the helper to the Job Object *before* sending the plan,
//!   so the target and all of its descendants start inside the job.
//! * Linux: the helper enters user/network namespaces, sets rlimits, applies Landlock,
//!   and `exec`s the target.
//! * macOS: the helper sets rlimits and `exec`s the target through `sandbox-exec`.

pub(crate) mod protocol;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use std::{
    ffi::OsString,
    fs::File,
    io::{self, Write},
};

use protocol::{HELPER_FAILURE_EXIT, HelperPlan};

/// A setup step that failed; reported to the parent as a failure line.
pub(crate) struct HelperFailure {
    pub stage: &'static str,
    pub message: String,
}

impl HelperFailure {
    pub(crate) fn new(stage: &'static str, message: impl ToString) -> Self {
        Self {
            stage,
            message: message.to_string(),
        }
    }
}

/// Entry point used by the `aidex-sandbox-exec` binary. Returns the process exit code.
pub fn run(args: impl IntoIterator<Item = OsString>) -> i32 {
    let args: Vec<OsString> = args.into_iter().skip(1).collect();
    match args.first().and_then(|arg| arg.to_str()) {
        Some("--exec") if args.len() == 1 => exec_mode(),
        Some("--probe") if args.len() == 1 => probe_mode(),
        _ => {
            let _ = writeln!(
                io::stderr(),
                "aidex-sandbox-exec is an internal AiDEX helper; it is started by the sandbox with --exec or --probe"
            );
            2
        }
    }
}

fn exec_mode() -> i32 {
    let plan = match read_plan_from_stdin() {
        Ok(plan) => plan,
        Err(error) => return report(&HelperFailure::new("plan", error)),
    };
    match platform_exec(&plan) {
        Ok(code) => code,
        Err(failure) => report(&failure),
    }
}

fn probe_mode() -> i32 {
    #[cfg(target_os = "linux")]
    let report = linux::probe();
    #[cfg(not(target_os = "linux"))]
    let report = protocol::ProbeReport {
        version: protocol::PROTOCOL_VERSION,
        ..protocol::ProbeReport::default()
    };
    match serde_json::to_vec(&report) {
        Ok(json) => {
            let mut stdout = io::stdout();
            if stdout
                .write_all(&json)
                .and_then(|()| stdout.flush())
                .is_ok()
            {
                0
            } else {
                1
            }
        }
        Err(_) => 1,
    }
}

/// Report a failure line on stderr and return the failure exit code.
fn report(failure: &HelperFailure) -> i32 {
    let _ = protocol::write_failure(&mut io::stderr(), failure.stage, &failure.message);
    HELPER_FAILURE_EXIT
}

/// Tell the parent that every restriction is in place.
pub(crate) fn signal_ready() -> Result<(), HelperFailure> {
    let mut stdout = io::stdout();
    stdout
        .write_all(protocol::READY_FRAME)
        .and_then(|()| stdout.flush())
        .map_err(|error| HelperFailure::new("ready", error))
}

/// Duplicate stdin as an unbuffered file so the plan read never consumes target input.
fn unbuffered_stdin() -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd;
        Ok(File::from(io::stdin().as_fd().try_clone_to_owned()?))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsHandle;
        Ok(File::from(io::stdin().as_handle().try_clone_to_owned()?))
    }
}

fn read_plan_from_stdin() -> io::Result<HelperPlan> {
    let mut stdin = unbuffered_stdin()?;
    protocol::read_plan(&mut stdin)
}

#[cfg(target_os = "linux")]
fn platform_exec(plan: &HelperPlan) -> Result<i32, HelperFailure> {
    linux::exec(plan)
}

#[cfg(target_os = "macos")]
fn platform_exec(plan: &HelperPlan) -> Result<i32, HelperFailure> {
    macos::exec(plan)
}

#[cfg(windows)]
fn platform_exec(plan: &HelperPlan) -> Result<i32, HelperFailure> {
    windows::exec(plan)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn platform_exec(_plan: &HelperPlan) -> Result<i32, HelperFailure> {
    Err(HelperFailure::new(
        "platform",
        "no sandbox backend exists for this operating system",
    ))
}
