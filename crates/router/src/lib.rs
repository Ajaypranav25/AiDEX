//! Model routing for AiDEX (spec §36): choose a model for a task from the
//! models providers offer, honouring privacy mode, network state, secrets,
//! context/tool/JSON-schema requirements, host RAM, and user overrides, and
//! run a chat with explicit fallback events.
//!
//! The router never routes to a remote model when offline, air-gapped,
//! local-only, or secret-heavy, and never silently swaps a user's explicit
//! choice: an override that violates a constraint is an error.

mod decision;
mod fallback;
mod request;
mod select;

pub use decision::{RejectReason, Rejection, RouteDecision, RouteError};
pub use fallback::{ProviderLookup, is_fallback_eligible, run_with_fallback};
pub use request::{HostSummary, NetworkState, PrivacyMode, RemoteBlock, RouteRequest, TaskKind};
pub use select::{DEFAULT_MAX_FALLBACKS, Router};
