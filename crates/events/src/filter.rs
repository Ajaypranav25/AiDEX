//! Replay and subscription filters: scope equality plus event-name prefixes.

use aidex_core::EntityId;
use serde::{Deserialize, Serialize};

use crate::{EVENT_NAMES, EventEnvelope, EventError};

/// Most kind prefixes one filter may carry.
pub const MAX_FILTER_KINDS: usize = 32;
const MAX_PREFIX_BYTES: usize = 64;

/// Selects events by scope and name.
///
/// - `workspace_id` / `session_id`: when set, only events whose scope has exactly that
///   id match (global events do not match a workspace filter).
/// - `kinds`: dotted name prefixes matched on segment boundaries; `tool` matches
///   `tool.requested`, `agent.subagent` matches `agent.subagent.created`, and an exact
///   name matches itself. Empty means every kind. Each prefix must match at least one
///   catalog name, so typos are rejected instead of silently matching nothing.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
}

impl EventFilter {
    /// Every event.
    pub fn all() -> Self {
        Self::default()
    }

    pub fn for_workspace(workspace_id: EntityId) -> Self {
        Self {
            workspace_id: Some(workspace_id),
            ..Self::default()
        }
    }

    pub fn for_session(session_id: EntityId) -> Self {
        Self {
            session_id: Some(session_id),
            ..Self::default()
        }
    }

    /// Add a kind prefix.
    #[must_use]
    pub fn with_kind(mut self, prefix: impl Into<String>) -> Self {
        self.kinds.push(prefix.into());
        self
    }

    pub fn validate(&self) -> Result<(), EventError> {
        if self.kinds.len() > MAX_FILTER_KINDS {
            return Err(EventError::InvalidFilter(format!(
                "at most {MAX_FILTER_KINDS} kind prefixes are allowed"
            )));
        }
        for prefix in &self.kinds {
            let well_formed = !prefix.is_empty()
                && prefix.len() <= MAX_PREFIX_BYTES
                && prefix.split('.').all(|segment| {
                    !segment.is_empty()
                        && segment.bytes().all(|byte| {
                            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                        })
                });
            if !well_formed {
                return Err(EventError::InvalidFilter(format!(
                    "kind prefix `{}` must be dotted lowercase segments",
                    prefix.escape_debug()
                )));
            }
            if !EVENT_NAMES.iter().any(|name| kind_matches(prefix, name)) {
                return Err(EventError::InvalidFilter(format!(
                    "kind prefix `{prefix}` matches no event in the catalog"
                )));
            }
        }
        Ok(())
    }

    /// Whether `envelope` passes this filter (the same rule replay applies in SQL).
    pub fn matches(&self, envelope: &EventEnvelope) -> bool {
        let scope = &envelope.scope;
        if self
            .workspace_id
            .as_ref()
            .is_some_and(|id| scope.workspace_id.as_ref() != Some(id))
        {
            return false;
        }
        if self
            .session_id
            .as_ref()
            .is_some_and(|id| scope.session_id.as_ref() != Some(id))
        {
            return false;
        }
        self.matches_kind(envelope.name())
    }

    pub fn matches_kind(&self, name: &str) -> bool {
        self.kinds.is_empty() || self.kinds.iter().any(|prefix| kind_matches(prefix, name))
    }
}

/// `prefix` equals `name` or is a whole-segment prefix of it.
pub fn kind_matches(prefix: &str, name: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}
