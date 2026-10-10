//! Routing outcomes: the chosen model, ordered fallbacks, and explanations.

use std::fmt;

use aidex_core::ServiceErrorCode;
use aidex_providers::{ModelRef, ModelRole};
use serde::{Deserialize, Serialize};

use crate::{PrivacyMode, RemoteBlock, TaskKind};

/// Why a candidate model was not eligible.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum RejectReason {
    /// A remote model while privacy, network, or secrets require local models.
    RemoteForbidden {
        cause: RemoteBlock,
    },
    /// The task needs embeddings (or chat) and the model does not provide them.
    WrongCapability {
        needed: String,
    },
    ContextTooSmall {
        window: u32,
        required: u32,
    },
    NoToolSupport,
    NoJsonSchema,
    InsufficientMemory {
        required: u64,
        available: u64,
    },
    /// The model's limits are assumed defaults, not configured or reported; it
    /// is only used when the user selects it explicitly.
    UnverifiedCapabilities,
}

impl fmt::Display for RejectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RemoteForbidden { cause } => f.write_str(cause.describe()),
            Self::WrongCapability { needed } => write!(f, "does not support {needed}"),
            Self::ContextTooSmall { window, required } => write!(
                f,
                "context window {window} tokens is smaller than the required {required}"
            ),
            Self::NoToolSupport => f.write_str("does not support tool calls"),
            Self::NoJsonSchema => f.write_str("does not support JSON-schema output"),
            Self::InsufficientMemory {
                required,
                available,
            } => write!(
                f,
                "needs {} MiB of RAM; the host has {} MiB",
                required / (1024 * 1024),
                available / (1024 * 1024)
            ),
            Self::UnverifiedCapabilities => f.write_str(
                "limits are assumed defaults (declare the model in providers.toml or select it explicitly)",
            ),
        }
    }
}

/// A rejected candidate; serialized flat as `{model, reason, ..., explanation}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Rejection {
    pub model: ModelRef,
    #[serde(flatten)]
    pub reason: RejectReason,
    /// Human-readable form of `reason`.
    pub explanation: String,
}

impl Rejection {
    pub fn new(model: ModelRef, reason: RejectReason) -> Self {
        Self {
            explanation: reason.to_string(),
            model,
            reason,
        }
    }
}

/// The router's choice for one request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDecision {
    pub task: TaskKind,
    pub privacy: PrivacyMode,
    pub primary: ModelRef,
    /// Eligible alternatives in order; empty for a user override.
    pub fallbacks: Vec<ModelRef>,
    /// Whether remote models may be used at all for this request.
    /// [`crate::run_with_fallback`] re-checks every candidate against it.
    pub allow_remote: bool,
    pub preferred_role: ModelRole,
    /// Why the primary was chosen and which constraints applied.
    pub reasons: Vec<String>,
    pub rejected: Vec<Rejection>,
}

impl RouteDecision {
    /// Primary first, then fallbacks.
    pub fn candidates(&self) -> impl Iterator<Item = &ModelRef> {
        std::iter::once(&self.primary).chain(&self.fallbacks)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteError {
    /// No model satisfies the request's constraints.
    NoEligibleModel {
        reasons: Vec<String>,
        rejected: Vec<Rejection>,
    },
    /// The user's explicit choice violates a hard constraint.
    OverrideRejected { rejection: Rejection },
    /// The user's explicit choice is not offered by any provider.
    OverrideNotFound { model: ModelRef },
}

impl RouteError {
    pub const fn code(&self) -> ServiceErrorCode {
        match self {
            Self::NoEligibleModel { .. } => ServiceErrorCode::Unavailable,
            Self::OverrideRejected { .. } => ServiceErrorCode::PermissionDenied,
            Self::OverrideNotFound { .. } => ServiceErrorCode::NotFound,
        }
    }

    pub fn rejected(&self) -> &[Rejection] {
        match self {
            Self::NoEligibleModel { rejected, .. } => rejected,
            Self::OverrideRejected { rejection } => std::slice::from_ref(rejection),
            Self::OverrideNotFound { .. } => &[],
        }
    }
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEligibleModel { rejected, .. } if rejected.is_empty() => {
                f.write_str("no models are available")
            }
            Self::NoEligibleModel { rejected, .. } => write!(
                f,
                "no eligible model: {} candidate(s) rejected",
                rejected.len()
            ),
            Self::OverrideRejected { rejection } => write!(
                f,
                "requested model {} cannot be used: {}",
                rejection.model, rejection.explanation
            ),
            Self::OverrideNotFound { model } => {
                write!(f, "requested model {model} is not offered by any provider")
            }
        }
    }
}

impl std::error::Error for RouteError {}
