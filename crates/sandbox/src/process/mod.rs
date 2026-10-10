//! Managed process handles, exit reporting, and the state shared with monitor threads.

pub(crate) mod monitor;
mod output;

use std::{
    io::Write,
    process::ChildStdin,
    sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError},
    time::{Duration, Instant},
};

use aidex_core::EntityId;
use serde::Serialize;

pub use output::{CapturedOutput, StreamCapture};
pub(crate) use output::{Stream, StreamBuffer};

use crate::{
    EnforcementPlan, ProcessRegistry, Requirement, SandboxError, backend::TreeHandle,
    request::serialize_duration_ms,
};

/// Why AiDEX terminated a process tree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "reason", content = "detail", rename_all = "snake_case")]
pub enum KillReason {
    /// [`ManagedProcess::kill_tree`] was called.
    Requested,
    /// The [`ManagedProcess`] handle was dropped while the tree was running.
    Dropped,
    /// The wall-clock timeout elapsed.
    WallTimeout,
    /// A watchdog-enforced resource limit was exceeded.
    LimitExceeded(Requirement),
    /// [`ProcessRegistry::terminate_all`] stopped every managed process.
    Stopped(StopReason),
    /// The helper did not confirm that the restrictions were applied.
    HandshakeFailed,
}

/// Why every managed process is being stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Cancelled,
    DaemonShutdown,
    EmergencyStop,
    DriveRemoved,
}

/// How a managed process tree ended.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExitStatusInfo {
    /// Exit code of the launched program (Windows: relayed by the helper).
    pub code: Option<i32>,
    /// Terminating signal on Unix.
    pub signal: Option<i32>,
    /// Set when AiDEX killed the tree.
    pub killed_by: Option<KillReason>,
    /// Time from launch until the tree ended.
    #[serde(rename = "elapsed_ms", serialize_with = "serialize_duration_ms")]
    pub elapsed: Duration,
    /// False when a descendant kept the output pipes open after the tree was killed
    /// (possible only for processes that escaped the process group on Unix).
    pub output_complete: bool,
}

impl ExitStatusInfo {
    /// Exited with code 0 without being killed.
    pub fn success(&self) -> bool {
        self.killed_by.is_none() && self.code == Some(0)
    }
}

struct RunState {
    kill_reason: Option<KillReason>,
    finished: Option<Result<ExitStatusInfo, SandboxError>>,
    open_streams: u8,
    setup_failure: Option<(String, String)>,
}

struct OutputState {
    stdout: StreamBuffer,
    stderr: StreamBuffer,
}

