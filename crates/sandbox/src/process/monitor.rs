//! Thread bodies that launch, supervise, and drain a sandboxed process tree.

use std::{
    io::{self, Read, Write},
    process::{ChildStderr, ChildStdin, ChildStdout, ExitStatus},
    sync::{Arc, mpsc::SyncSender},
    time::{Duration, Instant},
};

use super::{ExitStatusInfo, KillReason, ProcessShared, Stream};
use crate::{
    SandboxError,
    backend::{self, WATCHDOG_INTERVAL, Watchdog},
    helper::protocol::{self, FailureSniff, HELPER_FAILURE_EXIT, READY_FRAME},
};

/// How long the readers may keep draining after the tree was killed.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
/// Bound on the stderr prefix inspected for a helper failure line.
const MAX_SNIFF_BYTES: usize = 4096;

/// Pipes handed from the monitor thread back to `Sandbox::spawn`.
pub(crate) struct LaunchedPipes {
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

/// Launch the helper and own it until it is reaped.
///
/// The launch happens on this long-lived thread on purpose: on Linux the helper's
/// parent-death signal fires when the *thread* that forked it exits.
pub(crate) fn run_monitor(
    shared: Arc<ProcessShared>,
    launched: SyncSender<Result<LaunchedPipes, SandboxError>>,
) {
    let mut child = match backend::launch(&shared.plan) {
        Ok(launch) => {
            let _ = shared.tree.set(launch.tree);
            launch.child
        }
        Err(error) => {
            shared.finish(Err(error.clone()));
            shared.registry.remove(&shared.id);
            let _ = launched.send(Err(error));
            return;
        }
    };
    let _ = shared.root_pid.set(child.id());
    let pipes = LaunchedPipes {
        stdin: child.stdin.take(),
        stdout: child.stdout.take(),
        stderr: child.stderr.take(),
    };
    let delivered = launched.send(Ok(pipes)).is_ok();
    drop(launched);
    // Honour a kill requested while the helper was starting (or a vanished caller).
    if !delivered || shared.kill_requested() {
        let _ = shared.kill_tree_now();
    }
    let status = child.wait();
    // The managed program is over: nothing it left behind may keep running.
    let _ = shared.kill_tree_now();
    let complete = shared.wait_streams(DRAIN_TIMEOUT);
    let result = exit_result(&shared, status, complete);
    shared.finish(result);
    shared.registry.remove(&shared.id);
}

fn exit_result(
    shared: &ProcessShared,
    status: io::Result<ExitStatus>,
    output_complete: bool,
) -> Result<ExitStatusInfo, SandboxError> {
    let status = status.map_err(|error| SandboxError::io("waiting for sandbox helper", &error))?;
    let code = status.code();
    if code == Some(HELPER_FAILURE_EXIT) {
        if let Some((stage, message)) = shared.setup_failure() {
            return Err(SandboxError::EnforcementFailed { stage, message });
        }
    }
    Ok(ExitStatusInfo {
        code,
        signal: exit_signal(&status),
        killed_by: shared.kill_reason(),
        elapsed: shared.started.elapsed(),
        output_complete,
    })
}

#[cfg(unix)]
fn exit_signal(status: &ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn exit_signal(_status: &ExitStatus) -> Option<i32> {
    None
}

/// Enforce the wall timeout and watchdog limits until the tree finishes.
pub(crate) fn run_supervisor(
    shared: Arc<ProcessShared>,
    deadline: Option<Instant>,
    mut watchdog: Option<Watchdog>,
) {
    loop {
        let now = Instant::now();
        if deadline.is_some_and(|deadline| now >= deadline) {
            let _ = shared.kill(KillReason::WallTimeout);
            return;
        }
        let mut wait = if watchdog.is_some() {
            WATCHDOG_INTERVAL
        } else {
            Duration::from_secs(3600)
        };
        if let Some(deadline) = deadline {
            wait = wait.min(deadline.saturating_duration_since(now));
        }
        if shared.wait_finished(Some(wait)).is_some() {
            return;
        }
        if let (Some(watchdog), Some(tree)) = (watchdog.as_mut(), shared.tree.get()) {
            if let Some(requirement) = watchdog.check(tree) {
                let _ = shared.kill(KillReason::LimitExceeded(requirement));
                return;
            }
        }
    }
}

/// Write the framed plan, then the target's stdin, then close the pipe.
pub(crate) fn run_stdin_writer(mut pipe: ChildStdin, frame: Vec<u8>, input: Option<Vec<u8>>) {
    if pipe.write_all(&frame).and_then(|()| pipe.flush()).is_err() {
        return;
    }
    if let Some(input) = input {
        let _ = pipe.write_all(&input);
    }
}

/// Confirm the helper's ready frame, then capture stdout.
pub(crate) fn run_stdout_reader(
    shared: Arc<ProcessShared>,
    mut pipe: ChildStdout,
    ready: SyncSender<bool>,
) {
    let mut frame = vec![0_u8; READY_FRAME.len()];
    let confirmed = pipe.read_exact(&mut frame).is_ok() && frame == READY_FRAME;
    let _ = ready.send(confirmed);
    if confirmed {
        pump(&shared, &mut pipe, Stream::Stdout);
    } else {
        drain(&mut pipe);
    }
    shared.stream_closed();
}

/// Capture stderr, extracting a leading helper failure line.
pub(crate) fn run_stderr_reader(shared: Arc<ProcessShared>, mut pipe: ChildStderr) {
    let mut pending = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut sniffing = true;
    loop {
        let read = match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        if !sniffing {
            shared.push_output(Stream::Stderr, &buffer[..read]);
            continue;
        }
        pending.extend_from_slice(&buffer[..read]);
        match protocol::sniff_failure(&pending, false) {
            FailureSniff::Undecided if pending.len() < MAX_SNIFF_BYTES => {}
            FailureSniff::Failure {
                stage,
                message,
                rest,
            } => {
                shared.set_setup_failure(stage, message);
                shared.push_output(Stream::Stderr, &rest);
                sniffing = false;
            }
            FailureSniff::Undecided | FailureSniff::NotFailure => {
                shared.push_output(Stream::Stderr, &pending);
                pending.clear();
                sniffing = false;
            }
        }
    }
    if sniffing && !pending.is_empty() {
        match protocol::sniff_failure(&pending, true) {
            FailureSniff::Failure {
                stage,
                message,
                rest,
            } => {
                shared.set_setup_failure(stage, message);
                shared.push_output(Stream::Stderr, &rest);
            }
            FailureSniff::Undecided | FailureSniff::NotFailure => {
                shared.push_output(Stream::Stderr, &pending);
            }
        }
    }
    shared.stream_closed();
}

fn pump(shared: &ProcessShared, pipe: &mut impl Read, stream: Stream) {
    let mut buffer = [0_u8; 8192];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => shared.push_output(stream, &buffer[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

fn drain(pipe: &mut impl Read) {
    let mut buffer = [0_u8; 8192];
    while matches!(pipe.read(&mut buffer), Ok(read) if read > 0) {}
}
