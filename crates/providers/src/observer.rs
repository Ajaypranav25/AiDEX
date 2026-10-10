//! Model-domain notifications, adapted to typed events by the daemon.
//!
//! Events carry identifiers, kinds, and sanitized messages only; they never
//! carry credentials, prompts, or model output.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::{Locality, ModelRef, ProviderErrorKind, ProviderId, ProviderKind};

/// A candidate failed and the next one is being tried (`model.fallback`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackEvent {
    pub from: ModelRef,
    pub to: ModelRef,
    pub reason: ProviderErrorKind,
    /// The sanitized error message of the failed attempt.
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ModelEvent {
    #[serde(rename = "model.selected")]
    Selected {
        model: ModelRef,
        fallbacks: Vec<ModelRef>,
        reasons: Vec<String>,
    },
    #[serde(rename = "model.fallback")]
    Fallback(FallbackEvent),
    #[serde(rename = "model.default_changed")]
    DefaultChanged { model: Option<ModelRef> },
    #[serde(rename = "provider.added")]
    ProviderAdded {
        provider_id: ProviderId,
        provider_kind: ProviderKind,
        locality: Locality,
    },
    #[serde(rename = "provider.removed")]
    ProviderRemoved { provider_id: ProviderId },
    #[serde(rename = "network.mode_changed")]
    NetworkModeChanged { offline: bool },
}

impl ModelEvent {
    /// The dotted event name (`model.fallback`, ...).
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Selected { .. } => "model.selected",
            Self::Fallback(_) => "model.fallback",
            Self::DefaultChanged { .. } => "model.default_changed",
            Self::ProviderAdded { .. } => "provider.added",
            Self::ProviderRemoved { .. } => "provider.removed",
            Self::NetworkModeChanged { .. } => "network.mode_changed",
        }
    }
}

/// Receives model-domain events. Implementations must not block for long.
pub trait ModelObserver: Send + Sync {
    fn observe(&self, event: &ModelEvent);
}

/// Discards every event.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopObserver;

impl ModelObserver for NoopObserver {
    fn observe(&self, _event: &ModelEvent) {}
}

/// Keeps every event in memory (tests and diagnostics).
#[derive(Debug, Default)]
pub struct RecordingObserver {
    events: Mutex<Vec<ModelEvent>>,
}

impl RecordingObserver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<ModelEvent> {
        self.events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }
}

impl ModelObserver for RecordingObserver {
    fn observe(&self, event: &ModelEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event.clone());
        }
    }
}
