//! Registry of every live managed process, for cancel, daemon shutdown, emergency stop,
//! and drive removal.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
    time::{Duration, Instant},
};

use aidex_core::EntityId;
use serde::Serialize;

use crate::{
    SandboxError,
    process::{KillReason, ProcessShared, StopReason},
};

/// Default time `terminate_all` waits for each tree to be confirmed dead.
pub const DEFAULT_TERMINATION_GRACE: Duration = Duration::from_secs(5);

/// A process that could not be confirmed terminated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TerminationFailure {
    pub id: EntityId,
    pub message: String,
}

/// Outcome of [`ProcessRegistry::terminate_all`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TerminationReport {
    pub reason: StopReason,
    /// Killed by this call and confirmed exited.
    pub terminated: Vec<EntityId>,
    /// Already finished (or finishing on its own) before this call.
    pub already_exited: Vec<EntityId>,
    /// Kill failed or exit was not confirmed within the grace period.
    pub failed: Vec<TerminationFailure>,
}

impl TerminationReport {
    /// True when nothing is left running.
    pub fn is_clean(&self) -> bool {
        self.failed.is_empty()
    }
}

#[derive(Default)]
struct RegistryState {
    closed: bool,
    processes: BTreeMap<EntityId, Weak<ProcessShared>>,
}

/// Shared registry of live managed processes. Cloning shares the same registry.
#[derive(Clone, Default)]
pub struct ProcessRegistry {
    inner: Arc<Mutex<RegistryState>>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, RegistryState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn register(&self, process: &Arc<ProcessShared>) -> Result<(), SandboxError> {
        let mut state = self.state();
        if state.closed {
            return Err(SandboxError::RegistryClosed);
        }
        state
            .processes
            .insert(process.id.clone(), Arc::downgrade(process));
        Ok(())
    }

    pub(crate) fn remove(&self, id: &EntityId) {
        self.state().processes.remove(id);
    }

    fn live(&self) -> Vec<Arc<ProcessShared>> {
        let mut state = self.state();
        state
            .processes
            .retain(|_, process| process.upgrade().is_some_and(|p| !p.is_finished()));
        state.processes.values().filter_map(Weak::upgrade).collect()
    }

    /// Identifiers of every live managed process.
    pub fn live_ids(&self) -> Vec<EntityId> {
        self.live()
            .iter()
            .map(|process| process.id.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.live().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether new processes may start.
    pub fn is_accepting(&self) -> bool {
        !self.state().closed
    }

    /// Refuse new processes (emergency stop, shutdown, drive removal).
    pub fn close(&self) {
        self.state().closed = true;
    }

    /// Accept new processes again after a stop was cleared.
    pub fn reopen(&self) {
        self.state().closed = false;
    }

    /// Kill every live tree and confirm each exit within [`DEFAULT_TERMINATION_GRACE`].
    pub fn terminate_all(&self, reason: StopReason) -> TerminationReport {
        self.terminate_all_within(reason, DEFAULT_TERMINATION_GRACE)
    }

    /// Kill every live tree and confirm each exit within `grace`.
    pub fn terminate_all_within(&self, reason: StopReason, grace: Duration) -> TerminationReport {
        let mut report = TerminationReport {
            reason,
            terminated: Vec::new(),
            already_exited: Vec::new(),
            failed: Vec::new(),
        };
        let mut killed = Vec::new();
        for process in self.live() {
            if process.is_finished() {
                report.already_exited.push(process.id.clone());
                continue;
            }
            match process.kill(KillReason::Stopped(reason)) {
                Ok(()) => killed.push(process),
                Err(error) => report.failed.push(TerminationFailure {
                    id: process.id.clone(),
                    message: error.to_string(),
                }),
            }
        }
        let deadline = Instant::now() + grace;
        for process in killed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match process.wait_finished(Some(remaining)) {
                Some(Ok(status)) if status.killed_by == Some(KillReason::Stopped(reason)) => {
                    report.terminated.push(process.id.clone());
                }
                Some(Ok(_)) => report.already_exited.push(process.id.clone()),
                Some(Err(error)) => report.failed.push(TerminationFailure {
                    id: process.id.clone(),
                    message: error.to_string(),
                }),
                None => report.failed.push(TerminationFailure {
                    id: process.id.clone(),
                    message: format!("did not exit within {} ms", grace.as_millis()),
                }),
            }
        }
        report
    }

    /// Close the registry and terminate everything (daemon shutdown, emergency stop,
    /// drive removal).
    pub fn shutdown(&self, reason: StopReason) -> TerminationReport {
        self.close();
        self.terminate_all(reason)
    }
}

impl std::fmt::Debug for ProcessRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state();
        f.debug_struct("ProcessRegistry")
            .field("closed", &state.closed)
            .field("tracked", &state.processes.len())
            .finish()
    }
}
