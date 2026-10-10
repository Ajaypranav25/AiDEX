//! Resident-model scheduling (spec §97): at most one large model at a time, a
//! bounded number of small ones, least-recently-used eviction of idle models,
//! and no eviction of a model that is serving a request.

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use aidex_core::Timestamp;
use serde::{Deserialize, Serialize};

use crate::{LaunchPlan, LocalModelId, server::ManagedServer, server::ServerInfo};

/// Residency limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerPolicy {
    /// Models whose file is at least this large count as large.
    pub large_model_bytes: u64,
    /// Small models resident at once.
    pub max_small_models: usize,
}

impl Default for SchedulerPolicy {
    fn default() -> Self {
        Self {
            large_model_bytes: 2 * 1024 * 1024 * 1024,
            max_small_models: 2,
        }
    }
}

/// A model with a running server.
pub(crate) struct LoadedModel {
    pub id: LocalModelId,
    pub server: ManagedServer,
    pub plan: LaunchPlan,
    pub large: bool,
    pub loaded_at: Timestamp,
    in_flight: AtomicUsize,
    last_used: Mutex<Instant>,
}

impl LoadedModel {
    pub fn new(id: LocalModelId, server: ManagedServer, plan: LaunchPlan, large: bool) -> Self {
        Self {
            id,
            server,
            plan,
            large,
            loaded_at: Timestamp::now(),
            in_flight: AtomicUsize::new(0),
            last_used: Mutex::new(Instant::now()),
        }
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    pub fn idle_for(&self) -> Duration {
        self.last_used
            .lock()
            .map(|used| used.elapsed())
            .unwrap_or_default()
    }

    fn touch(&self) {
        if let Ok(mut used) = self.last_used.lock() {
            *used = Instant::now();
        }
    }

    pub fn summary(&self) -> LoadedSummary {
        LoadedSummary {
            model: self.id.clone(),
            large: self.large,
            in_flight: self.in_flight(),
            idle_ms: u64::try_from(self.idle_for().as_millis()).unwrap_or(u64::MAX),
            loaded_at: self.loaded_at.clone(),
            plan: self.plan.clone(),
            server: self.server.info(),
        }
    }
}

/// A resident model as reported to clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadedSummary {
    pub model: LocalModelId,
    pub large: bool,
    pub in_flight: usize,
    pub idle_ms: u64,
    pub loaded_at: Timestamp,
    pub plan: LaunchPlan,
    pub server: ServerInfo,
}

pub(crate) enum Slot {
    Loading { large: bool },
    Ready(Arc<LoadedModel>),
}

/// The resident set, guarded by [`Residency::lock`].
#[derive(Default)]
pub(crate) struct Resident {
    pub models: BTreeMap<LocalModelId, Slot>,
    pub closed: bool,
}

/// What a caller asking for `id` must do.
pub(crate) enum Admission {
    Ready(Arc<LoadedModel>),
    /// Something blocks the load (another large model is busy or loading).
    Wait(String),
    /// Load now after evicting these idle (or dead) models.
    Load {
        evict: Vec<LocalModelId>,
    },
}

impl Resident {
    pub fn admit(&self, id: &LocalModelId, large: bool, policy: &SchedulerPolicy) -> Admission {
        let mut evict = Vec::new();
        match self.models.get(id) {
            Some(Slot::Loading { .. }) => return Admission::Wait(format!("`{id}` is loading")),
            Some(Slot::Ready(model)) if model.server.is_running() => {
                return Admission::Ready(Arc::clone(model));
            }
            Some(Slot::Ready(_)) => evict.push(id.clone()),
            None => {}
        }
        let others = self.models.iter().filter(|(other, _)| *other != id);
        if large {
            for (other, slot) in others {
                match slot {
                    Slot::Loading { large: true } => {
                        return Admission::Wait(format!("large model `{other}` is loading"));
                    }
                    Slot::Ready(model) if model.large && model.server.is_running() => {
                        if model.in_flight() > 0 {
                            return Admission::Wait(format!(
                                "large model `{other}` is serving a request"
                            ));
                        }
                        evict.push(other.clone());
                    }
                    Slot::Ready(model) if !model.server.is_running() => evict.push(other.clone()),
                    _ => {}
                }
            }
        } else {
            let mut small: Vec<(&LocalModelId, Option<&Arc<LoadedModel>>)> = others
                .filter_map(|(other, slot)| match slot {
                    Slot::Loading { large: false } => Some((other, None)),
                    Slot::Ready(model) if !model.large && model.server.is_running() => {
                        Some((other, Some(model)))
                    }
                    Slot::Ready(model) if !model.server.is_running() => {
                        evict.push(other.clone());
                        None
                    }
                    _ => None,
                })
                .collect();
            // Least recently used first.
            small.sort_by_key(|(_, model)| {
                std::cmp::Reverse(model.map_or(Duration::ZERO, |model| model.idle_for()))
            });
            while small.len() >= policy.max_small_models.max(1) {
                let Some(index) = small
                    .iter()
                    .position(|(_, model)| model.is_some_and(|model| model.in_flight() == 0))
                else {
                    return Admission::Wait("every small-model slot is busy".into());
                };
                let (other, _) = small.remove(index);
                evict.push(other.clone());
            }
        }
        Admission::Load { evict }
    }

    /// Remove idle models unused for `timeout`.
    pub fn take_idle(&mut self, timeout: Duration) -> Vec<Arc<LoadedModel>> {
        let idle: Vec<LocalModelId> = self
            .models
            .iter()
            .filter_map(|(id, slot)| match slot {
                Slot::Ready(model)
                    if model.in_flight() == 0
                        && (model.idle_for() >= timeout || !model.server.is_running()) =>
                {
                    Some(id.clone())
                }
                _ => None,
            })
            .collect();
        idle.iter().filter_map(|id| self.take_ready(id)).collect()
    }

    pub fn take_ready(&mut self, id: &LocalModelId) -> Option<Arc<LoadedModel>> {
        match self.models.get(id) {
            Some(Slot::Ready(_)) => match self.models.remove(id) {
                Some(Slot::Ready(model)) => Some(model),
                _ => None,
            },
            _ => None,
        }
    }
}

/// The resident set with its change notification.
#[derive(Default)]
pub(crate) struct Residency {
    state: Mutex<Resident>,
    changed: Condvar,
}

impl Residency {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Resident> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn wait<'a>(
        &self,
        guard: std::sync::MutexGuard<'a, Resident>,
        timeout: Duration,
    ) -> std::sync::MutexGuard<'a, Resident> {
        match self.changed.wait_timeout(guard, timeout) {
            Ok((guard, _)) => guard,
            Err(poisoned) => poisoned.into_inner().0,
        }
    }

    pub fn notify(&self) {
        self.changed.notify_all();
    }
}

/// One request's hold on a resident model; dropping it marks the model idle.
pub(crate) struct Lease {
    model: Arc<LoadedModel>,
    residency: Arc<Residency>,
}

impl Lease {
    /// Count a new request on `model` (call with the residency lock held).
    pub fn new(model: Arc<LoadedModel>, residency: Arc<Residency>) -> Self {
        model.in_flight.fetch_add(1, Ordering::SeqCst);
        model.touch();
        Self { model, residency }
    }

    pub fn model(&self) -> &Arc<LoadedModel> {
        &self.model
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.model.touch();
        self.model.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.residency.notify();
    }
}