/// State shared between a [`ManagedProcess`], its monitor threads, and the registry.
pub(crate) struct ProcessShared {
    pub(crate) id: EntityId,
    pub(crate) plan: Arc<EnforcementPlan>,
    pub(crate) started: Instant,
    pub(crate) tree: OnceLock<TreeHandle>,
    pub(crate) root_pid: OnceLock<u32>,
    pub(crate) registry: ProcessRegistry,
    state: Mutex<RunState>,
    cond: Condvar,
    output: Mutex<OutputState>,
    pub(crate) stdin: Mutex<Option<ChildStdin>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl ProcessShared {
    pub(crate) fn new(id: EntityId, plan: Arc<EnforcementPlan>, registry: ProcessRegistry) -> Self {
        let max = plan.limits.max_output_bytes;
        Self {
            id,
            plan,
            started: Instant::now(),
            tree: OnceLock::new(),
            root_pid: OnceLock::new(),
            registry,
            stdin: Mutex::new(None),
            state: Mutex::new(RunState {
                kill_reason: None,
                finished: None,
                open_streams: 2,
                setup_failure: None,
            }),
            cond: Condvar::new(),
            output: Mutex::new(OutputState {
                stdout: StreamBuffer::new(max),
                stderr: StreamBuffer::new(max),
            }),
        }
    }

    /// Record `reason` (the first reason wins) and kill the tree if it exists yet.
    pub(crate) fn kill(&self, reason: KillReason) -> Result<(), SandboxError> {
        {
            let mut state = lock(&self.state);
            if state.finished.is_some() {
                return Ok(());
            }
            if state.kill_reason.is_none() {
                state.kill_reason = Some(reason);
            }
        }
        self.kill_tree_now()
    }

    pub(crate) fn kill_tree_now(&self) -> Result<(), SandboxError> {
        match self.tree.get() {
            Some(tree) => tree.kill(),
            // Still launching: the monitor kills the tree as soon as it exists.
            None => Ok(()),
        }
    }

    pub(crate) fn kill_requested(&self) -> bool {
        lock(&self.state).kill_reason.is_some()
    }

    pub(crate) fn kill_reason(&self) -> Option<KillReason> {
        lock(&self.state).kill_reason.clone()
    }

    pub(crate) fn is_finished(&self) -> bool {
        lock(&self.state).finished.is_some()
    }

    pub(crate) fn finish(&self, result: Result<ExitStatusInfo, SandboxError>) {
        lock(&self.stdin).take();
        let mut state = lock(&self.state);
        if state.finished.is_none() {
            state.finished = Some(result);
        }
        self.cond.notify_all();
    }

    /// Wait until finished or `timeout` elapses (`None` waits forever).
    pub(crate) fn wait_finished(
        &self,
        timeout: Option<Duration>,
    ) -> Option<Result<ExitStatusInfo, SandboxError>> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let mut state = lock(&self.state);
        loop {
            if let Some(result) = &state.finished {
                return Some(result.clone());
            }
            match deadline {
                None => {
                    state = self
                        .cond
                        .wait(state)
                        .unwrap_or_else(PoisonError::into_inner)
                }
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return None;
                    }
                    state = self
                        .cond
                        .wait_timeout(state, deadline - now)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            }
        }
    }

    pub(crate) fn stream_closed(&self) {
        let mut state = lock(&self.state);
        state.open_streams = state.open_streams.saturating_sub(1);
        self.cond.notify_all();
    }

    /// Wait for both output readers to reach end-of-file; false on timeout.
    pub(crate) fn wait_streams(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = lock(&self.state);
        while state.open_streams > 0 {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            state = self
                .cond
                .wait_timeout(state, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }

    pub(crate) fn set_setup_failure(&self, stage: String, message: String) {
        lock(&self.state).setup_failure = Some((stage, message));
    }

    pub(crate) fn setup_failure(&self) -> Option<(String, String)> {
        lock(&self.state).setup_failure.clone()
    }

    pub(crate) fn push_output(&self, stream: Stream, bytes: &[u8]) {
        let mut output = lock(&self.output);
        match stream {
            Stream::Stdout => output.stdout.push(bytes),
            Stream::Stderr => output.stderr.push(bytes),
        }
    }

    fn output_snapshot(&self) -> CapturedOutput {
        let output = lock(&self.output);
        CapturedOutput {
            stdout: output.stdout.snapshot(),
            stderr: output.stderr.snapshot(),
        }
    }

    fn take_output(&self) -> CapturedOutput {
        let mut output = lock(&self.output);
        CapturedOutput {
            stdout: output.stdout.take(),
            stderr: output.stderr.take(),
        }
    }
}

/// A running (or finished) sandboxed process tree. Dropping it kills the tree.
pub struct ManagedProcess {
    pub(crate) shared: Arc<ProcessShared>,
}

impl ManagedProcess {
    /// Write to an explicitly interactive process. Never logs supplied bytes.
    pub fn write_stdin(&self, bytes: &[u8]) -> Result<(), SandboxError> {
        if !self.is_running() {
            return Err(SandboxError::Io("process input is closed".into()));
        }
        let mut guard = lock(&self.shared.stdin);
        let pipe = guard
            .as_mut()
            .ok_or_else(|| SandboxError::Io("process input is closed".into()))?;
        pipe.write_all(bytes)
            .and_then(|()| pipe.flush())
            .map_err(|_| SandboxError::Io("writing process input failed".into()))
    }
    /// End the interactive stream; fixed-input processes already have closed stdin.
    pub fn close_stdin(&self) {
        lock(&self.shared.stdin).take();
    }
    /// Registry identifier (`proc_<ULID>`).
    pub fn id(&self) -> &EntityId {
        &self.shared.id
    }

    /// Process id of the tree root: the target itself on Unix, the helper that owns the
    /// target on Windows.
    pub fn pid(&self) -> u32 {
        self.shared.root_pid.get().copied().unwrap_or(0)
    }

    /// The plan this process runs under.
    pub fn plan(&self) -> &EnforcementPlan {
        &self.shared.plan
    }

    /// Wait up to `timeout`; `Ok(None)` means still running.
    pub fn wait_timeout(&self, timeout: Duration) -> Result<Option<ExitStatusInfo>, SandboxError> {
        self.shared.wait_finished(Some(timeout)).transpose()
    }

    /// Wait for the whole tree to end.
    pub fn wait(&self) -> Result<ExitStatusInfo, SandboxError> {
        match self.shared.wait_finished(None) {
            Some(result) => result,
            None => Err(SandboxError::Io("wait ended without a result".into())),
        }
    }

    /// Non-blocking status check.
    pub fn try_wait(&self) -> Result<Option<ExitStatusInfo>, SandboxError> {
        self.wait_timeout(Duration::ZERO)
    }

    /// Whether the tree is still running.
    pub fn is_running(&self) -> bool {
        !self.shared.is_finished()
    }

    /// Kill the process and every descendant in its container.
    pub fn kill_tree(&self) -> Result<(), SandboxError> {
        self.shared.kill(KillReason::Requested)
    }

    /// Copy of the output captured so far.
    pub fn output_so_far(&self) -> CapturedOutput {
        self.shared.output_snapshot()
    }

    /// Take the buffered output, freeing room for more.
    pub fn take_output(&self) -> CapturedOutput {
        self.shared.take_output()
    }

    /// Process ids currently in the tree's container (job members on Windows, process
    /// group members on Linux). Empty once the tree has been terminated.
    pub fn member_pids(&self) -> Result<Vec<u32>, SandboxError> {
        match self.shared.tree.get() {
            Some(tree) => tree.members(),
            None => Ok(Vec::new()),
        }
    }
}

impl std::fmt::Debug for ManagedProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedProcess")
            .field("id", &self.shared.id)
            .field("pid", &self.pid())
            .field("running", &self.is_running())
            .finish()
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        if !self.shared.is_finished() {
            let _ = self.shared.kill(KillReason::Dropped);
        }
    }
}
