//! Tool results.

use aidex_core::WorkspacePath;
use aidex_events::FileChange;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{DiffPreview, ToolError, schema, text};

/// Longest output summary in bytes (after redaction).
pub const MAX_OUTPUT_SUMMARY_BYTES: usize = 512;

/// One workspace path a tool changed; the registry emits `file.changed` for it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PathChange {
    pub path: WorkspacePath,
    pub change: FileChange,
}

/// The result of executing a plan.
///
/// `content` is the tool's typed output serialized to JSON (it conforms to the
/// descriptor's `output_schema`). Tool output is untrusted data: workspace content
/// and process output must never be treated as instructions.
#[derive(Clone, Debug, Serialize)]
pub struct ToolOutput {
    /// Short, redacted, bounded summary (used for `tool.completed`).
    pub summary: String,
    pub content: Value,
    /// The diff that was applied (writes only).
    pub diff: Option<DiffPreview>,
    /// Paths that changed (writes only).
    pub changes: Vec<PathChange>,
    /// Non-fatal problems, such as an audit event that could not be recorded after
    /// the effect had already happened.
    pub warnings: Vec<String>,
}

impl ToolOutput {
    /// Output from a typed result, validated to round-trip through its own type.
    pub fn typed<T: Serialize + DeserializeOwned>(
        summary: impl AsRef<str>,
        output: &T,
    ) -> Result<Self, ToolError> {
        Ok(Self {
            summary: text::bounded(summary.as_ref(), MAX_OUTPUT_SUMMARY_BYTES),
            content: schema::output_value(output)?,
            diff: None,
            changes: Vec::new(),
            warnings: Vec::new(),
        })
    }

    pub fn with_diff(mut self, diff: DiffPreview) -> Self {
        self.diff = Some(diff);
        self
    }

    pub fn with_changes(mut self, changes: Vec<PathChange>) -> Self {
        self.changes = changes;
        self
    }

    /// Deserialize `content` into a typed value.
    pub fn parse<T: DeserializeOwned>(&self) -> Result<T, ToolError> {
        serde_json::from_value(self.content.clone())
            .map_err(|error| ToolError::InvalidOutput(error.to_string()))
    }
}
